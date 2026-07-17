use std::{
    collections::{HashMap, HashSet},
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use futures_util::{future::BoxFuture, stream, StreamExt};
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use super::{
    adapters::responses::render_responses_response,
    endpoint_adapter::{response_headers_for_downstream, EndpointAdapter, ResponseMode},
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    observability::AttemptTrace,
    request::{ByteStream, CanonicalProxyRequest},
    router::{self, RichRouteCandidate, RouteRequest},
    routing_repository::{
        CandidateFeedback, CandidateFeedbackKind, FinalRequestOutcome, RoutingRepository,
    },
    upstream::{UpstreamAttempt, UpstreamClientPool},
};

use crate::{
    models::{
        pricing::BalanceSnapshot,
        routing::{RouteEndpointKind, RoutingPolicy},
    },
    services::database::now_millis_for_services,
};

#[derive(Clone)]
pub(crate) struct ExecutionEngine {
    repository: Arc<dyn RoutingRepository>,
    attempts: Arc<dyn AttemptExecutor>,
    retry_policy: RetryPolicy,
}

pub(crate) trait AttemptExecutor: Send + Sync {
    fn attempt<'a>(
        &'a self,
        request: &'a CanonicalProxyRequest,
        candidate: &'a RichRouteCandidate,
        mapped_model: Option<&'a str>,
    ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>>;
}

pub(crate) enum PreparedAttempt {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: ByteStream,
    },
}

pub(crate) struct ProxyExecutionResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ProxyExecutionBody,
    selected_station_key_id: Option<String>,
    selected_station_id: Option<String>,
    fallback_count: i64,
    pub final_outcome: FinalRequestOutcome,
}

pub(crate) enum ProxyExecutionBody {
    Buffered(Bytes),
    Stream(ByteStream),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryDecision {
    NextCandidate,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetryPolicy {
    max_candidate_attempts: usize,
    precommit_budget: Duration,
    first_byte_timeout: Duration,
    buffered_budget: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_candidate_attempts: 3,
            precommit_budget: Duration::from_secs(180),
            first_byte_timeout: Duration::from_secs(120),
            buffered_budget: Duration::from_secs(300),
        }
    }
}

impl ExecutionEngine {
    pub(crate) fn new(
        repository: Arc<dyn RoutingRepository>,
        attempts: Arc<dyn AttemptExecutor>,
    ) -> Self {
        Self {
            repository,
            attempts,
            retry_policy: RetryPolicy::default(),
        }
    }

    #[cfg(test)]
    fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub(crate) async fn execute(
        &self,
        request: CanonicalProxyRequest,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let precommit_started = Instant::now();
        if request.local_path == "/usage" || request.local_path == "/v1/usage" {
            return self.execute_usage(request).await;
        }

        let candidates = self
            .repository
            .load_runtime_candidates()
            .await
            .map_err(|error| internal_failure(format!("load route candidates failed: {error}")))?;
        let aliases = self
            .repository
            .load_model_alias_pairs()
            .await
            .map_err(|error| internal_failure(format!("load model aliases failed: {error}")))?;
        let selection =
            router::select_route_candidates(&route_request(&request), candidates, &aliases)
                .map_err(|error| {
                    internal_failure(format!("select route candidates failed: {error}"))
                })?;
        let candidates = selection.accepted;
        if candidates.is_empty() {
            return Err(ProxyFailure::new(
                ProxyFailureCode::RouteNoCandidate,
                FailureSource::Routing,
                RetryClass::Never,
                StatusCode::SERVICE_UNAVAILABLE,
                "no eligible route candidate",
            ));
        }

        if matches!(request.endpoint, RouteEndpointKind::Models) {
            return self
                .execute_models(request, candidates, selection.mapped_model)
                .await;
        }

        let idempotent = request.idempotency_key.is_some();
        let mut last_failure = None;
        let mut traces = Vec::new();
        for (attempt_index, candidate) in candidates
            .iter()
            .take(self.retry_policy.max_attempts(candidates.len()))
            .enumerate()
        {
            let Some(remaining) = self
                .retry_policy
                .remaining_precommit_budget(precommit_started)
            else {
                return Err(precommit_timeout_failure());
            };
            let attempt_result = tokio::time::timeout(remaining, async {
                match self
                    .attempts
                    .attempt(&request, candidate, selection.mapped_model.as_deref())
                    .await
                {
                    Ok(prepared) => self.bootstrap_stream(prepared).await,
                    Err(failure) => Err(failure),
                }
            })
            .await
            .unwrap_or_else(|_| Err(precommit_timeout_failure()));
            match attempt_result {
                Ok(prepared) => {
                    traces.push(AttemptTrace {
                        station_key_id: candidate.candidate.station_key_id.clone(),
                        failure_code: None,
                        duration_ms: 0,
                    });
                    return Ok(ProxyExecutionResponse::from_prepared(
                        prepared,
                        candidate,
                        attempt_index as i64,
                        &request,
                        traces,
                    ));
                }
                Err(failure) => {
                    traces.push(AttemptTrace {
                        station_key_id: candidate.candidate.station_key_id.clone(),
                        failure_code: Some(failure.code.as_str().to_string()),
                        duration_ms: 0,
                    });
                    let decision = self.retry_policy.decide(&failure, idempotent, false);
                    last_failure = Some(failure);
                    if decision == RetryDecision::Stop {
                        break;
                    }
                }
            }
        }

        Err(last_failure.unwrap_or_else(|| {
            ProxyFailure::new(
                ProxyFailureCode::RouteNoCandidate,
                FailureSource::Routing,
                RetryClass::Never,
                StatusCode::BAD_GATEWAY,
                "all route candidates failed",
            )
        }))
    }

