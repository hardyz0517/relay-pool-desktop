use std::{collections::HashMap, fmt, sync::Arc};

use bytes::{Bytes, BytesMut};
use futures_util::{StreamExt, TryStreamExt};
use http::{header, HeaderMap, HeaderValue, StatusCode};
use tokio::sync::RwLock;

use crate::{
    application::{
        operational_facts::target_resolver::ExecutionTargetHandle,
        request_finalization::failure::{
            failure_from_provider_signal, CapabilityApplicabilitySet, ProviderErrorSemanticSignal,
        },
    },
    services::{outbound::current_system_proxy_url, station_endpoints::build_api_url},
};

use super::{
    adapters::error_envelope::MAX_ERROR_BODY_BYTES,
    diagnostic_memory::{DiagnosticMemoryBudget, DiagnosticMemoryPermit},
    endpoint_adapter::PreparedUpstreamRequest,
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    limits::ProxyServerLimits,
    protocol::TransportMode,
    redact_error_message,
    request::ByteStream,
    request_send::RequestSendPhase,
};

// reqwest's response stream yields an owned Bytes chunk before our capture loop can
// inspect it. Reserve one maximum decoded chunk in addition to the retained body so
// the admission contract covers both simultaneously-owned buffers. Automatic content
// decoding is disabled in build_client, so this is also the complete decoder scratch
// bound for diagnostic bodies.
const HTTP_DIAGNOSTIC_CHUNK_BYTES: usize = 64 * 1024;
// `BytesMut` may grow geometrically. Charge the next power-of-two capacity,
// not only the logical max+1 marker length, before reading an untrusted body.
const HTTP_DIAGNOSTIC_RETAINED_CAPACITY_BYTES: usize =
    (MAX_ERROR_BODY_BYTES + 1).next_power_of_two();

#[derive(Clone)]
pub(crate) struct UpstreamClientPool {
    direct: Arc<reqwest::Client>,
    proxied: Arc<RwLock<HashMap<ProxyRoute, Arc<reqwest::Client>>>>,
    limits: ProxyServerLimits,
    diagnostic_memory: DiagnosticMemoryBudget,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum ProxyRoute {
    Direct,
    System,
    Http(String),
    Socks(String),
}

pub(crate) enum UpstreamAttempt {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        diagnostic_memory: Option<DiagnosticMemoryPermit>,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: ByteStream,
    },
}

impl fmt::Debug for UpstreamAttempt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered {
                status,
                headers,
                body,
                ..
            } => formatter
                .debug_struct("Buffered")
                .field("status", status)
                .field("headers", headers)
                .field("body_len", &body.len())
                .finish(),
            Self::Stream {
                status, headers, ..
            } => formatter
                .debug_struct("Stream")
                .field("status", status)
                .field("headers", headers)
                .finish(),
        }
    }
}

impl UpstreamClientPool {
    pub(crate) fn new(
        limits: ProxyServerLimits,
        diagnostic_memory: DiagnosticMemoryBudget,
    ) -> Result<Self, ProxyFailure> {
        Ok(Self {
            direct: Arc::new(build_client(&ProxyRoute::Direct, &limits)?),
            proxied: Arc::new(RwLock::new(HashMap::new())),
            limits,
            diagnostic_memory,
        })
    }

    pub(crate) async fn client(
        &self,
        route: &ProxyRoute,
    ) -> Result<Arc<reqwest::Client>, ProxyFailure> {
        if matches!(route, ProxyRoute::Direct) {
            return Ok(Arc::clone(&self.direct));
        }

        if let Some(client) = self.proxied.read().await.get(route).cloned() {
            return Ok(client);
        }

        let mut clients = self.proxied.write().await;
        if let Some(client) = clients.get(route).cloned() {
            return Ok(client);
        }
        let client = Arc::new(build_client(route, &self.limits)?);
        clients.insert(route.clone(), Arc::clone(&client));
        Ok(client)
    }

    pub(crate) fn diagnostic_memory_budget(&self) -> DiagnosticMemoryBudget {
        self.diagnostic_memory.clone()
    }

    pub(crate) async fn send_resolved(
        &self,
        prepared: PreparedUpstreamRequest,
        target: &ExecutionTargetHandle,
    ) -> Result<UpstreamAttempt, ProxyFailure> {
        self.send_with_parts(
            prepared,
            &target.collector_proxy_mode,
            target.collector_proxy_url.as_deref(),
            &target.api_base_url,
            target.api_key.as_bytes(),
            &target.station_id,
            target.endpoint_revision,
        )
        .await
    }

