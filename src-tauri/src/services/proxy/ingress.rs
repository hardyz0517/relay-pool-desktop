use std::{
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, OnceLock,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, HeaderMap, Method, Response, StatusCode, Uri},
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use futures_util::{future::BoxFuture, TryStreamExt};
use tokio::{sync::Semaphore, time::timeout};

use crate::models::routing::RouteEndpointKind;

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    lifecycle::{
        delivery::DeliveryTerminal,
        request::{
            FinalRequestRecord, RequestContextSnapshot, RequestFailure, RequestLogAnnotations,
            RequestStartRecord, RequestTerminal, RequestTerminalSnapshot,
        },
        writer::{LifecycleWriter, WriterAdmissionError},
    },
    limits::{BodyBudget, BodyBudgetError, ProxyStartupResourceLimits, RequestLease},
    local_auth::{self, AuthDecision},
    observability::RequestObservation,
    request::{
        CanonicalProxyRequest, ProxyHttpResponse, ProxyResponsePayload, RequestLifecycleAdmission,
        RequestRequirements, RequestTimingContext,
    },
};

const REQUEST_ID_HEADER: &str = "x-relay-request-id";
const SAFE_FORWARD_HEADERS: &[&str] = &[
    "accept",
    "content-type",
    "idempotency-key",
    "openai-organization",
    "openai-project",
    "openai-beta",
    "user-agent",
];

pub(crate) trait IngressExecutor: Send + Sync {
    fn execute(
        &self,
        request: CanonicalProxyRequest,
    ) -> BoxFuture<'static, Result<ProxyHttpResponse, ProxyFailure>>;
}

pub(crate) struct IngressState {
    local_access_key: String,
    limits: ProxyStartupResourceLimits,
    body_budget: BodyBudget,
    request_semaphore: Arc<Semaphore>,
    active_requests: Arc<AtomicU32>,
    request_count: Arc<AtomicU64>,
    executor: Arc<dyn IngressExecutor>,
    lifecycle_writer: Option<LifecycleWriter>,
    transport_policy_store: super::transport_policy::TransportPolicyStore,
}

impl IngressState {
    // Unit tests use an isolated ingress without runtime-owned counters or lifecycle writes.
    #[cfg(test)]
    pub(crate) fn new(
        local_access_key: impl Into<String>,
        limits: ProxyStartupResourceLimits,
        executor: Arc<dyn IngressExecutor>,
    ) -> Self {
        Self::with_active_requests(
            local_access_key,
            limits,
            executor,
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicU64::new(0)),
            None,
        )
    }

    #[cfg(test)]
    pub(crate) fn with_active_requests(
        local_access_key: impl Into<String>,
        limits: ProxyStartupResourceLimits,
        executor: Arc<dyn IngressExecutor>,
        active_requests: Arc<AtomicU32>,
        request_count: Arc<AtomicU64>,
        lifecycle_writer: Option<LifecycleWriter>,
    ) -> Self {
        Self::with_active_requests_and_policy(
            local_access_key,
            limits,
            executor,
            active_requests,
            request_count,
            lifecycle_writer,
            super::transport_policy::TransportPolicyStore::default(),
        )
    }

    pub(crate) fn with_active_requests_and_policy(
        local_access_key: impl Into<String>,
        limits: ProxyStartupResourceLimits,
        executor: Arc<dyn IngressExecutor>,
        active_requests: Arc<AtomicU32>,
        request_count: Arc<AtomicU64>,
        lifecycle_writer: Option<LifecycleWriter>,
        transport_policy_store: super::transport_policy::TransportPolicyStore,
    ) -> Self {
        Self {
            local_access_key: local_access_key.into(),
            body_budget: BodyBudget::new(limits.max_buffered_body_bytes),
            request_semaphore: Arc::new(Semaphore::new(limits.max_in_flight_requests)),
            active_requests,
            request_count,
            limits,
            executor,
            lifecycle_writer,
            transport_policy_store,
        }
    }
}

pub(crate) fn router(state: Arc<IngressState>) -> Router {
    Router::new()
        .route("/usage", get(handle))
        .route("/v1/usage", get(handle))
        .route("/v1/models", get(handle))
        .route("/v1/chat/completions", post(handle).options(handle))
        .route("/v1/responses", post(handle).options(handle))
        .route("/v1/embeddings", post(handle).options(handle))
        .fallback(handle)
        .with_state(state)
}