    async fn bootstrap_stream(
        &self,
        prepared: PreparedAttempt,
    ) -> Result<PreparedAttempt, ProxyFailure> {
        let PreparedAttempt::Stream {
            status,
            headers,
            mut chunks,
        } = prepared
        else {
            return Ok(prepared);
        };

        loop {
            match tokio::time::timeout(self.retry_policy.first_byte_timeout(), chunks.next()).await
            {
                Ok(Some(Ok(bytes))) if bytes.is_empty() => continue,
                Ok(Some(Ok(bytes))) => {
                    let prefixed = stream::once(async move { Ok(bytes) }).chain(chunks).boxed();
                    return Ok(PreparedAttempt::Stream {
                        status,
                        headers,
                        chunks: prefixed,
                    });
                }
                Ok(Some(Err(failure))) => return Err(precommit_stream_failure(failure)),
                Ok(None) => return Err(precommit_stream_ended_failure()),
                Err(_) => return Err(upstream_first_byte_timeout_failure()),
            }
        }
    }

    async fn execute_usage(
        &self,
        request: CanonicalProxyRequest,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let snapshots = self
            .repository
            .load_balance_snapshots()
            .await
            .map_err(|error| internal_failure(format!("load balance snapshots failed: {error}")))?;
        Ok(ProxyExecutionResponse::local_buffered(
            StatusCode::OK,
            json_headers(),
            local_usage_body(snapshots)?,
            &request,
            "local_usage_success",
        ))
    }

    async fn execute_models(
        &self,
        request: CanonicalProxyRequest,
        candidates: Vec<RichRouteCandidate>,
        mapped_model: Option<String>,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let mut seen_ids = HashSet::new();
        let mut models = Vec::new();
        let mut failed_count = 0_i64;
        let mut last_failure = None;
        let mut headers = HeaderMap::new();

        for candidate in candidates
            .iter()
            .take(self.retry_policy.max_attempts(candidates.len()))
        {
            match self
                .attempts
                .attempt(&request, candidate, mapped_model.as_deref())
                .await
            {
                Ok(prepared) => {
                    let (_, attempt_headers, body) = prepared.into_parts();
                    headers = attempt_headers;
                    match body {
                        ProxyExecutionBody::Buffered(body) => match extract_models(&body) {
                            Ok(items) => {
                                for item in items {
                                    let Some(id) = item.get("id").and_then(Value::as_str) else {
                                        continue;
                                    };
                                    if seen_ids.insert(id.to_string()) {
                                        models.push(item);
                                    }
                                }
                            }
                            Err(error) => {
                                failed_count += 1;
                                last_failure = Some(internal_failure(error));
                            }
                        },
                        ProxyExecutionBody::Stream(_) => {
                            failed_count += 1;
                            last_failure = Some(internal_failure(
                                "model list upstream returned a stream response",
                            ));
                        }
                    }
                }
                Err(failure) => {
                    failed_count += 1;
                    last_failure = Some(failure);
                }
            }
        }

        if models.is_empty() {
            return Err(
                last_failure.unwrap_or_else(|| internal_failure("all model upstreams failed"))
            );
        }

        let body = serde_json::to_vec(&serde_json::json!({
            "object": "list",
            "data": models,
        }))
        .map(Bytes::from)
        .map_err(|error| internal_failure(format!("serialize models response failed: {error}")))?;
        if headers.is_empty() {
            headers = json_headers();
        }

        Ok(ProxyExecutionResponse::local_buffered_with_fallback(
            StatusCode::OK,
            headers,
            body,
            &request,
            "models_aggregated_success",
            failed_count,
        ))
    }
}