    async fn send_with_parts(
        &self,
        prepared: PreparedUpstreamRequest,
        collector_proxy_mode: &str,
        collector_proxy_url: Option<&str>,
        api_base_url: &str,
        api_key: &[u8],
        station_id: &str,
        endpoint_revision: i64,
    ) -> Result<UpstreamAttempt, ProxyFailure> {
        let route = ProxyRoute::from_candidate_parts(collector_proxy_mode, collector_proxy_url)?;
        let url = build_api_url(api_base_url, &prepared.path).map_err(internal_proxy_failure)?;
        let client = self.client(&route).await?;
        let method = reqwest::Method::from_bytes(prepared.method.as_str().as_bytes())
            .map_err(|error| internal_proxy_failure(format!("invalid upstream method: {error}")))?;
        let mut request = client.request(method, url);
        for (name, value) in prepared.headers.iter() {
            request = request.header(name.as_str(), value.clone());
        }
        let mut authorization = Vec::with_capacity("Bearer ".len() + api_key.len());
        authorization.extend_from_slice(b"Bearer ");
        authorization.extend_from_slice(api_key);
        request = request.header(
            header::AUTHORIZATION.as_str(),
            HeaderValue::from_bytes(&authorization).map_err(|error| {
                internal_proxy_failure(format!("invalid upstream authorization header: {error}"))
            })?,
        );
        if !prepared.body.is_empty() {
            request = request.body(prepared.body.clone());
        }

        let response = request
            .send()
            .await
            .map_err(|error| upstream_send_failure(error, station_id, endpoint_revision))?;
        let status = response.status();
        let headers = response.headers().clone();
        match prepared.response_plan.transport {
            TransportMode::Streaming if status.is_success() => {
                let chunks = response
                    .bytes_stream()
                    .map_err(upstream_stream_failure)
                    .boxed();
                Ok(UpstreamAttempt::Stream {
                    status,
                    headers,
                    chunks,
                })
            }
            TransportMode::Buffered | TransportMode::Streaming => {
                // Error payloads are diagnostic evidence, not application data. Keep their
                // retention ceiling independent from the (also bounded) successful buffered
                // response ceiling. In particular, never use `Response::bytes()` here: a relay
                // controls the body length and could otherwise make one attempt retain an
                // unbounded allocation.
                let body_limit = if status.is_success() {
                    self.limits.max_buffered_body_bytes
                } else {
                    MAX_ERROR_BODY_BYTES
                };
                let captured = capture_bounded_body(
                    response,
                    body_limit,
                    (!status.is_success()).then_some(&self.diagnostic_memory),
                )
                .await?;
                let body = match captured {
                    CapturedBody::Complete {
                        body,
                        _diagnostic_memory,
                    } => (body, _diagnostic_memory),
                    CapturedBody::OverLimit {
                        body: prefix,
                        _diagnostic_memory,
                    } if !status.is_success() => {
                        // Preserve a bounded over-limit marker for the evidence parser. Its
                        // `BodyCapture::Complete` contract deliberately treats max+1 bytes as
                        // `ErrorBodyTooLarge` without trying to parse or classify body fields.
                        (prefix, _diagnostic_memory)
                    }
                    CapturedBody::OverLimit { .. } => {
                        return Err(upstream_buffered_body_too_large_failure(body_limit));
                    }
                };
                Ok(UpstreamAttempt::Buffered {
                    status,
                    headers,
                    body: body.0,
                    diagnostic_memory: body.1,
                })
            }
        }
    }
}

#[derive(Debug)]
enum CapturedBody {
    Complete {
        body: Bytes,
        _diagnostic_memory: Option<DiagnosticMemoryPermit>,
    },
    /// Contains at most `limit + 1` bytes. The extra byte is the unambiguous
    /// over-limit marker consumed by the error-envelope parser.
    OverLimit {
        body: Bytes,
        _diagnostic_memory: Option<DiagnosticMemoryPermit>,
    },
}

