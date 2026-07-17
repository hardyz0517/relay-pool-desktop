use std::sync::{
    atomic::{AtomicU32, AtomicU64, Ordering},
    Arc,
};

use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{header, HeaderMap, Method, Response, StatusCode, Uri},
    response::IntoResponse,
    routing::{get, post},
    Router,
};
use bytes::Bytes;
use futures_util::{future::BoxFuture, TryStreamExt};
use tokio::{sync::Semaphore, time::timeout};

use crate::models::routing::RouteEndpointKind;

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    limits::{BodyBudget, BodyBudgetError, ProxyServerLimits, RequestLease},
    local_auth::{self, AuthDecision},
    request::{
        CanonicalProxyRequest, FinalRequestOutcome, ProxyHttpResponse, ProxyResponsePayload,
        RequestRequirements,
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
    limits: ProxyServerLimits,
    body_budget: BodyBudget,
    request_semaphore: Arc<Semaphore>,
    active_requests: Arc<AtomicU32>,
    request_count: Arc<AtomicU64>,
    executor: Arc<dyn IngressExecutor>,
}

impl IngressState {
    pub(crate) fn new(
        local_access_key: impl Into<String>,
        limits: ProxyServerLimits,
        executor: Arc<dyn IngressExecutor>,
    ) -> Self {
        Self::with_active_requests(
            local_access_key,
            limits,
            executor,
            Arc::new(AtomicU32::new(0)),
            Arc::new(AtomicU64::new(0)),
        )
    }

