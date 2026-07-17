use std::{fmt, sync::Arc, time::Duration};

use bytes::Bytes;
use futures_util::future::BoxFuture;
use http::{HeaderMap, StatusCode};

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    request::{ByteStream, CanonicalProxyRequest},
    router::RichRouteCandidate,
    routing_repository::RoutingRepository,
};

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
    buffered_budget: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_candidate_attempts: 3,
            precommit_budget: Duration::from_secs(180),
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

    pub(crate) async fn execute(
        &self,
        request: CanonicalProxyRequest,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let candidates = self
            .repository
            .load_runtime_candidates()
            .await
            .map_err(|error| internal_failure(format!("load route candidates failed: {error}")))?;
        if candidates.is_empty() {
            return Err(ProxyFailure::new(
                ProxyFailureCode::RouteNoCandidate,
                FailureSource::Routing,
                RetryClass::Never,
                StatusCode::SERVICE_UNAVAILABLE,
                "no eligible route candidate",
            ));
        }

        let idempotent = request.idempotency_key.is_some();
        let mut last_failure = None;
        for (attempt_index, candidate) in candidates
            .iter()
            .take(self.retry_policy.max_attempts(candidates.len()))
            .enumerate()
        {
            match self.attempts.attempt(&request, candidate).await {
                Ok(prepared) => {
                    return Ok(ProxyExecutionResponse::from_prepared(
                        prepared,
                        candidate,
                        attempt_index as i64,
                    ));
                }
                Err(failure) => {
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
    ) -> Self {
        let (status, headers, body) = prepared.into_parts();
        Self {
            status,
            headers,
            body,
            selected_station_key_id: Some(candidate.candidate.station_key_id.clone()),
            selected_station_id: Some(candidate.candidate.station_id.clone()),
            fallback_count,
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
}

impl RetryPolicy {
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

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use bytes::Bytes;
    use futures_util::future::BoxFuture;
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
    }

    impl FakeAttemptExecutor {
        fn responses(responses: Vec<Result<PreparedAttempt, ProxyFailure>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen_ids: Mutex::new(Vec::new()),
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
        ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>> {
            self.seen_ids
                .lock()
                .expect("seen lock")
                .push(candidate.candidate.station_key_id.clone());
            Box::pin(async move { self.responses.lock().expect("responses lock").remove(0) })
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