async fn capture_bounded_body(
    response: reqwest::Response,
    limit: usize,
    diagnostic_memory: Option<&DiagnosticMemoryBudget>,
) -> Result<CapturedBody, ProxyFailure> {
    let content_length_over_limit = response
        .content_length()
        .is_some_and(|length| length > limit as u64);
    let initial_capacity = response_body_initial_capacity(limit, content_length_over_limit);
    let diagnostic_reservation = diagnostic_memory
        .map(|budget| {
            budget
                .try_reserve(
                    HTTP_DIAGNOSTIC_RETAINED_CAPACITY_BYTES
                        .saturating_add(HTTP_DIAGNOSTIC_CHUNK_BYTES),
                )
                .map_err(|_| diagnostic_memory_saturated_failure())
        })
        .transpose()?;
    let mut chunks = response.bytes_stream();
    let mut retained = BytesMut::with_capacity(initial_capacity);

    while let Some(chunk) = chunks.next().await {
        let chunk = chunk.map_err(upstream_response_body_failure)?;
        let remaining_with_marker = limit.saturating_add(1).saturating_sub(retained.len());
        if remaining_with_marker == 0 {
            return Ok(CapturedBody::OverLimit {
                body: retained.freeze(),
                _diagnostic_memory: diagnostic_reservation,
            });
        }
        let keep = chunk.len().min(remaining_with_marker);
        retained.extend_from_slice(&chunk[..keep]);
        if retained.len() > limit || keep < chunk.len() {
            return Ok(CapturedBody::OverLimit {
                body: retained.freeze(),
                _diagnostic_memory: diagnostic_reservation,
            });
        }
    }

    Ok(CapturedBody::Complete {
        body: retained.freeze(),
        _diagnostic_memory: diagnostic_reservation,
    })
}

fn response_body_initial_capacity(limit: usize, declared_over_limit: bool) -> usize {
    if declared_over_limit {
        return 0;
    }
    // Start small and let BytesMut grow only up to the hard limit above.
    limit.saturating_add(1).min(16 * 1024)
}

impl ProxyRoute {
    pub(crate) fn from_candidate_parts(
        mode: &str,
        url: Option<&str>,
    ) -> Result<Self, ProxyFailure> {
        match mode.trim().to_ascii_lowercase().as_str() {
            "" | "direct" | "inherit" => Ok(Self::Direct),
            "system" => match current_system_proxy_url() {
                Some(url) => proxy_route_from_url(&url),
                None => Ok(Self::System),
            },
            "manual" => proxy_route_from_url(required_proxy_url(url)?),
            "http" | "http_proxy" | "https" | "https_proxy" => {
                let url = required_proxy_url(url)?;
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(invalid_proxy_failure(
                        "HTTP proxy route requires http(s) URL",
                    ));
                }
                Ok(Self::Http(url.to_string()))
            }
            "socks" | "socks5" | "socks_proxy" => {
                let url = required_proxy_url(url)?;
                if !(url.starts_with("socks5://") || url.starts_with("socks5h://")) {
                    return Err(invalid_proxy_failure(
                        "SOCKS proxy route requires socks5 URL",
                    ));
                }
                Ok(Self::Socks(url.to_string()))
            }
            other => Err(invalid_proxy_failure(format!(
                "unsupported upstream proxy mode: {other}"
            ))),
        }
    }
}

fn build_client(
    route: &ProxyRoute,
    limits: &ProxyServerLimits,
) -> Result<reqwest::Client, ProxyFailure> {
    let mut builder = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(limits.upstream_connect_timeout)
        .pool_idle_timeout(Some(limits.stream_idle_timeout))
        // Diagnostic capture is bounded in wire bytes. Keep automatic content
        // decoding disabled even if a future reqwest feature set enables it;
        // decoded-body admission would require a separate resource contract.
        .no_gzip()
        .no_deflate()
        .no_brotli();
    match route {
        ProxyRoute::Direct => {
            builder = builder.no_proxy();
        }
        ProxyRoute::System => {}
        ProxyRoute::Http(url) | ProxyRoute::Socks(url) => {
            let proxy = reqwest::Proxy::all(url).map_err(|error| {
                invalid_proxy_failure(format!("invalid upstream proxy URL: {error}"))
            })?;
            builder = builder.proxy(proxy);
        }
    }
    builder
        .build()
        .map_err(|error| internal_proxy_failure(format!("build upstream client failed: {error}")))
}

fn proxy_route_from_url(url: &str) -> Result<ProxyRoute, ProxyFailure> {
    let url = url.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        return Ok(ProxyRoute::Http(url.to_string()));
    }
    if url.starts_with("socks5://") || url.starts_with("socks5h://") {
        return Ok(ProxyRoute::Socks(url.to_string()));
    }
    Err(invalid_proxy_failure(
        "proxy URL must start with http(s):// or socks5(h)://",
    ))
}