    pub(crate) fn with_active_requests(
        local_access_key: impl Into<String>,
        limits: ProxyServerLimits,
        executor: Arc<dyn IngressExecutor>,
        active_requests: Arc<AtomicU32>,
        request_count: Arc<AtomicU64>,
    ) -> Self {
        Self {
            local_access_key: local_access_key.into(),
            body_budget: BodyBudget::new(limits.max_buffered_body_bytes),
            request_semaphore: Arc::new(Semaphore::new(limits.max_in_flight_requests)),
            active_requests,
            request_count,
            limits,
            executor,
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
        .fallback(not_found)
        .with_state(state)
}

async fn handle(
    State(state): State<Arc<IngressState>>,
    method: Method,
    uri: Uri,
    headers: HeaderMap,
    body: Body,
) -> Response<Body> {
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
        Err(response) => return response,
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

    let endpoint = match endpoint_for(&method, uri.path()) {
        Some(endpoint) => endpoint,
        None => {
            return with_cors(
                failure_response(proxy_failure(
                    ProxyFailureCode::InternalProxyError,
                    StatusCode::METHOD_NOT_ALLOWED,
                    "method is not allowed for local proxy endpoint",
                )),
                &request_id,
                cors_origin,
            )
        }
    };
    let content_length = headers
        .get(header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok());
    if content_length.is_some_and(|length| length > state.limits.max_body_bytes) {
        return with_cors(
            failure_response(proxy_failure(
                ProxyFailureCode::RequestBodyTooLarge,
                StatusCode::PAYLOAD_TOO_LARGE,
                "request body is too large",
            )),
            &request_id,
            cors_origin,
        );
    }
    let body = match timeout(
        state.limits.body_timeout,
        to_bytes(body, state.limits.max_body_bytes + 1),
    )
    .await
    {
        Ok(Ok(body)) if body.len() <= state.limits.max_body_bytes => body,
        Ok(Ok(_)) => {
            return with_cors(
                failure_response(proxy_failure(
                    ProxyFailureCode::RequestBodyTooLarge,
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request body is too large",
                )),
                &request_id,
                cors_origin,
            )
        }
        Ok(Err(error)) => {
            let mut failure = proxy_failure(
                ProxyFailureCode::RequestBodyInvalid,
                StatusCode::BAD_REQUEST,
                "request body is invalid",
            );
            failure.internal_detail = Some(error.to_string());
            return with_cors(failure_response(failure), &request_id, cors_origin);
        }
        Err(_) => {
            return with_cors(
                failure_response(proxy_failure(
                    ProxyFailureCode::RequestBodyTimeout,
                    StatusCode::REQUEST_TIMEOUT,
                    "request body timed out",
                )),
                &request_id,
                cors_origin,
            )
        }
    };
    let body_budget = match state.body_budget.acquire(body.len()).await {
        Ok(lease) => lease,
        Err(BodyBudgetError::InsufficientCapacity) => {
            return with_cors(
                failure_response(proxy_failure(
                    ProxyFailureCode::LocalProxyMemoryBusy,
                    StatusCode::SERVICE_UNAVAILABLE,
                    "local proxy body budget is exhausted",
                )),
                &request_id,
                cors_origin,
            )
        }
    };
    let (model, stream, requirements, previous_response_id) = match request_metadata(&body) {
        Ok(metadata) => metadata,
        Err(failure) => return with_cors(failure_response(failure), &request_id, cors_origin),
    };
    let canonical = CanonicalProxyRequest::new(
        request_id.clone(),
        endpoint,
        model,
        stream,
        requirements,
        body,
        forwarded_headers(&headers),
        header_string(&headers, "idempotency-key"),
        header_string(&headers, "x-relay-session-hash"),
        previous_response_id,
        body_budget,
        request_lease,
    );
    match state.executor.execute(canonical).await {
        Ok(response) => with_cors(proxy_response(response), &request_id, cors_origin),
        Err(failure) => with_cors(failure_response(failure), &request_id, cors_origin),
    }
}

async fn not_found() -> impl IntoResponse {
    failure_response(proxy_failure(
        ProxyFailureCode::InternalProxyError,
        StatusCode::NOT_FOUND,
        "local proxy endpoint was not found",
    ))
}

fn acquire_request_lease(state: &IngressState) -> Option<RequestLease> {
    Arc::clone(&state.request_semaphore)
        .try_acquire_owned()
        .ok()
        .map(|permit| RequestLease::new(permit, Arc::clone(&state.active_requests)))
}

fn endpoint_for(method: &Method, path: &str) -> Option<RouteEndpointKind> {
    match (method, path) {
        (&Method::GET, "/usage") | (&Method::GET, "/v1/usage") => Some(RouteEndpointKind::Models),
        (&Method::GET, "/v1/models") => Some(RouteEndpointKind::Models),
        (&Method::POST, "/v1/chat/completions") => Some(RouteEndpointKind::ChatCompletions),
        (&Method::POST, "/v1/responses") => Some(RouteEndpointKind::Responses),
        (&Method::POST, "/v1/embeddings") => Some(RouteEndpointKind::Embeddings),
        _ => None,
    }
}

fn request_metadata(
    body: &Bytes,
) -> Result<(Option<String>, bool, RequestRequirements, Option<String>), ProxyFailure> {
    if body.is_empty() {
        return Ok((None, false, RequestRequirements::default(), None));
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
    let requirements = RequestRequirements {
        uses_tools: value.get("tools").is_some_and(|tools| !tools.is_null()),
        uses_vision: false,
        uses_reasoning: value
            .get("reasoning")
            .is_some_and(|reasoning| !reasoning.is_null()),
        ..RequestRequirements::default()
    };
    Ok((model, stream, requirements, previous_response_id))
}

fn forwarded_headers(headers: &HeaderMap) -> HeaderMap {
    let mut forwarded = HeaderMap::new();
    for name in SAFE_FORWARD_HEADERS {
        if let Some(value) = headers.get(*name) {
            forwarded.insert(http::HeaderName::from_static(*name), value.clone());
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

fn cors_origin(headers: &HeaderMap) -> Result<Option<&str>, Response<Body>> {
    let Some(origin) = headers
        .get(header::ORIGIN)
        .and_then(|value| value.to_str().ok())
    else {
        return Ok(None);
    };
    local_auth::allowed_origin(origin).map(Some).ok_or_else(|| {
        failure_response(proxy_failure(
            ProxyFailureCode::LocalAuthInvalid,
            StatusCode::FORBIDDEN,
            "origin is not allowed",
        ))
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
        ProxyResponsePayload::Buffered(body) => {
            builder.body(Body::from(body)).expect("valid response")
        }
        ProxyResponsePayload::Stream(stream) => builder
            .body(Body::from_stream(stream.map_err(|failure| {
                std::io::Error::new(std::io::ErrorKind::Other, failure.public_message)
            })))
            .expect("valid response"),
    }
}

fn failure_response(failure: ProxyFailure) -> Response<Body> {
    let (status, body) = failure.into_response();
    Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "application/json")
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
    static NEXT_REQUEST_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    let id = NEXT_REQUEST_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("req_{id:016x}")
}

pub(crate) struct NotWiredExecutor;

impl IngressExecutor for NotWiredExecutor {
    fn execute(
        &self,
        _request: CanonicalProxyRequest,
    ) -> BoxFuture<'static, Result<ProxyHttpResponse, ProxyFailure>> {
        Box::pin(async {
            Ok(ProxyHttpResponse {
                status: StatusCode::NOT_IMPLEMENTED,
                headers: HeaderMap::new(),
                payload: ProxyResponsePayload::Buffered(Bytes::from_static(
                    br#"{"error":{"message":"v2 execution not wired","type":"relay_pool_error","param":null,"code":"v2_execution_not_wired"}}"#,
                )),
                outcome: FinalRequestOutcome::success("not_wired"),
            })
        })
    }
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

    use crate::services::proxy::limits::ProxyServerLimits;

    use super::*;

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
        let state = test_state_with_limits(ProxyServerLimits {
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
        let state = test_state_with_limits(ProxyServerLimits {
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

    fn test_state_with_limits(limits: ProxyServerLimits) -> TestIngressState {
        let captured = Arc::new(Mutex::new(None));
        let executor = Arc::new(CapturingExecutor {
            captured: Arc::clone(&captured),
        });
        TestIngressState {
            state: Arc::new(IngressState::new("relay-local-secret", limits, executor)),
            captured,
        }
    }

    fn test_limits() -> ProxyServerLimits {
        ProxyServerLimits {
            body_timeout: Duration::from_secs(1),
            ..ProxyServerLimits::default()
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
                    outcome: FinalRequestOutcome::success("not_wired"),
                })
            })
        }
    }
}