impl PreparedAttempt {
    fn into_parts(self) -> (StatusCode, HeaderMap, ProxyExecutionBody) {
        match self {
            Self::Buffered {
                status,
                headers,
                body,
            } => (status, headers, ProxyExecutionBody::Buffered(body)),
            Self::Stream {
                status,
                headers,
                chunks,
            } => (status, headers, ProxyExecutionBody::Stream(chunks)),
        }
    }
}

impl ProxyExecutionResponse {
    fn from_prepared(
        prepared: PreparedAttempt,
        candidate: &RichRouteCandidate,
        fallback_count: i64,
        request: &CanonicalProxyRequest,
        traces: Vec<AttemptTrace>,
    ) -> Self {
        let (status, headers, body) = prepared.into_parts();
        let now = now_millis_for_services().to_string();
        let body_bytes = match &body {
            ProxyExecutionBody::Buffered(body) => Some(body.len() as i64),
            ProxyExecutionBody::Stream(_) => None,
        };
        let attempts_json = serialize_attempt_traces(&traces);
        Self {
            status,
            headers,
            body,
            selected_station_key_id: Some(candidate.candidate.station_key_id.clone()),
            selected_station_id: Some(candidate.candidate.station_id.clone()),
            fallback_count,
            final_outcome: FinalRequestOutcome {
                request_id: request.request_id.clone(),
                method: if matches!(
                    request.endpoint,
                    crate::models::routing::RouteEndpointKind::Models
                ) {
                    "GET".to_string()
                } else {
                    "POST".to_string()
                },
                path: endpoint_path(&request.endpoint).to_string(),
                model: request.model.clone(),
                stream: request.stream,
                status: "success".to_string(),
                lifecycle_status: Some("buffered_success".to_string()),
                selected_station_key_id: Some(candidate.candidate.station_key_id.clone()),
                selected_station_id: Some(candidate.candidate.station_id.clone()),
                upstream_base_url: Some(candidate.candidate.upstream_base_url.clone()),
                fallback_count,
                error_message: None,
                route_policy: Some("priority_fallback".to_string()),
                route_reason: Some(format!(
                    "selected {} for {}",
                    candidate.candidate.station_key_id,
                    endpoint_path(&request.endpoint)
                )),
                rejected_candidates_json: Some("[]".to_string()),
                body_bytes,
                attempt_count: Some((fallback_count + 1).max(1)),
                route_wait_ms: Some(0),
                upstream_headers_ms: Some(0),
                failure_source: None,
                attempts_json,
                completion_source: Some("upstream".to_string()),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                cache_creation_tokens: None,
                cache_read_tokens: None,
                first_token_ms: None,
                started_at: now.clone(),
                finished_at: now,
                duration_ms: Some(0),
                feedback: Some(CandidateFeedback {
                    station_key_id: candidate.candidate.station_key_id.clone(),
                    station_id: candidate.candidate.station_id.clone(),
                    station_endpoint_revision: candidate.candidate.station_endpoint_revision,
                    kind: CandidateFeedbackKind::Success,
                }),
            },
        }
    }

    pub(crate) fn selected_station_key_id(&self) -> Option<&str> {
        self.selected_station_key_id.as_deref()
    }

    pub(crate) fn selected_station_id(&self) -> Option<&str> {
        self.selected_station_id.as_deref()
    }

    pub(crate) fn fallback_count(&self) -> i64 {
        self.fallback_count
    }

    fn local_buffered(
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        request: &CanonicalProxyRequest,
        lifecycle_status: &str,
    ) -> Self {
        Self::local_buffered_with_fallback(status, headers, body, request, lifecycle_status, 0)
    }

    fn local_buffered_with_fallback(
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        request: &CanonicalProxyRequest,
        lifecycle_status: &str,
        fallback_count: i64,
    ) -> Self {
        let now = now_millis_for_services().to_string();
        let body_bytes = body.len() as i64;
        Self {
            status,
            headers,
            body: ProxyExecutionBody::Buffered(body),
            selected_station_key_id: None,
            selected_station_id: None,
            fallback_count,
            final_outcome: FinalRequestOutcome {
                request_id: request.request_id.clone(),
                method: "GET".to_string(),
                path: request.local_path.clone(),
                model: request.model.clone(),
                stream: request.stream,
                status: "success".to_string(),
                lifecycle_status: Some(lifecycle_status.to_string()),
                selected_station_key_id: None,
                selected_station_id: None,
                upstream_base_url: None,
                fallback_count,
                error_message: None,
                route_policy: None,
                route_reason: None,
                rejected_candidates_json: Some("[]".to_string()),
                body_bytes: Some(body_bytes),
                attempt_count: Some(0),
                route_wait_ms: Some(0),
                upstream_headers_ms: None,
                failure_source: None,
                attempts_json: Some("[]".to_string()),
                completion_source: Some("local".to_string()),
                prompt_tokens: None,
                completion_tokens: None,
                total_tokens: None,
                cache_creation_tokens: None,
                cache_read_tokens: None,
                first_token_ms: None,
                started_at: now.clone(),
                finished_at: now,
                duration_ms: Some(0),
                feedback: None,
            },
        }
    }
}