fn required_proxy_url(url: Option<&str>) -> Result<&str, ProxyFailure> {
    let Some(url) = url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Err(invalid_proxy_failure("proxy URL is required"));
    };
    Ok(url)
}

fn upstream_send_failure(
    error: reqwest::Error,
    station_id: &str,
    endpoint_revision: i64,
) -> ProxyFailure {
    let connection_not_established = error.is_connect();
    // Transport evidence flows through the single canonical classifier so
    // retry/health/capability/public consumers all see the same outcome.
    let canonical = failure_from_provider_signal(
        ProviderErrorSemanticSignal::Transport {
            station_id: station_id.to_string(),
            endpoint_revision,
        },
        CapabilityApplicabilitySet::UnknownModelCatalog,
    );
    let mut failure = ProxyFailure::from_canonical(canonical);
    failure.internal_detail = Some(redact_error_message(&format!(
        "upstream request failed: {error}"
    )));
    if connection_not_established {
        failure.internal_detail = Some("connection_not_established".to_string());
    }
    failure.with_request_send_phase(if connection_not_established {
        RequestSendPhase::NotConnected
    } else {
        RequestSendPhase::Unknown
    })
}

fn upstream_stream_failure(error: reqwest::Error) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        StatusCode::BAD_GATEWAY,
        redact_error_message(&format!("upstream stream failed: {error}")),
    )
    .with_request_send_phase(RequestSendPhase::ResponseStarted)
}

fn upstream_buffered_body_too_large_failure(limit: usize) -> ProxyFailure {
    let mut failure = ProxyFailure::new(
        ProxyFailureCode::UpstreamMalformedResponse,
        FailureSource::Upstream,
        RetryClass::Never,
        StatusCode::BAD_GATEWAY,
        "upstream buffered response exceeded the configured limit",
    );
    failure.internal_detail = Some(format!("buffered_body_limit_bytes={limit}"));
    failure.with_request_send_phase(RequestSendPhase::ResponseStarted)
}

fn upstream_response_body_failure(error: reqwest::Error) -> ProxyFailure {
    upstream_stream_failure(error).with_request_send_phase(RequestSendPhase::ResponseStarted)
}

fn diagnostic_memory_saturated_failure() -> ProxyFailure {
    let mut failure = ProxyFailure::new(
        ProxyFailureCode::LocalProxyMemoryBusy,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::SERVICE_UNAVAILABLE,
        "local diagnostic memory is saturated",
    );
    failure.internal_detail = Some("diagnostic_memory_saturated".to_string());
    failure
}

fn invalid_proxy_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::InternalProxyError,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::BAD_GATEWAY,
        message,
    )
}