async fn handle(
    State(state): State<Arc<IngressState>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response<Body> {
    // Start the monotonic request budget before any local admission, body
    // buffering, lifecycle write, or route planning work.
    let request_timing = RequestTimingContext::now(now_millis());
    let Some(request_lease) = acquire_request_lease(&state) else {
        return failure_response(proxy_failure(
            ProxyFailureCode::LocalProxyBusy,
            StatusCode::SERVICE_UNAVAILABLE,
            "local proxy is busy",
        ));
    };
    state.request_count.fetch_add(1, Ordering::Relaxed);
    let request_id = next_request_id();
    let cors_origin = match cors_origin(&headers) {
        Ok(origin) => origin,
        Err(failure) => return failure_response(failure),
    };
    if method == Method::OPTIONS {
        let mut response = Response::builder().status(StatusCode::NO_CONTENT);
        add_common_headers(&mut response, &request_id, cors_origin);
        return response.body(Body::empty()).expect("valid response");
    }
    match local_auth::authorize_headers(&headers, &state.local_access_key) {
        AuthDecision::Accepted => {}
        AuthDecision::Missing => {
            return with_cors(
                failure_response(proxy_failure(
                    ProxyFailureCode::LocalAuthMissing,
                    StatusCode::UNAUTHORIZED,
                    "missing local proxy bearer token",
                )),
                &request_id,
                cors_origin,
            )
        }
        AuthDecision::Invalid => {
            return with_cors(
                failure_response(proxy_failure(
                    ProxyFailureCode::LocalAuthInvalid,
                    StatusCode::UNAUTHORIZED,
                    "invalid local proxy bearer token",
                )),
                &request_id,
                cors_origin,
            )
        }
    }

    // Freeze one immutable transport policy for this request. Later publishes
    // affect only requests that have not reached this point yet.
    let transport_policy = state.transport_policy_store.load();
    let request_deadline = transport_policy.request_deadline;
    let request_timing = request_timing.with_deadline(request_deadline);

    let route = route_disposition(&method, uri.path());
    let lifecycle_context = request_lifecycle_context(
        &request_id,
        &method,
        uri.path(),
        route_context_endpoint(&route),
        request_timing.started_at_ms,
    );
    let record_start = !is_balance_query_path(uri.path());
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if content_length.is_some_and(|length| length > state.limits.max_body_bytes) {
        let failure = proxy_failure(
            ProxyFailureCode::RequestBodyTooLarge,
            StatusCode::PAYLOAD_TOO_LARGE,
            "request body is too large",
        );
        let failure = persist_ingress_failure(
            &state,
            lifecycle_context,
            record_start,
            RequestLogAnnotations::default(),
            request_timing,
            failure,
        )
        .await;
        return with_cors(failure_response(failure), &request_id, cors_origin);
    }
    let body_deadline = request_deadline
        .checked_sub(request_timing.started_at.elapsed())
        .unwrap_or_default();
    let body = match timeout(
        state.limits.body_timeout.min(body_deadline),
        to_bytes(body, state.limits.max_body_bytes + 1),
    )
    .await
    {
        Ok(Ok(body)) if body.len() <= state.limits.max_body_bytes => body,
        Ok(Ok(_)) => {
            let failure = proxy_failure(
                ProxyFailureCode::RequestBodyTooLarge,
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body is too large",
            );
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                RequestLogAnnotations::default(),
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
        Ok(Err(error)) => {
            let mut failure = proxy_failure(
                ProxyFailureCode::RequestBodyInvalid,
                StatusCode::BAD_REQUEST,
                "request body is invalid",
            );
            failure.internal_detail = Some(error.to_string());
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                RequestLogAnnotations::default(),
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
        Err(_) => {
            let failure = if request_timing.started_at.elapsed() >= request_deadline {
                proxy_failure(
                    ProxyFailureCode::RouteDeadlineExceeded,
                    StatusCode::GATEWAY_TIMEOUT,
                    "local request deadline exceeded",
                )
            } else {
                proxy_failure(
                    ProxyFailureCode::RequestBodyTimeout,
                    StatusCode::REQUEST_TIMEOUT,
                    "request body timed out",
                )
            };
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                RequestLogAnnotations::default(),
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
    };
    let metadata = match request_metadata(&body) {
        Ok(metadata) => metadata,
        Err(failure) => {
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                RequestLogAnnotations::default(),
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
    };
    let start_annotations = RequestLogAnnotations {
        model: metadata.model.clone(),
        stream: metadata.stream,
        reasoning_effort: metadata.reasoning_effort.clone(),
        ..RequestLogAnnotations::default()
    };
    let endpoint = match route {
        RouteDisposition::Known(endpoint) => endpoint,
        RouteDisposition::UnknownRoute => {
            let failure = proxy_failure(
                ProxyFailureCode::InternalProxyError,
                StatusCode::NOT_FOUND,
                "local proxy endpoint was not found",
            );
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                start_annotations,
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
        RouteDisposition::MethodNotAllowed => {
            let failure = proxy_failure(
                ProxyFailureCode::InternalProxyError,
                StatusCode::METHOD_NOT_ALLOWED,
                "method is not allowed for local proxy endpoint",
            );
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                start_annotations,
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
    };
    let body_budget = match timeout(
        request_deadline
            .checked_sub(request_timing.started_at.elapsed())
            .unwrap_or_default(),
        state.body_budget.acquire(body.len()),
    )
    .await
    {
        Ok(Ok(lease)) => lease,
        Ok(Err(BodyBudgetError::InsufficientCapacity)) | Err(_) => {
            let failure = if request_timing.started_at.elapsed() >= request_deadline {
                proxy_failure(
                    ProxyFailureCode::RouteDeadlineExceeded,
                    StatusCode::GATEWAY_TIMEOUT,
                    "local request deadline exceeded",
                )
            } else {
                proxy_failure(
                    ProxyFailureCode::LocalProxyMemoryBusy,
                    StatusCode::SERVICE_UNAVAILABLE,
                    "local proxy body budget is exhausted",
                )
            };
            let failure = persist_ingress_failure(
                &state,
                lifecycle_context,
                record_start,
                start_annotations,
                request_timing,
                failure,
            )
            .await;
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
    };
    let lifecycle_admission = match admit_request_lifecycle(
        &state,
        lifecycle_context,
        record_start,
        start_annotations,
        request_timing,
    )
    .await
    {
        Ok(admission) => admission,
        Err(failure) => {
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
    };
    let canonical = CanonicalProxyRequest::new(
        request_id.clone(),
        uri.path().to_string(),
        endpoint,
        metadata.model,
        metadata.stream,
        metadata.reasoning_effort,
        metadata.requirements,
        body,
        forwarded_headers(&headers),
        header_string(&headers, "idempotency-key"),
        header_string(&headers, "x-relay-session-hash"),
        metadata.previous_response_id,
        lifecycle_admission,
        body_budget,
        request_lease,
    )
    .with_request_timing(request_timing)
    .with_transport_policy(transport_policy);
    match state.executor.execute(canonical).await {
        Ok(response) => with_cors(proxy_response(response), &request_id, cors_origin),
        Err(failure) => with_cors(failure_response(failure), &request_id, cors_origin),
    }
}

fn request_lifecycle_context(
    request_id: &str,
    method: &Method,
    path: &str,
    endpoint: impl Into<String>,
    received_at_ms: i64,
) -> RequestContextSnapshot {
    let endpoint = endpoint.into();
    RequestContextSnapshot {
        request_id: request_id.to_string(),
        method: if endpoint == "Models" || method == Method::GET {
            "GET".to_string()
        } else {
            "POST".to_string()
        },
        local_path: path.to_string(),
        endpoint,
        received_at_ms,
    }
}

enum RouteDisposition {
    Known(RouteEndpointKind),
    UnknownRoute,
    MethodNotAllowed,
}

struct ParsedRequestMetadata {
    model: Option<String>,
    stream: bool,
    reasoning_effort: Option<String>,
    requirements: RequestRequirements,
    previous_response_id: Option<String>,
}

fn route_context_endpoint(route: &RouteDisposition) -> String {
    match route {
        RouteDisposition::Known(endpoint) => format!("{endpoint:?}"),
        RouteDisposition::UnknownRoute => "UnknownRoute".to_string(),
        RouteDisposition::MethodNotAllowed => "MethodNotAllowed".to_string(),
    }
}

fn route_disposition(method: &Method, path: &str) -> RouteDisposition {
    match (method, path) {
        (&Method::GET, "/usage") | (&Method::GET, "/v1/usage") | (&Method::GET, "/v1/models") => {
            RouteDisposition::Known(RouteEndpointKind::Models)
        }
        (&Method::POST, "/v1/chat/completions") => {
            RouteDisposition::Known(RouteEndpointKind::ChatCompletions)
        }
        (&Method::POST, "/v1/responses") => RouteDisposition::Known(RouteEndpointKind::Responses),
        (&Method::POST, "/v1/embeddings") => RouteDisposition::Known(RouteEndpointKind::Embeddings),
        (_, "/usage")
        | (_, "/v1/usage")
        | (_, "/v1/models")
        | (_, "/v1/chat/completions")
        | (_, "/v1/responses")
        | (_, "/v1/embeddings") => RouteDisposition::MethodNotAllowed,
        _ => RouteDisposition::UnknownRoute,
    }
}

fn is_balance_query_path(path: &str) -> bool {
    matches!(path, "/usage" | "/v1/usage")
}

async fn admit_request_lifecycle(
    state: &IngressState,
    context: RequestContextSnapshot,
    record_start: bool,
    annotations: RequestLogAnnotations,
    request_timing: RequestTimingContext,
) -> Result<Option<RequestLifecycleAdmission>, ProxyFailure> {
    let Some(writer) = state.lifecycle_writer.as_ref() else {
        return Ok(None);
    };
    let request_reservation = writer
        .try_reserve_request()
        .map_err(lifecycle_admission_failure)?;
    if !record_start {
        return Ok(Some(RequestLifecycleAdmission {
            context,
            terminal: request_reservation.into_terminal(),
        }));
    }
    let start_record = RequestStartRecord {
        context: context.clone(),
    };
    let (terminal, start_ack) = if annotations == RequestLogAnnotations::default() {
        request_reservation.send_start(start_record)
    } else {
        request_reservation.send_start_with_annotations(start_record, annotations)
    };
    let remaining = request_timing
        .deadline
        .unwrap_or_else(|| state.transport_policy_store.load().request_deadline)
        .checked_sub(request_timing.started_at.elapsed())
        .unwrap_or_default();
    match timeout(remaining, start_ack).await {
        Ok(Ok(Ok(_))) => {}
        Ok(Ok(Err(error))) => return Err(lifecycle_write_failure(error)),
        Ok(Err(_)) => return Err(lifecycle_unavailable_failure("request-start ack dropped")),
        Err(_) => {
            return Err(proxy_failure(
                ProxyFailureCode::RouteDeadlineExceeded,
                StatusCode::GATEWAY_TIMEOUT,
                "local request deadline exceeded during lifecycle admission",
            ));
        }
    }
    Ok(Some(RequestLifecycleAdmission { context, terminal }))
}

async fn persist_ingress_failure(
    state: &IngressState,
    context: RequestContextSnapshot,
    record_start: bool,
    annotations: RequestLogAnnotations,
    request_timing: RequestTimingContext,
    failure: ProxyFailure,
) -> ProxyFailure {
    let admission =
        match admit_request_lifecycle(state, context, record_start, annotations, request_timing)
            .await
        {
            Ok(admission) => admission,
            Err(admission_failure) => return admission_failure,
        };
    finalize_ingress_failure(admission, &failure).await;
    failure
}

async fn finalize_ingress_failure(
    admission: Option<RequestLifecycleAdmission>,
    failure: &ProxyFailure,
) {
    let Some(admission) = admission else {
        return;
    };
    let record = FinalRequestRecord::new(
        admission.context,
        RequestTerminalSnapshot {
            terminal: RequestTerminal::Failed(RequestFailure {
                code: failure.code.as_str().to_string(),
                detail: Some(failure.public_message.clone()),
            }),
            delivery: DeliveryTerminal::NotStarted,
        },
        None,
        failure
            .attempt_count()
            .unwrap_or(0)
            .clamp(0, u16::MAX as i64) as u16,
        failure
            .attempt_count()
            .unwrap_or(0)
            .saturating_sub(1)
            .clamp(0, u16::MAX as i64) as u16,
        RequestLogAnnotations {
            http_status: Some(failure.http_status.as_u16()),
            ..RequestLogAnnotations::default()
        },
    );
    let record = match failure.routing_outcome_facts() {
        Some(outcome) => record.with_routing_outcome(outcome),
        None => record,
    };
    let _ = admission.terminal.send(record).await;
}

fn lifecycle_admission_failure(error: WriterAdmissionError) -> ProxyFailure {
    let mut failure = lifecycle_unavailable_failure("local proxy lifecycle writer unavailable");
    failure.internal_detail = Some(format!("lifecycle writer admission rejected: {error:?}"));
    failure
}

fn lifecycle_write_failure(error: super::lifecycle::ports::LifecycleWriteError) -> ProxyFailure {
    let mut failure =
        lifecycle_unavailable_failure("local proxy lifecycle persistence unavailable");
    failure.internal_detail = Some(format!("lifecycle write failed: {error:?}"));
    failure
}

fn lifecycle_unavailable_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::LocalProxyBusy,
        FailureSource::Local,
        RetryClass::Never,
        StatusCode::SERVICE_UNAVAILABLE,
        message,
    )
}

fn acquire_request_lease(state: &IngressState) -> Option<RequestLease> {
    Arc::clone(&state.request_semaphore)
        .try_acquire_owned()
        .ok()
        .map(|permit| RequestLease::new(permit, Arc::clone(&state.active_requests)))
}

fn request_metadata(body: &Bytes) -> Result<ParsedRequestMetadata, ProxyFailure> {
    if body.is_empty() {
        return Ok(ParsedRequestMetadata {
            model: None,
            stream: false,
            reasoning_effort: None,
            requirements: RequestRequirements::default(),
            previous_response_id: None,
        });
    }
    let value = serde_json::from_slice::<serde_json::Value>(body).map_err(|error| {
        let mut failure = proxy_failure(
            ProxyFailureCode::RequestBodyInvalid,
            StatusCode::BAD_REQUEST,
            "request body must be JSON",
        );
        failure.internal_detail = Some(error.to_string());
        failure
    })?;
    let model = value
        .get("model")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let stream = value
        .get("stream")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    let previous_response_id = value
        .get("previous_response_id")
        .and_then(serde_json::Value::as_str)
        .map(ToString::to_string);
    let observation = RequestObservation::from_json(&value);
    let requirements = RequestRequirements {
        uses_tools: value.get("tools").is_some_and(|tools| !tools.is_null()),
        uses_vision: false,
        uses_reasoning: observation.uses_reasoning,
        ..RequestRequirements::default()
    };
    Ok(ParsedRequestMetadata {
        model,
        stream,
        reasoning_effort: observation.reasoning_effort,
        requirements,
        previous_response_id,
    })
}

fn forwarded_headers(headers: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for &name in SAFE_FORWARD_HEADERS {
        if let Some(value) = headers.get(name) {
            forwarded.insert(http::HeaderName::from_static(name), value.clone());
        }
    }
    forwarded
}

fn header_string(headers: &HeaderMap, name: &'static str) -> Option<String> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(ToString::to_string)
}

fn cors_origin(headers: &HeaderMap) -> Result<Option<&str>, ProxyFailure> {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    local_auth::allowed_origin(origin).map(Some).ok_or_else(|| {
        proxy_failure(
            ProxyFailureCode::LocalAuthInvalid,
            StatusCode::FORBIDDEN,
            "origin is not allowed",
        )
    })
}

fn with_cors(
    mut response: Response<Body>,
    request_id: &str,
    origin: Option<&str>,
) -> Response<Body> {
    response.headers_mut().insert(
        REQUEST_ID_HEADER,
        request_id.parse().expect("valid request id header"),
    );
    if let Some(origin) = origin {
        response.headers_mut().insert(
            header::ACCESS_CONTROL_ALLOW_ORIGIN,
            origin.parse().expect("valid origin header"),
        );
    }
    response
}

fn add_common_headers(
    builder: &mut http::response::Builder,
    request_id: &str,
    origin: Option<&str>,
) {
    *builder = std::mem::take(builder).header(REQUEST_ID_HEADER, request_id);
    *builder =
        std::mem::take(builder).header(header::ACCESS_CONTROL_ALLOW_METHODS, "GET,POST,OPTIONS");
    *builder = std::mem::take(builder).header(header::ACCESS_CONTROL_ALLOW_HEADERS, "authorization,content-type,accept,idempotency-key,openai-organization,openai-project,openai-beta");
    if let Some(origin) = origin {
        *builder = std::mem::take(builder).header(header::ACCESS_CONTROL_ALLOW_ORIGIN, origin);
    }
}

fn proxy_response(response: ProxyHttpResponse) -> Response<Body> {
    let mut builder = Response::builder().status(response.status);
    for (name, value) in response.headers.iter() {
        builder = builder.header(name, value);
    }
    match response.payload {
        #[cfg(test)]
        ProxyResponsePayload::Buffered(body) => {
            builder.body(Body::from(body)).expect("valid response")
        }
        ProxyResponsePayload::Stream(stream) => builder
            .body(Body::from_stream(stream.map_err(|failure| {
                std::io::Error::other(failure.public_message)
            })))
            .expect("valid response"),
    }
}

fn failure_response(failure: ProxyFailure) -> Response<Body> {
    let retry_after_ms = failure.retry_after_ms;
    let (status, body) = failure.into_response();
    let mut builder = Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json");
    let retry_after = matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS
            | StatusCode::BAD_GATEWAY
            | StatusCode::SERVICE_UNAVAILABLE
            | StatusCode::GATEWAY_TIMEOUT
    )
    .then(|| {
        retry_after_ms
            .map(|milliseconds| milliseconds.saturating_add(999) / 1_000)
            .filter(|seconds| *seconds > 0)
            .map(|seconds| seconds.min(7 * 24 * 60 * 60))
            .or_else(|| (status == StatusCode::SERVICE_UNAVAILABLE).then_some(1))
    })
    .flatten();
    if let Some(retry_after) = retry_after {
        builder = builder.header(header::RETRY_AFTER, retry_after.to_string());
    }
    builder
        .body(Body::from(body.to_string()))
        .expect("valid response")
}

fn proxy_failure(
    code: ProxyFailureCode,
    status: StatusCode,
    message: &'static str,
) -> ProxyFailure {
    ProxyFailure::new(
        code,
        FailureSource::Local,
        RetryClass::Never,
        status,
        message,
    )
}

fn next_request_id() -> String {
    static PROCESS_NAMESPACE: OnceLock<(u64, u32)> = OnceLock::new();
    static NEXT_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let &(process_started_at_ms, process_id) = PROCESS_NAMESPACE.get_or_init(|| {
        let started_at_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .min(u64::MAX as u128) as u64;
        (started_at_ms, std::process::id())
    });
    let id = NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("req_{process_started_at_ms:016x}_{process_id:08x}_{id:016x}")
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, Mutex},
        time::Duration,
    };

    use axum::body::{to_bytes, Body};
    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream};
    use http::{Method, Request, StatusCode};
    use tower::ServiceExt;

    use crate::services::proxy::limits::ProxyStartupResourceLimits;

    use super::*;

    #[test]
    fn request_ids_include_a_process_namespace() {
        let request_id = next_request_id();
        let segments = request_id.split('_').collect::<Vec<_>>();

        assert_eq!(
            segments.len(),
            4,
            "request ids must include process-start and pid namespaces so a restart cannot reuse persisted ids"
        );
        assert_eq!(segments[0], "req");
        assert!(segments[1].len() >= 13);
        assert_eq!(segments[2].len(), 8);
        assert_eq!(segments[3].len(), 16);
    }

    #[test]
    fn failure_response_preserves_retry_after_and_defaults_service_unavailable() {
        let mut rate_limited = ProxyFailure::new(
            ProxyFailureCode::UpstreamRateLimited,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::TOO_MANY_REQUESTS,
            "upstream is rate limited",
        );
        rate_limited.retry_after_ms = Some(1_001);
        let response = failure_response(rate_limited);
        assert_eq!(response.headers().get(header::RETRY_AFTER).unwrap(), "2");

        let mut bad_gateway = ProxyFailure::new(
            ProxyFailureCode::UpstreamUnavailable,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::BAD_GATEWAY,
            "upstream is unavailable",
        );
        bad_gateway.retry_after_ms = Some(7_000);
        assert_eq!(
            failure_response(bad_gateway)
                .headers()
                .get(header::RETRY_AFTER)
                .unwrap(),
            "7"
        );

        let mut rejected = ProxyFailure::new(
            ProxyFailureCode::UpstreamRequestRejected,
            FailureSource::Upstream,
            RetryClass::Never,
            StatusCode::BAD_REQUEST,
            "upstream rejected the request",
        );
        rejected.retry_after_ms = Some(7_000);
        assert!(!failure_response(rejected)
            .headers()
            .contains_key(header::RETRY_AFTER));

        let unavailable = ProxyFailure::new(
            ProxyFailureCode::UpstreamOverloaded,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::SERVICE_UNAVAILABLE,
            "upstream is overloaded",
        );
        assert_eq!(
            failure_response(unavailable)
                .headers()
                .get(header::RETRY_AFTER)
                .unwrap(),
            "1"
        );
    }

    #[tokio::test]
    async fn ingress_requires_auth_and_returns_request_id() {
        let app = test_router(test_state());
        let missing = app
            .clone()
            .oneshot(request("POST", "/v1/responses", None, b"{}"))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(error_code(missing).await, "local_auth_missing");

        let accepted = app
            .oneshot(request(
                "POST",
                "/v1/responses",
                Some("relay-local-secret"),
                br#"{"model":"gpt-test"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(accepted.status(), StatusCode::NOT_IMPLEMENTED);
        assert!(accepted.headers().contains_key("x-relay-request-id"));
    }

    #[tokio::test]
    async fn ingress_enforces_body_and_global_memory_limits() {
        let state = test_state_with_limits(ProxyStartupResourceLimits {
            max_body_bytes: 4,
            max_buffered_body_bytes: 4,
            ..test_limits()
        });
        let app = test_router(state);
        let too_large = app
            .clone()
            .oneshot(request(
                "POST",
                "/v1/responses",
                Some("relay-local-secret"),
                b"12345",
            ))
            .await
            .unwrap();
        assert_eq!(too_large.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(error_code(too_large).await, "request_body_too_large");
    }

    #[tokio::test]
    async fn ingress_times_out_while_reading_request_body() {
        let state = test_state_with_limits(ProxyStartupResourceLimits {
            body_timeout: Duration::from_millis(10),
            ..test_limits()
        });
        let app = test_router(state);
        let body = Body::from_stream(stream::pending::<Result<Bytes, std::io::Error>>());
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/responses")
            .header("authorization", "Bearer relay-local-secret")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(error_code(response).await, "request_body_timeout");
    }

    #[tokio::test]
    async fn ingress_deadline_covers_body_read_before_execution() {
        let state = test_state_with_limits_and_deadline(
            ProxyStartupResourceLimits {
                body_timeout: Duration::from_secs(1),
                ..test_limits()
            },
            Duration::from_millis(10),
        );
        let app = test_router(state);
        let body = Body::from_stream(stream::pending::<Result<Bytes, std::io::Error>>());
        let request = Request::builder()
            .method(Method::POST)
            .uri("/v1/responses")
            .header("authorization", "Bearer relay-local-secret")
            .header("content-type", "application/json")
            .body(body)
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        assert_eq!(error_code(response).await, "route_deadline_exceeded");
    }

    #[tokio::test]
    async fn ingress_routes_known_endpoints_and_rejects_unknown_method_or_path() {
        let app = test_router(test_state());
        for (method, path, expected) in [
            ("OPTIONS", "/v1/responses", StatusCode::NO_CONTENT),
            ("GET", "/usage", StatusCode::NOT_IMPLEMENTED),
            ("GET", "/v1/usage", StatusCode::NOT_IMPLEMENTED),
            ("GET", "/v1/models", StatusCode::NOT_IMPLEMENTED),
            ("POST", "/v1/chat/completions", StatusCode::NOT_IMPLEMENTED),
            ("POST", "/v1/responses", StatusCode::NOT_IMPLEMENTED),
            ("POST", "/v1/embeddings", StatusCode::NOT_IMPLEMENTED),
        ] {
            let response = app
                .clone()
                .oneshot(request(method, path, Some("relay-local-secret"), b"{}"))
                .await
                .unwrap();
            assert_eq!(response.status(), expected, "{method} {path}");
        }

        let missing = app
            .clone()
            .oneshot(request(
                "POST",
                "/v1/unknown",
                Some("relay-local-secret"),
                b"{}",
            ))
            .await
            .unwrap();
        assert_eq!(missing.status(), StatusCode::NOT_FOUND);

        let wrong_method = app
            .oneshot(request(
                "PUT",
                "/v1/responses",
                Some("relay-local-secret"),
                b"{}",
            ))
            .await
            .unwrap();
        assert_eq!(wrong_method.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn ingress_allows_only_loopback_cors_origins() {
        let app = test_router(test_state());
        let loopback = app
            .clone()
            .oneshot(with_header(
                request("OPTIONS", "/v1/responses", None, b""),
                "origin",
                "http://127.0.0.1:5173",
            ))
            .await
            .unwrap();
        assert_eq!(loopback.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            loopback
                .headers()
                .get("access-control-allow-origin")
                .unwrap(),
            "http://127.0.0.1:5173"
        );

        let remote = app
            .oneshot(with_header(
                request("OPTIONS", "/v1/responses", None, b""),
                "origin",
                "https://attacker.example",
            ))
            .await
            .unwrap();
        assert_eq!(remote.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn ingress_preserves_safe_forward_headers_only() {
        let state = test_state();
        let app = test_router(state.clone());
        let response = app
            .oneshot(with_header(
                with_header(
                    with_header(
                        with_header(
                            request(
                                "POST",
                                "/v1/responses",
                                Some("relay-local-secret"),
                                br#"{"model":"gpt-test","stream":true}"#,
                            ),
                            "accept",
                            "text/event-stream",
                        ),
                        "openai-project",
                        "project-1",
                    ),
                    "cookie",
                    "secret",
                ),
                "x-api-key",
                "secret",
            ))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_IMPLEMENTED);
        let captured = state.last_request().expect("captured request");
        assert_eq!(
            captured.forwarded_headers.get("accept").unwrap(),
            "text/event-stream"
        );
        assert_eq!(
            captured.forwarded_headers.get("openai-project").unwrap(),
            "project-1"
        );
        assert!(!captured.forwarded_headers.contains_key("cookie"));
        assert!(!captured.forwarded_headers.contains_key("x-api-key"));
    }

    fn request(
        method: &str,
        path: &str,
        bearer: Option<&str>,
        body: &'static [u8],
    ) -> Request<Body> {
        let mut builder = Request::builder()
            .method(Method::from_bytes(method.as_bytes()).unwrap())
            .uri(path)
            .header("content-type", "application/json");
        if let Some(token) = bearer {
            builder = builder.header("authorization", format!("Bearer {token}"));
        }
        builder.body(Body::from(body)).unwrap()
    }

    fn with_header(
        mut request: Request<Body>,
        name: &'static str,
        value: &'static str,
    ) -> Request<Body> {
        request.headers_mut().insert(name, value.parse().unwrap());
        request
    }

    async fn error_code(response: http::Response<Body>) -> String {
        let body = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        value["error"]["code"].as_str().unwrap().to_string()
    }

    fn test_router(state: TestIngressState) -> Router {
        router(Arc::clone(&state.state))
    }

    fn test_state() -> TestIngressState {
        test_state_with_limits(test_limits())
    }

    fn test_state_with_limits(limits: ProxyStartupResourceLimits) -> TestIngressState {
        test_state_with_limits_and_policy(
            limits,
            super::super::transport_policy::TransportPolicyStore::default(),
        )
    }

    fn test_state_with_limits_and_deadline(
        limits: ProxyStartupResourceLimits,
        deadline: Duration,
    ) -> TestIngressState {
        let mut policy =
            (*super::super::transport_policy::TransportPolicyStore::default().load()).clone();
        policy.request_deadline = deadline;
        let store = super::super::transport_policy::TransportPolicyStore::new(policy)
            .expect("test transport policy");
        test_state_with_limits_and_policy(limits, store)
    }

    fn test_state_with_limits_and_policy(
        limits: ProxyStartupResourceLimits,
        policy: super::super::transport_policy::TransportPolicyStore,
    ) -> TestIngressState {
        let captured = Arc::new(Mutex::new(None));
        let executor = Arc::new(CapturingExecutor {
            captured: Arc::clone(&captured),
        });
        TestIngressState {
            state: Arc::new(IngressState::with_active_requests_and_policy(
                "relay-local-secret",
                limits,
                executor,
                Arc::new(std::sync::atomic::AtomicU32::new(0)),
                Arc::new(std::sync::atomic::AtomicU64::new(0)),
                None,
                policy,
            )),
            captured,
        }
    }

    fn test_limits() -> ProxyStartupResourceLimits {
        ProxyStartupResourceLimits {
            body_timeout: Duration::from_secs(1),
            ..ProxyStartupResourceLimits::default()
        }
    }

    #[derive(Clone)]
    struct TestIngressState {
        state: Arc<IngressState>,
        captured: Arc<Mutex<Option<CapturedIngressRequest>>>,
    }

    impl TestIngressState {
        fn last_request(&self) -> Option<CapturedIngressRequest> {
            self.captured.lock().expect("captured request lock").clone()
        }
    }

    #[derive(Clone)]
    struct CapturedIngressRequest {
        forwarded_headers: HeaderMap,
    }

    struct CapturingExecutor {
        captured: Arc<Mutex<Option<CapturedIngressRequest>>>,
    }

    impl IngressExecutor for CapturingExecutor {
        fn execute(
            &self,
            request: CanonicalProxyRequest,
        ) -> BoxFuture<'static, Result<ProxyHttpResponse, ProxyFailure>> {
            *self.captured.lock().expect("captured request lock") = Some(CapturedIngressRequest {
                forwarded_headers: request.forwarded_headers.clone(),
            });
            Box::pin(async {
                Ok(ProxyHttpResponse {
                    status: StatusCode::NOT_IMPLEMENTED,
                    headers: HeaderMap::new(),
                    payload: ProxyResponsePayload::Buffered(Bytes::from_static(
                        br#"{"error":{"message":"v2 execution not wired","type":"relay_pool_error","param":null,"code":"v2_execution_not_wired"}}"#,
                    )),
                })
            })
        }
    }
}