pub(crate) struct UpstreamAttemptExecutor {
    pool: UpstreamClientPool,
}

impl UpstreamAttemptExecutor {
    pub(crate) fn new(pool: UpstreamClientPool) -> Self {
        Self { pool }
    }
}

impl AttemptExecutor for UpstreamAttemptExecutor {
    fn attempt<'a>(
        &'a self,
        request: &'a CanonicalProxyRequest,
        candidate: &'a RichRouteCandidate,
        mapped_model: Option<&'a str>,
    ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>> {
        Box::pin(async move {
            let adapter = EndpointAdapter::for_endpoint(&request.endpoint);
            let prepared = adapter.prepare(request, &candidate.candidate, mapped_model)?;
            let response_mode = prepared.response_mode;
            let attempt = self.pool.send(prepared, &candidate.candidate).await?;
            match attempt {
                UpstreamAttempt::Buffered {
                    status,
                    headers,
                    body,
                } => {
                    if !status.is_success() {
                        return Err(upstream_http_failure(status));
                    }
                    let body = transform_buffered_body(body, response_mode, mapped_model)?;
                    Ok(PreparedAttempt::Buffered {
                        status,
                        headers: response_headers_for_downstream(&headers),
                        body,
                    })
                }
                UpstreamAttempt::Stream {
                    status,
                    headers,
                    chunks,
                } => {
                    if !status.is_success() {
                        return Err(upstream_http_failure(status));
                    }
                    Ok(PreparedAttempt::Stream {
                        status,
                        headers: response_headers_for_downstream(&headers),
                        chunks,
                    })
                }
            }
        })
    }
}

impl RetryPolicy {
    #[cfg(test)]
    fn for_tests(
        max_candidate_attempts: usize,
        precommit_budget: Duration,
        first_byte_timeout: Duration,
        buffered_budget: Duration,
    ) -> Self {
        Self {
            max_candidate_attempts,
            precommit_budget,
            first_byte_timeout,
            buffered_budget,
        }
    }

    pub(crate) fn decide(
        &self,
        failure: &ProxyFailure,
        idempotent: bool,
        committed: bool,
    ) -> RetryDecision {
        if committed || failure.retry_class == RetryClass::AfterCommitStop {
            return RetryDecision::Stop;
        }
        if matches!(failure.code, ProxyFailureCode::UpstreamStreamFailed) {
            return RetryDecision::Stop;
        }
        if matches!(failure.code, ProxyFailureCode::RouteWaitTimeout) {
            return RetryDecision::Stop;
        }
        if matches!(failure.code, ProxyFailureCode::UpstreamConnectFailed) {
            return if idempotent {
                RetryDecision::NextCandidate
            } else {
                RetryDecision::Stop
            };
        }

        match failure.http_status.as_u16() {
            401 | 403 | 408 | 425 | 429 | 500..=599 => RetryDecision::NextCandidate,
            404 if failure.internal_detail.as_deref() == Some("capability_mismatch") => {
                RetryDecision::NextCandidate
            }
            400 | 404 | 409 | 422 => RetryDecision::Stop,
            _ if failure.retry_class == RetryClass::BeforeOutput => RetryDecision::NextCandidate,
            _ => RetryDecision::Stop,
        }
    }

    pub(crate) fn max_attempts(&self, eligible_candidates: usize) -> usize {
        eligible_candidates.min(self.max_candidate_attempts)
    }

    pub(crate) fn precommit_budget(&self) -> Duration {
        self.precommit_budget
    }

    pub(crate) fn buffered_budget(&self) -> Duration {
        self.buffered_budget
    }

    pub(crate) fn first_byte_timeout(&self) -> Duration {
        self.first_byte_timeout
    }

    fn remaining_precommit_budget(&self, started: Instant) -> Option<Duration> {
        self.precommit_budget.checked_sub(started.elapsed())
    }
}