fn internal_proxy_failure(message: impl Into<String>) -> ProxyFailure {
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
    use std::{sync::Arc, time::Duration};

    use bytes::Bytes;
    use http::{HeaderMap, Method, StatusCode};

    use crate::{
        application::{
            credentials::SecretBytes,
            operational_facts::target_resolver::ExecutionTargetHandle,
            routing_engine::capacity::{
                CompositeCapacityRegistry, CompositeCapacityRequest, ProviderAccountConstraint,
            },
        },
        models::proxy::UpstreamApiFormat,
        services::proxy::{
            endpoint_adapter::PreparedUpstreamRequest,
            error::ProxyFailureCode,
            limits::ProxyServerLimits,
            protocol::{
                CompletionPolicy, DownstreamTransform, ResponsePlan, TransportMode,
                UpstreamProtocol,
            },
            test_support::{LoopbackUpstream, ScriptedResponse},
        },
    };

    use super::{DiagnosticMemoryBudget, ProxyRoute, UpstreamAttempt, UpstreamClientPool};

    #[tokio::test]
    async fn upstream_transport_reuses_clients_and_never_follows_redirects() {
        let pool =
            UpstreamClientPool::new(test_limits(), DiagnosticMemoryBudget::new(32 * 1024 * 1024))
                .expect("pool");
        assert!(Arc::ptr_eq(
            &pool
                .client(&ProxyRoute::Direct)
                .await
                .expect("direct client"),
            &pool
                .client(&ProxyRoute::Direct)
                .await
                .expect("direct client")
        ));
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Redirect {
            location: "https://other.example/secret".to_string(),
        }]);

        let outcome = pool
            .send_resolved(
                prepared_request("/v1/models"),
                &test_target(&upstream.base_url),
            )
            .await
            .expect("upstream outcome");

        assert_eq!(outcome.status(), StatusCode::FOUND);
    }

    #[tokio::test]
    async fn upstream_transport_classifies_connect_timeout_and_http_status() {
        let pool = UpstreamClientPool::new(
            short_limits(),
            DiagnosticMemoryBudget::new(32 * 1024 * 1024),
        )
        .expect("pool");
        let connect = pool
            .send_resolved(
                prepared_request("/v1/models"),
                &test_target("http://127.0.0.1:9"),
            )
            .await
            .expect_err("connect failure");
        assert_eq!(connect.code, ProxyFailureCode::UpstreamConnectFailed);
        assert_eq!(
            connect.request_send_phase,
            super::RequestSendPhase::NotConnected,
            "reqwest exposes connect errors as the only pre-write fact this transport can trust"
        );

        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Status {
            status: 429,
            reason: "Too Many Requests",
        }]);
        let status = pool
            .send_resolved(
                prepared_request("/v1/models"),
                &test_target(&upstream.base_url),
            )
            .await
            .expect("http status response");

        assert_eq!(status.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn upstream_disconnect_after_receiving_request_stays_unknown() {
        let pool = UpstreamClientPool::new(
            short_limits(),
            DiagnosticMemoryBudget::new(32 * 1024 * 1024),
        )
        .expect("pool");
        let upstream = RawLoopback::disconnect_after_request();

        let failure = pool
            .send_resolved(
                prepared_post_request("/v1/chat/completions", Bytes::from_static(b"{}")),
                &test_target(&upstream.base_url),
            )
            .await
            .expect_err("a peer close before response headers must fail the request");

        // The fixture proves the peer accepted a request and read bytes, but reqwest
        // does not expose the write boundary to us. Reporting NotConnected here would
        // authorize an unsafe transparent replay of a non-idempotent request.
        assert_eq!(failure.request_send_phase, super::RequestSendPhase::Unknown);
    }

    #[tokio::test]
    async fn upstream_error_body_capture_is_hard_bounded() {
        let pool =
            UpstreamClientPool::new(test_limits(), DiagnosticMemoryBudget::new(32 * 1024 * 1024))
                .expect("pool");
        let upstream = RawLoopback::serve(
            500,
            vec![b'x'; super::MAX_ERROR_BODY_BYTES.saturating_add(64 * 1024)],
        );
        let outcome = pool
            .send_resolved(
                prepared_request("/v1/models"),
                &test_target(&upstream.base_url),
            )
            .await
            .expect("bounded upstream outcome");
        match outcome {
            UpstreamAttempt::Buffered { status, body, .. } => {
                assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
                assert_eq!(body.len(), super::MAX_ERROR_BODY_BYTES + 1);
            }
            UpstreamAttempt::Stream { .. } => panic!("error status must be buffered"),
        }
    }

    #[tokio::test]
    async fn encoded_error_bodies_remain_wire_byte_bounded_for_every_supported_encoding() {
        let pool =
            UpstreamClientPool::new(test_limits(), DiagnosticMemoryBudget::new(32 * 1024 * 1024))
                .expect("pool");
        for encoding in ["gzip", "deflate", "br"] {
            let content_encoding = format!("Content-Encoding: {encoding}");
            let upstream = RawLoopback::serve_with_headers(
                500,
                vec![b'x'; super::MAX_ERROR_BODY_BYTES + 1],
                &[content_encoding.as_str()],
            );

            let outcome = pool
                .send_resolved(
                    prepared_request("/v1/models"),
                    &test_target(&upstream.base_url),
                )
                .await
                .expect("bounded encoded error outcome");
            match outcome {
                UpstreamAttempt::Buffered { body, .. } => assert_eq!(
                    body.len(),
                    super::MAX_ERROR_BODY_BYTES + 1,
                    "{encoding}: the client must retain only wire bytes; decoded-body admission requires a separate contract"
                ),
                UpstreamAttempt::Stream { .. } => panic!("error status must be buffered"),
            }
        }
    }

    #[tokio::test]
    async fn error_capture_fails_fast_when_shared_diagnostic_memory_is_saturated() {
        let budget = DiagnosticMemoryBudget::new(
            super::HTTP_DIAGNOSTIC_RETAINED_CAPACITY_BYTES + super::HTTP_DIAGNOSTIC_CHUNK_BYTES,
        );
        let blocker = budget
            .try_reserve(1)
            .expect("occupy one byte of the shared budget");
        let pool = UpstreamClientPool::new(test_limits(), budget.clone()).expect("pool");
        let upstream = RawLoopback::serve(500, b"diagnostic".to_vec());

        let failure = pool
            .send_resolved(
                prepared_request("/v1/models"),
                &test_target(&upstream.base_url),
            )
            .await
            .expect_err("saturation must reject without buffering");
        assert_eq!(failure.code, ProxyFailureCode::LocalProxyMemoryBusy);
        assert_eq!(
            failure.internal_detail.as_deref(),
            Some("diagnostic_memory_saturated")
        );
        assert_eq!(budget.retained(), 1);
        drop(blocker);
        assert_eq!(budget.retained(), 0);
    }

    #[tokio::test]
    async fn successful_buffered_response_uses_its_separate_configured_limit() {
        let limits = ProxyServerLimits {
            max_buffered_body_bytes: 32,
            ..test_limits()
        };
        let pool = UpstreamClientPool::new(limits, DiagnosticMemoryBudget::new(32 * 1024 * 1024))
            .expect("pool");
        let upstream = RawLoopback::serve(200, vec![b'x'; 64]);
        let failure = pool
            .send_resolved(
                prepared_request("/v1/models"),
                &test_target(&upstream.base_url),
            )
            .await
            .expect_err("oversized successful response must fail closed");
        assert_eq!(failure.code, ProxyFailureCode::UpstreamMalformedResponse);
    }

    #[test]
    fn upstream_transport_validates_http_and_socks_proxy_urls() {
        assert_eq!(
            ProxyRoute::from_candidate_parts("direct", None).expect("direct"),
            ProxyRoute::Direct
        );
        assert!(
            ProxyRoute::from_candidate_parts("system", None).is_ok(),
            "system proxy mode is a valid station/global setting and must not be rejected"
        );
        assert_eq!(
            ProxyRoute::from_candidate_parts("http", Some("http://127.0.0.1:8888"))
                .expect("http proxy"),
            ProxyRoute::Http("http://127.0.0.1:8888".to_string())
        );
        assert_eq!(
            ProxyRoute::from_candidate_parts("manual", Some("http://127.0.0.1:8888"))
                .expect("manual http proxy"),
            ProxyRoute::Http("http://127.0.0.1:8888".to_string())
        );
        assert_eq!(
            ProxyRoute::from_candidate_parts("socks", Some("socks5://127.0.0.1:1080"))
                .expect("socks proxy"),
            ProxyRoute::Socks("socks5://127.0.0.1:1080".to_string())
        );
        assert_eq!(
            ProxyRoute::from_candidate_parts("manual", Some("socks5://127.0.0.1:1080"))
                .expect("manual socks proxy"),
            ProxyRoute::Socks("socks5://127.0.0.1:1080".to_string())
        );
        assert!(ProxyRoute::from_candidate_parts("http", Some("socks5://127.0.0.1:1080")).is_err());
        assert!(ProxyRoute::from_candidate_parts("socks", Some("http://127.0.0.1:8888")).is_err());
    }

    fn prepared_request(path: &str) -> PreparedUpstreamRequest {
        PreparedUpstreamRequest {
            method: Method::GET,
            path: path.to_string(),
            headers: HeaderMap::new(),
            body: Bytes::new(),
            response_plan: ResponsePlan {
                transport: TransportMode::Buffered,
                upstream_protocol: UpstreamProtocol::ModelsJson,
                downstream_transform: DownstreamTransform::Passthrough,
                completion_policy: CompletionPolicy::ValidatedJsonBody,
            },
        }
    }

    fn prepared_post_request(path: &str, body: Bytes) -> PreparedUpstreamRequest {
        PreparedUpstreamRequest {
            method: Method::POST,
            path: path.to_string(),
            headers: HeaderMap::new(),
            body,
            response_plan: ResponsePlan {
                transport: TransportMode::Buffered,
                upstream_protocol: UpstreamProtocol::ChatCompletionsJson,
                downstream_transform: DownstreamTransform::Passthrough,
                completion_policy: CompletionPolicy::ValidatedJsonBody,
            },
        }
    }

    fn test_target(upstream_base_url: &str) -> ExecutionTargetHandle {
        let capacity = CompositeCapacityRegistry::default();
        let lease = capacity
            .try_acquire(CompositeCapacityRequest {
                station_id: "station-test".to_string(),
                station_key_id: "key-test".to_string(),
                half_open_probe_id: None,
                global_max_concurrency: 8,
                station_account_max_concurrency: 8,
                station_key_max_concurrency: 8,
                provider_account_constraint: ProviderAccountConstraint::NotApplicable,
            })
            .expect("test capacity lease");
        ExecutionTargetHandle {
            station_key_id: "key-test".to_string(),
            station_id: "station-test".to_string(),
            station_type: "openai_compatible".to_string(),
            group_binding_id: None,
            endpoint_revision: 1,
            api_base_url: upstream_base_url.to_string(),
            upstream_api_format: UpstreamApiFormat::Auto,
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            api_key: SecretBytes::from("sk-upstream-test".to_string()),
            commitment: crate::application::operational_facts::target_resolver::TargetExecutionCommitment {
                version: crate::application::operational_facts::target_resolver::TARGET_EXECUTION_COMMITMENT_VERSION,
                station_key_id: "key-test".to_string(),
                station_id: "station-test".to_string(),
                station_type: "openai_compatible".to_string(),
                credential_revision: 1,
                endpoint_revision: 1,
                account_revision: 1,
                group_binding_id: None,
                group_revision: None,
                resolved_upstream_model: Some("gpt-test".to_string()),
                model_alias_revision: 1,
                capacity_domain: None,
                capacity_domain_revision: None,
                policy_revision: 1,
                request_body_identity: crate::application::operational_facts::target_resolver::RequestBodyIdentity::from_bytes(b"{}"),
                protocol_profile: crate::application::operational_facts::target_resolver::TargetProtocolProfile {
                    upstream_api_format: UpstreamApiFormat::Auto,
                    stream: false,
                    uses_tools: false,
                    uses_vision: false,
                    uses_reasoning: false,
                },
            },
            lease,
            _retry_permit: None,
        }
    }

    fn test_limits() -> ProxyServerLimits {
        ProxyServerLimits::default()
    }

    fn short_limits() -> ProxyServerLimits {
        ProxyServerLimits {
            upstream_connect_timeout: Duration::from_millis(50),
            ..ProxyServerLimits::default()
        }
    }

    impl UpstreamAttempt {
        fn status(&self) -> StatusCode {
            match self {
                Self::Buffered { status, .. } | Self::Stream { status, .. } => *status,
            }
        }
    }

    struct RawLoopback {
        base_url: String,
        worker: Option<std::thread::JoinHandle<()>>,
    }

    impl RawLoopback {
        fn serve(status: u16, body: Vec<u8>) -> Self {
            Self::serve_with_headers(status, body, &[])
        }

        fn serve_with_headers(status: u16, body: Vec<u8>, extra_headers: &[&str]) -> Self {
            use std::io::{Read, Write};
            use std::net::TcpListener;

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind raw loopback");
            let address = listener.local_addr().expect("raw loopback address");
            let extra_headers = extra_headers.join("\r\n");
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept raw loopback request");
                let mut request = [0_u8; 4096];
                let _ = stream.read(&mut request);
                write!(
                    stream,
                    "HTTP/1.1 {status} fixture\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n{extra_headers}\r\nConnection: close\r\n\r\n",
                    body.len(),
                )
                .expect("write raw loopback headers");
                stream.write_all(&body).expect("write raw loopback body");
            });
            Self {
                base_url: format!("http://{address}"),
                worker: Some(worker),
            }
        }

        fn disconnect_after_request() -> Self {
            use std::io::Read;
            use std::net::TcpListener;

            let listener = TcpListener::bind("127.0.0.1:0").expect("bind raw loopback");
            let address = listener.local_addr().expect("raw loopback address");
            let worker = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept raw loopback request");
                let mut request = [0_u8; 4096];
                let read = stream
                    .read(&mut request)
                    .expect("read request before close");
                assert!(read > 0, "fixture must observe upstream request bytes");
                // Drop the accepted socket without a response. The client cannot infer
                // whether headers or body bytes had reached the peer from this error.
            });
            Self {
                base_url: format!("http://{address}"),
                worker: Some(worker),
            }
        }
    }

    impl Drop for RawLoopback {
        fn drop(&mut self) {
            if let Some(worker) = self.worker.take() {
                worker.join().expect("join raw loopback");
            }
        }
    }
}