impl fmt::Debug for ProxyExecutionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxyExecutionResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .field("selected_station_key_id", &self.selected_station_key_id)
            .field("selected_station_id", &self.selected_station_id)
            .field("fallback_count", &self.fallback_count)
            .finish()
    }
}

impl fmt::Debug for ProxyExecutionBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered(body) => formatter
                .debug_struct("Buffered")
                .field("body_len", &body.len())
                .finish(),
            Self::Stream(_) => formatter.write_str("Stream"),
        }
    }
}

fn internal_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::InternalProxyError,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::INTERNAL_SERVER_ERROR,
        message,
    )
}

fn precommit_timeout_failure() -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::RouteWaitTimeout,
        FailureSource::Routing,
        RetryClass::BeforeOutput,
        StatusCode::GATEWAY_TIMEOUT,
        "route precommit budget exhausted",
    )
}

fn route_request(request: &CanonicalProxyRequest) -> RouteRequest {
    RouteRequest {
        endpoint: request.endpoint.clone(),
        model: request.model.clone(),
        stream: request.stream,
        uses_tools: request.requirements.uses_tools,
        uses_vision: request.requirements.uses_vision,
        uses_reasoning: request.requirements.uses_reasoning,
        policy: RoutingPolicy::PriorityFallback,
        max_rate_multiplier: None,
        routing_group_filter: request.requirements.routing_group_filter.clone(),
        session_hash: request.session_hash.clone(),
        previous_response_id: request.previous_response_id.clone(),
        excluded_key_ids: Vec::new(),
        current_station_key_id: None,
        allow_depleted_fallback: false,
        now_ms: now_millis_for_services() as i64,
    }
}

fn endpoint_path(endpoint: &crate::models::routing::RouteEndpointKind) -> &'static str {
    match endpoint {
        crate::models::routing::RouteEndpointKind::Models => "/v1/models",
        crate::models::routing::RouteEndpointKind::ChatCompletions => "/v1/chat/completions",
        crate::models::routing::RouteEndpointKind::Responses => "/v1/responses",
        crate::models::routing::RouteEndpointKind::Embeddings => "/v1/embeddings",
    }
}

fn transform_buffered_body(
    body: Bytes,
    response_mode: ResponseMode,
    mapped_model: Option<&str>,
) -> Result<Bytes, ProxyFailure> {
    if response_mode != ResponseMode::BufferedChatToResponses {
        return Ok(body);
    }
    let value = serde_json::from_slice::<Value>(&body).map_err(|error| {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamHttpError,
            FailureSource::Upstream,
            RetryClass::Never,
            StatusCode::BAD_GATEWAY,
            format!("upstream chat fallback response was not JSON: {error}"),
        )
    })?;
    serde_json::to_vec(&render_responses_response(value, mapped_model))
        .map(Bytes::from)
        .map_err(|error| internal_failure(format!("serialize responses fallback failed: {error}")))
}

fn serialize_attempt_traces(traces: &[AttemptTrace]) -> Option<String> {
    serde_json::to_string(traces).ok()
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    headers
}

fn extract_models(body: &Bytes) -> Result<Vec<Value>, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| format!("model list JSON could not be parsed: {error}"))?;
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        return Ok(data.clone());
    }
    if let Some(data) = value.as_array() {
        return Ok(data.clone());
    }
    Err("model list response did not contain data array".to_string())
}

fn local_usage_body(snapshots: Vec<BalanceSnapshot>) -> Result<Bytes, ProxyFailure> {
    let mut latest_by_station: HashMap<String, BalanceSnapshot> = HashMap::new();
    for snapshot in snapshots {
        let should_replace = latest_by_station
            .get(&snapshot.station_id)
            .map(|current| balance_snapshot_rank(&snapshot) > balance_snapshot_rank(current))
            .unwrap_or(true);
        if snapshot.scope == "station" && should_replace {
            latest_by_station.insert(snapshot.station_id.clone(), snapshot);
        }
    }

    let latest_station_balances = latest_by_station.values().collect::<Vec<_>>();
    let total_balance = latest_station_balances
        .iter()
        .filter_map(|snapshot| snapshot.value)
        .sum::<f64>();
    let currency = latest_station_balances
        .iter()
        .find_map(|snapshot| {
            let currency = snapshot.currency.trim();
            (!currency.is_empty()).then(|| currency.to_string())
        })
        .unwrap_or_else(|| "CNY".to_string());
    let low_balance_stations = latest_station_balances
        .iter()
        .filter(|snapshot| snapshot.status == "low" || snapshot.status == "depleted")
        .count();
    let updated_at = latest_station_balances
        .iter()
        .map(|snapshot| snapshot.updated_at.as_str())
        .max()
        .map(str::to_string);

    serde_json::to_vec(&serde_json::json!({
        "is_active": true,
        "remaining": total_balance,
        "balance": total_balance,
        "unit": currency,
        "quota": {
            "remaining": total_balance,
            "unit": currency,
        },
        "source": "relay_pool_desktop_balance_snapshots",
        "stations": latest_station_balances.len(),
        "low_balance_stations": low_balance_stations,
        "updated_at": updated_at,
    }))
    .map(Bytes::from)
    .map_err(|error| internal_failure(format!("serialize local usage response failed: {error}")))
}

fn balance_snapshot_rank(snapshot: &BalanceSnapshot) -> (i128, i128, i128) {
    (
        parse_balance_time(&snapshot.updated_at),
        parse_balance_time(&snapshot.created_at),
        snapshot
            .collected_at
            .as_deref()
            .map(parse_balance_time)
            .unwrap_or(0),
    )
}

fn parse_balance_time(value: &str) -> i128 {
    value.trim().parse::<i128>().unwrap_or(0)
}

fn upstream_http_failure(status: StatusCode) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamHttpError,
        FailureSource::Upstream,
        RetryClass::BeforeOutput,
        status,
        format!("upstream HTTP {}", status.as_u16()),
    )
}

fn precommit_stream_failure(failure: ProxyFailure) -> ProxyFailure {
    upstream_first_byte_failure(format!(
        "upstream stream failed before first byte: {}",
        failure.public_message
    ))
}

fn precommit_stream_ended_failure() -> ProxyFailure {
    upstream_first_byte_failure("upstream stream ended before first byte")
}

fn upstream_first_byte_timeout_failure() -> ProxyFailure {
    upstream_first_byte_failure("upstream first byte timed out")
}

fn upstream_first_byte_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamFirstByteTimeout,
        FailureSource::Upstream,
        RetryClass::BeforeOutput,
        StatusCode::BAD_GATEWAY,
        message,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream, StreamExt};
    use http::{HeaderMap, StatusCode};

    use crate::{
        models::{
            proxy::{RequestLog, UpstreamApiFormat},
            routing::{RouteEndpointKind, StationKeyCapabilities},
        },
        services::proxy::{
            error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
            limits::{BodyBudget, RequestLease},
            request::{CanonicalProxyRequest, RequestRequirements},
            router::RichRouteCandidate,
            routing_repository::{FinalRequestOutcome, RoutingRepository},
            RouteCandidate,
        },
    };

    use super::{AttemptExecutor, ExecutionEngine, PreparedAttempt, RetryDecision, RetryPolicy};

    #[test]
    fn retry_policy_matches_the_approved_precommit_matrix() {
        let cases = [
            (failure(401), false, false, RetryDecision::NextCandidate),
            (failure(403), false, false, RetryDecision::NextCandidate),
            (
                capability_mismatch(404),
                false,
                false,
                RetryDecision::NextCandidate,
            ),
            (failure(404), false, false, RetryDecision::Stop),
            (failure(408), false, false, RetryDecision::NextCandidate),
            (failure(425), false, false, RetryDecision::NextCandidate),
            (failure(429), false, false, RetryDecision::NextCandidate),
            (failure(500), false, false, RetryDecision::NextCandidate),
            (failure(400), false, false, RetryDecision::Stop),
            (failure(409), false, false, RetryDecision::Stop),
            (failure(422), false, false, RetryDecision::Stop),
            (
                ambiguous_transport_failure(),
                false,
                false,
                RetryDecision::Stop,
            ),
            (
                ambiguous_transport_failure(),
                true,
                false,
                RetryDecision::NextCandidate,
            ),
            (stream_failure(), true, true, RetryDecision::Stop),
        ];

        for (failure, idempotent, committed, expected) in cases {
            assert_eq!(
                RetryPolicy::default().decide(&failure, idempotent, committed),
                expected,
                "failure={failure:?} idempotent={idempotent} committed={committed}"
            );
        }
    }

    #[test]
    fn retry_policy_caps_attempts_and_uses_the_approved_budgets() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.max_attempts(10), 3);
        assert_eq!(policy.max_attempts(2), 2);
        assert_eq!(policy.precommit_budget(), Duration::from_secs(180));
        assert_eq!(policy.buffered_budget(), Duration::from_secs(300));
    }

    #[tokio::test]
    async fn execution_engine_preserves_route_order_and_finalizes_one_candidate() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(429)),
            Ok(buffered_success(b"{\"ok\":true}")),
        ]));
        let engine = ExecutionEngine::new(repository.clone(), attempts.clone());

        let response = engine
            .execute(canonical_chat_request().await)
            .await
            .expect("response");

        assert_eq!(attempts.seen_ids(), ["a", "b"]);
        assert_eq!(response.selected_station_key_id(), Some("b"));
        assert_eq!(response.fallback_count(), 1);
        assert_eq!(
            repository.finalized_count(),
            0,
            "response body owns finalization"
        );
    }

    #[tokio::test]
    async fn execution_engine_tries_at_most_three_distinct_candidates() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
            rich_candidate("c"),
            rich_candidate("d"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(500)),
            Err(failure(500)),
            Err(failure(500)),
            Ok(buffered_success(b"{\"unexpected\":true}")),
        ]));
        let engine = ExecutionEngine::new(repository, attempts.clone());

        let failure = engine
            .execute(canonical_chat_request().await)
            .await
            .expect_err("first three failed candidates stop request");

        assert_eq!(attempts.seen_ids(), ["a", "b", "c"]);
        assert_eq!(failure.code, ProxyFailureCode::UpstreamHttpError);
    }

    #[tokio::test]
    async fn execution_engine_enforces_one_precommit_budget_across_candidates() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::delayed_responses(
            vec![
                Err(failure(500)),
                Ok(buffered_success(b"{\"too_late\":true}")),
            ],
            Duration::from_millis(20),
        ));
        let engine = ExecutionEngine::new(repository, attempts.clone()).with_retry_policy(
            RetryPolicy::for_tests(
                3,
                Duration::from_millis(5),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ),
        );

        let failure = engine
            .execute(canonical_chat_request().await)
            .await
            .expect_err("precommit budget exhausted");

        assert_eq!(failure.code, ProxyFailureCode::RouteWaitTimeout);
        assert_eq!(attempts.seen_ids(), ["a"]);
    }

    #[tokio::test]
    async fn stream_bootstrap_fails_over_before_first_chunk() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(stream_error_before_data()),
            Ok(stream_success(b"data: ok\n\n")),
        ]));
        let engine = ExecutionEngine::new(repository, attempts.clone());

        let mut response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("fallback stream response");

        assert_eq!(attempts.seen_ids(), ["a", "b"]);
        assert_eq!(response.selected_station_key_id(), Some("b"));
        assert_eq!(response.fallback_count(), 1);
        let super::ProxyExecutionBody::Stream(chunks) = &mut response.body else {
            panic!("expected stream body");
        };
        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"data: ok\n\n")
        );
    }

    #[tokio::test]
    async fn committed_stream_error_never_selects_another_candidate() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(stream_then_error(b"data: first\n\n")),
            Ok(stream_success(b"data: forbidden\n\n")),
        ]));
        let engine = ExecutionEngine::new(repository, attempts.clone());

        let mut response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("committed stream response");

        assert_eq!(response.selected_station_key_id(), Some("a"));
        assert_eq!(attempts.seen_ids(), ["a"]);
        let super::ProxyExecutionBody::Stream(chunks) = &mut response.body else {
            panic!("expected stream body");
        };
        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"data: first\n\n")
        );
        assert!(chunks.next().await.unwrap().is_err());
        assert_eq!(attempts.seen_ids(), ["a"]);
    }

    struct FakeRepository {
        candidates: Vec<RichRouteCandidate>,
        finalized: Mutex<Vec<FinalRequestOutcome>>,
    }

    impl FakeRepository {
        fn with_candidates(candidates: Vec<RichRouteCandidate>) -> Self {
            Self {
                candidates,
                finalized: Mutex::new(Vec::new()),
            }
        }

        fn finalized_count(&self) -> usize {
            self.finalized.lock().expect("finalized lock").len()
        }
    }

    impl RoutingRepository for FakeRepository {
        fn load_runtime_candidates(
            &self,
        ) -> BoxFuture<'static, Result<Vec<RichRouteCandidate>, String>> {
            let candidates = self.candidates.clone();
            Box::pin(async move { Ok(candidates) })
        }

        fn record_final_outcome(
            &self,
            outcome: FinalRequestOutcome,
        ) -> BoxFuture<'static, Result<Option<RequestLog>, String>> {
            self.finalized.lock().expect("finalized lock").push(outcome);
            Box::pin(async { Ok(None) })
        }
    }

    struct FakeAttemptExecutor {
        responses: Mutex<Vec<Result<PreparedAttempt, ProxyFailure>>>,
        seen_ids: Mutex<Vec<String>>,
        delay: Option<Duration>,
    }

    impl FakeAttemptExecutor {
        fn responses(responses: Vec<Result<PreparedAttempt, ProxyFailure>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen_ids: Mutex::new(Vec::new()),
                delay: None,
            }
        }

        fn delayed_responses(
            responses: Vec<Result<PreparedAttempt, ProxyFailure>>,
            delay: Duration,
        ) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen_ids: Mutex::new(Vec::new()),
                delay: Some(delay),
            }
        }

        fn seen_ids(&self) -> Vec<String> {
            self.seen_ids.lock().expect("seen lock").clone()
        }
    }

    impl AttemptExecutor for FakeAttemptExecutor {
        fn attempt<'a>(
            &'a self,
            _request: &'a CanonicalProxyRequest,
            candidate: &'a RichRouteCandidate,
            _mapped_model: Option<&'a str>,
        ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>> {
            self.seen_ids
                .lock()
                .expect("seen lock")
                .push(candidate.candidate.station_key_id.clone());
            Box::pin(async move {
                if let Some(delay) = self.delay {
                    tokio::time::sleep(delay).await;
                }
                self.responses.lock().expect("responses lock").remove(0)
            })
        }
    }

    fn failure(status: u16) -> ProxyFailure {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamHttpError,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::from_u16(status).expect("status"),
            format!("upstream HTTP {status}"),
        )
    }

    fn capability_mismatch(status: u16) -> ProxyFailure {
        let mut failure = failure(status);
        failure.internal_detail = Some("capability_mismatch".to_string());
        failure
    }

    fn ambiguous_transport_failure() -> ProxyFailure {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamConnectFailed,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::BAD_GATEWAY,
            "upstream connection reset after request write",
        )
    }

    fn stream_failure() -> ProxyFailure {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamStreamFailed,
            FailureSource::Upstream,
            RetryClass::AfterCommitStop,
            StatusCode::BAD_GATEWAY,
            "upstream stream failed",
        )
    }

    fn buffered_success(body: &'static [u8]) -> PreparedAttempt {
        PreparedAttempt::Buffered {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: Bytes::from_static(body),
        }
    }

    fn stream_success(first: &'static [u8]) -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![Ok(Bytes::from_static(first))])),
        }
    }

    fn stream_error_before_data() -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![Err(stream_failure())])),
        }
    }

    fn stream_then_error(first: &'static [u8]) -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![
                Ok(Bytes::from_static(first)),
                Err(stream_failure()),
            ])),
        }
    }

    async fn canonical_chat_request() -> CanonicalProxyRequest {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}]}"#,
        );
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        CanonicalProxyRequest::new(
            "req-exec".to_string(),
            "/v1/chat/completions".to_string(),
            RouteEndpointKind::ChatCompletions,
            Some("gpt-test".to_string()),
            false,
            RequestRequirements::default(),
            body,
            HeaderMap::new(),
            None,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(std::sync::atomic::AtomicU32::new(0))),
        )
    }

    async fn streaming_chat_request() -> CanonicalProxyRequest {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
        );
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        CanonicalProxyRequest::new(
            "req-stream".to_string(),
            "/v1/chat/completions".to_string(),
            RouteEndpointKind::ChatCompletions,
            Some("gpt-test".to_string()),
            true,
            RequestRequirements::default(),
            body,
            HeaderMap::new(),
            None,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(std::sync::atomic::AtomicU32::new(0))),
        )
    }

    fn rich_candidate(id: &str) -> RichRouteCandidate {
        RichRouteCandidate {
            candidate: RouteCandidate {
                station_key_id: id.to_string(),
                station_id: format!("station-{id}"),
                station_endpoint_revision: 1,
                upstream_base_url: "https://example.test/v1".to_string(),
                api_key: format!("sk-{id}"),
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
                upstream_api_format: UpstreamApiFormat::Auto,
                priority: 0,
                max_concurrency: 0,
                load_factor: None,
                schedulable: true,
            },
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            capabilities: StationKeyCapabilities {
                station_key_id: id.to_string(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: true,
                supports_stream: true,
                supports_tools: true,
                supports_vision: true,
                supports_reasoning: true,
                model_allowlist: Vec::new(),
                model_blocklist: Vec::new(),
                preferred_models: Vec::new(),
                only_use_as_backup: false,
                routing_tags: Vec::new(),
                updated_at: "0".to_string(),
            },
            health: None,
            economics: None,
            scheduler_group_binding_id: None,
            scheduler_group_id_hash: None,
            scheduler_group_type: None,
            scheduler_effective_multiplier: None,
            scheduler_multiplier_reject_reason: None,
        }
    }
}
