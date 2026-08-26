use std::time::{Duration, Instant};

use http::{HeaderName, HeaderValue, Method};
use tokio_util::sync::CancellationToken;

use crate::{
    models::monitoring::FailureKind,
    outbound::{
        AsyncOutboundClient, OutboundFailure, OutboundFailureKind, OutboundHeaderPolicy,
        OutboundHeaders, OutboundRequest, OutboundRetryPolicy, ProxyPolicy, RequestBudget,
        SecretHeaderValue,
    },
    services::monitoring::adapters::contract::RequestDescriptor,
};

#[cfg(test)]
use crate::outbound::{AsyncOutboundClientConfig, TimeoutPolicy};

#[derive(Clone)]
pub struct MonitoringTransport {
    client: AsyncOutboundClient,
    config: MonitoringTransportConfig,
}

#[derive(Clone, Debug)]
pub struct MonitoringTransportConfig {
    pub base_url: String,
    pub proxy: ProxyPolicy,
    #[cfg(test)]
    pub timeouts: TimeoutPolicy,
    #[cfg(test)]
    pub success_body_max_bytes: usize,
    #[cfg(test)]
    pub error_body_max_bytes: usize,
    #[cfg(test)]
    pub redirect_max_hops: usize,
}

impl MonitoringTransportConfig {
    #[cfg(test)]
    pub fn loopback_test(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            proxy: ProxyPolicy::Direct,
            timeouts: TimeoutPolicy {
                connect_timeout: Duration::from_millis(250),
                first_byte_timeout: Duration::from_millis(500),
                body_read_timeout: Duration::from_millis(500),
                total_timeout: Duration::from_secs(2),
            },
            // Streaming probe consumers enforce their own, protocol-specific
            // total limit. Keep the loopback transport high enough that it
            // does not reintroduce the historical 64 KiB false failure.
            success_body_max_bytes: 2 * 1024 * 1024,
            error_body_max_bytes: 8 * 1024,
            redirect_max_hops: 2,
        }
    }
}

#[derive(Debug)]
pub struct MonitoringTransportRequest {
    pub descriptor: RequestDescriptor,
    pub public_headers: Vec<(String, String)>,
    pub auth_header: Option<MonitoringAuthHeader>,
    pub request_deadline: Instant,
}

#[derive(Debug)]
pub struct MonitoringAuthHeader {
    pub name: String,
    pub value: SecretHeaderValue,
}

#[derive(Debug)]
pub struct MonitoringTransportResponse {
    pub http_status: u16,
    pub content_type: Option<String>,
    pub body: Vec<u8>,
    pub first_headers_latency_ms: u64,
    pub first_content_latency_ms: Option<u64>,
    pub total_latency_ms: u64,
    #[cfg(test)]
    pub response_bytes: usize,
    #[cfg(test)]
    pub evidence: MonitoringRequestEvidence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub struct MonitoringRequestEvidence {
    pub method: String,
    pub relative_path: String,
    pub final_url: String,
    pub redirect_chain_len: usize,
    pub header_names: Vec<String>,
    pub body_redaction: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MonitoringTransportError {
    pub kind: MonitoringTransportFailureKind,
    pub failure_kind: FailureKind,
    pub retry_after_ms: Option<u64>,
    pub redacted_url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MonitoringTransportFailureKind {
    InvalidUrl,
    InvalidHeader,
    HeaderRejected,
    ProxyPolicy,
    TransportPolicy,
    DnsOrConnectOrTls,
    FirstHeaderTimeout,
    BodyTimeout,
    TotalTimeout,
    BudgetExceeded,
    Cancelled,
    BodyLimitExceeded,
    Redirect,
    RetryAfterExceedsBudget,
    RequestFailed,
}

impl MonitoringTransport {
    #[cfg(test)]
    pub fn new(config: MonitoringTransportConfig) -> Self {
        let client = AsyncOutboundClient::new(AsyncOutboundClientConfig {
            timeouts: config.timeouts.clone(),
            header_policy: OutboundHeaderPolicy::provider_default(),
            success_body_max_bytes: config.success_body_max_bytes,
            error_body_max_bytes: config.error_body_max_bytes,
            max_attempts: 1,
            redirect_max_hops: config.redirect_max_hops,
            https_downgrade_allowed: false,
        });
        Self { client, config }
    }

    pub fn from_client(client: AsyncOutboundClient, config: MonitoringTransportConfig) -> Self {
        Self { client, config }
    }

    #[cfg(test)]
    pub fn client_metrics(&self) -> crate::outbound::OutboundClientMetrics {
        self.client.metrics()
    }

    pub async fn execute_buffered(
        &self,
        request: MonitoringTransportRequest,
        cancellation_token: CancellationToken,
    ) -> Result<MonitoringTransportResponse, MonitoringTransportError> {
        let started = Instant::now();
        let response = match self.outbound_request(&request) {
            Ok(outbound_request) => self
                .client
                .execute(outbound_request, cancellation_token)
                .await
                .map_err(map_outbound_failure),
            Err(error) => Err(error),
        };
        let response = match response {
            Ok(response) => response,
            Err(error) => return Err(self.record_failure(error)),
        };
        let total_latency_ms = elapsed_ms(started.elapsed());
        let content_type = response
            .headers
            .get(http::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_string);
        #[cfg(test)]
        let response_bytes = response.body.len();
        Ok(MonitoringTransportResponse {
            http_status: response.status.as_u16(),
            content_type,
            body: response.body.to_vec(),
            first_headers_latency_ms: total_latency_ms,
            first_content_latency_ms: Some(total_latency_ms),
            total_latency_ms,
            #[cfg(test)]
            response_bytes,
            #[cfg(test)]
            evidence: MonitoringRequestEvidence {
                method: request.descriptor.method,
                relative_path: request.descriptor.path,
                final_url: response.evidence.final_url,
                redirect_chain_len: response.evidence.redirect_chain.len(),
                header_names: request
                    .public_headers
                    .iter()
                    .map(|(name, _)| name.to_ascii_lowercase())
                    .chain(
                        request
                            .auth_header
                            .as_ref()
                            .map(|header| header.name.to_ascii_lowercase()),
                    )
                    .collect(),
                body_redaction: response.evidence.body_redaction,
            },
        })
    }

    /// Streams each received network chunk directly to `on_chunk`.
    ///
    /// Monitoring callers must reduce SSE events in this callback instead of
    /// reconstructing a successful response body in memory.
    pub async fn execute_streaming<H>(
        &self,
        request: MonitoringTransportRequest,
        cancellation_token: CancellationToken,
        mut on_chunk: H,
    ) -> Result<MonitoringTransportResponse, MonitoringTransportError>
    where
        H: FnMut(&[u8]) + Send,
    {
        let started = Instant::now();
        let mut first_content_latency_ms = None;
        let stream_response = match self.outbound_request(&request) {
            Ok(outbound_request) => self
                .client
                .execute_stream(outbound_request, cancellation_token, |chunk| {
                    if first_content_latency_ms.is_none() {
                        first_content_latency_ms = Some(elapsed_ms(started.elapsed()));
                    }
                    on_chunk(chunk);
                    Ok(())
                })
                .await
                .map_err(map_outbound_failure),
            Err(error) => Err(error),
        };
        let stream_response = match stream_response {
            Ok(response) => response,
            Err(error) => return Err(self.record_failure(error)),
        };
        let total_latency_ms = elapsed_ms(started.elapsed());
        Ok(MonitoringTransportResponse {
            http_status: stream_response.status.as_u16(),
            content_type: stream_response
                .headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok())
                .map(str::to_string),
            // A streaming success is deliberately not retained. The protocol
            // reducer receives each chunk above and owns only bounded state.
            body: Vec::new(),
            first_headers_latency_ms: stream_response.headers_latency_ms,
            first_content_latency_ms,
            total_latency_ms,
            #[cfg(test)]
            response_bytes: stream_response.body_bytes,
            #[cfg(test)]
            evidence: MonitoringRequestEvidence {
                method: request.descriptor.method,
                relative_path: request.descriptor.path,
                final_url: stream_response.evidence.final_url,
                redirect_chain_len: stream_response.evidence.redirect_chain.len(),
                header_names: request
                    .public_headers
                    .iter()
                    .map(|(name, _)| name.to_ascii_lowercase())
                    .chain(
                        request
                            .auth_header
                            .as_ref()
                            .map(|header| header.name.to_ascii_lowercase()),
                    )
                    .collect(),
                body_redaction: stream_response.evidence.body_redaction,
            },
        })
    }

    fn outbound_request(
        &self,
        request: &MonitoringTransportRequest,
    ) -> Result<OutboundRequest, MonitoringTransportError> {
        let policy = OutboundHeaderPolicy::provider_default();
        let mut headers = OutboundHeaders::new();
        for (name, value) in &request.public_headers {
            headers
                .insert_public(
                    HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
                        MonitoringTransportError::from_kind(
                            MonitoringTransportFailureKind::InvalidHeader,
                        )
                    })?,
                    HeaderValue::from_str(value).map_err(|_| {
                        MonitoringTransportError::from_kind(
                            MonitoringTransportFailureKind::InvalidHeader,
                        )
                    })?,
                    &policy,
                )
                .map_err(map_outbound_failure)?;
        }
        if let Some(auth_header) = &request.auth_header {
            headers
                .insert_sensitive(
                    HeaderName::from_bytes(auth_header.name.as_bytes()).map_err(|_| {
                        MonitoringTransportError::from_kind(
                            MonitoringTransportFailureKind::InvalidHeader,
                        )
                    })?,
                    auth_header.value.clone(),
                    &policy,
                )
                .map_err(map_outbound_failure)?;
        }
        Ok(OutboundRequest {
            method: Method::from_bytes(request.descriptor.method.as_bytes()).map_err(|_| {
                MonitoringTransportError::from_kind(MonitoringTransportFailureKind::InvalidHeader)
            })?,
            url: join_url(&self.config.base_url, &request.descriptor.path)?,
            correlation_id: None,
            headers,
            body: request.descriptor.body.clone(),
            proxy: self.config.proxy.clone(),
            budget: RequestBudget::from_deadline(request.request_deadline),
            retry_policy: OutboundRetryPolicy::Never,
        })
    }

    fn record_failure(&self, error: MonitoringTransportError) -> MonitoringTransportError {
        emit_transport_failure_event();
        error
    }
}

#[cfg(any(not(test), feature = "runtime-logging-artifact"))]
fn emit_transport_failure_event() {
    crate::observability::runtime::bootstrap::emit_rate_limited(
        crate::services::monitoring::runtime_events::transport_failed(),
    );
}

#[cfg(all(test, not(feature = "runtime-logging-artifact")))]
fn emit_transport_failure_event() {}

impl MonitoringTransportError {
    fn from_kind(kind: MonitoringTransportFailureKind) -> Self {
        Self {
            failure_kind: failure_kind_for_transport(&kind),
            kind,
            retry_after_ms: None,
            redacted_url: None,
        }
    }
}

fn join_url(base_url: &str, relative_path: &str) -> Result<String, MonitoringTransportError> {
    let base = reqwest::Url::parse(base_url).map_err(|_| {
        MonitoringTransportError::from_kind(MonitoringTransportFailureKind::InvalidUrl)
    })?;
    if !relative_path.starts_with('/') || relative_path.chars().any(char::is_control) {
        return Err(MonitoringTransportError::from_kind(
            MonitoringTransportFailureKind::InvalidUrl,
        ));
    }
    let joined = base
        .join(relative_path.trim_start_matches('/'))
        .map_err(|_| {
            MonitoringTransportError::from_kind(MonitoringTransportFailureKind::InvalidUrl)
        })?;
    Ok(joined.to_string())
}

fn map_outbound_failure(error: OutboundFailure) -> MonitoringTransportError {
    let kind = match error.kind {
        OutboundFailureKind::InvalidUrl => MonitoringTransportFailureKind::InvalidUrl,
        OutboundFailureKind::InvalidHeader => MonitoringTransportFailureKind::InvalidHeader,
        OutboundFailureKind::HeaderNotAllowed(_) => MonitoringTransportFailureKind::HeaderRejected,
        OutboundFailureKind::ProxyPolicy => MonitoringTransportFailureKind::ProxyPolicy,
        OutboundFailureKind::TransportPolicy => MonitoringTransportFailureKind::TransportPolicy,
        OutboundFailureKind::ConnectTimeout => MonitoringTransportFailureKind::DnsOrConnectOrTls,
        OutboundFailureKind::FirstByteTimeout => MonitoringTransportFailureKind::FirstHeaderTimeout,
        OutboundFailureKind::BodyTimeout => MonitoringTransportFailureKind::BodyTimeout,
        OutboundFailureKind::TotalTimeout => MonitoringTransportFailureKind::TotalTimeout,
        OutboundFailureKind::BudgetExhausted => MonitoringTransportFailureKind::BudgetExceeded,
        OutboundFailureKind::Cancelled => MonitoringTransportFailureKind::Cancelled,
        OutboundFailureKind::BodyLimitExceeded { .. } => {
            MonitoringTransportFailureKind::BodyLimitExceeded
        }
        OutboundFailureKind::RedirectBlocked
        | OutboundFailureKind::RedirectLoop
        | OutboundFailureKind::RedirectLimitExceeded => MonitoringTransportFailureKind::Redirect,
        OutboundFailureKind::RetryAfterExceedsBudget => {
            MonitoringTransportFailureKind::RetryAfterExceedsBudget
        }
        OutboundFailureKind::RequestFailed => MonitoringTransportFailureKind::RequestFailed,
    };
    MonitoringTransportError {
        failure_kind: failure_kind_for_transport(&kind),
        kind,
        retry_after_ms: error.retry_after_ms,
        redacted_url: error.url,
    }
}

fn failure_kind_for_transport(kind: &MonitoringTransportFailureKind) -> FailureKind {
    match kind {
        MonitoringTransportFailureKind::InvalidUrl
        | MonitoringTransportFailureKind::InvalidHeader
        | MonitoringTransportFailureKind::HeaderRejected
        | MonitoringTransportFailureKind::ProxyPolicy
        | MonitoringTransportFailureKind::TransportPolicy
        | MonitoringTransportFailureKind::Redirect => FailureKind::InvalidRequest,
        MonitoringTransportFailureKind::FirstHeaderTimeout
        | MonitoringTransportFailureKind::BodyTimeout
        | MonitoringTransportFailureKind::TotalTimeout
        | MonitoringTransportFailureKind::BudgetExceeded
        | MonitoringTransportFailureKind::RetryAfterExceedsBudget => FailureKind::Timeout,
        MonitoringTransportFailureKind::Cancelled => FailureKind::Cancelled,
        MonitoringTransportFailureKind::BodyLimitExceeded => FailureKind::ProtocolMismatch,
        MonitoringTransportFailureKind::DnsOrConnectOrTls
        | MonitoringTransportFailureKind::RequestFailed => FailureKind::Network,
    }
}

fn elapsed_ms(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use std::{
        io::Read,
        net::TcpListener,
        sync::Arc,
        time::{Duration, Instant},
    };

    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use super::{
        MonitoringTransport, MonitoringTransportConfig, MonitoringTransportFailureKind,
        MonitoringTransportRequest, RequestDescriptor,
    };

    fn request() -> MonitoringTransportRequest {
        MonitoringTransportRequest {
            descriptor: RequestDescriptor {
                method: "GET".to_string(),
                path: "/v1/models".to_string(),
                body: Vec::new(),
                stream: false,
            },
            public_headers: Vec::new(),
            auth_header: None,
            request_deadline: Instant::now() + Duration::from_secs(2),
        }
    }

    #[cfg(feature = "runtime-logging-artifact")]
    #[tokio::test]
    async fn loopback_timeout_and_cancel_publish_monitoring_jsonl_without_payloads() {
        crate::observability::runtime::bootstrap::reset_rate_limit_for_tests();
        let timeout_listener = TcpListener::bind("127.0.0.1:0").expect("bind timeout fixture");
        let timeout_address = timeout_listener
            .local_addr()
            .expect("timeout fixture address");
        let timeout_worker = std::thread::spawn(move || {
            let (mut stream, _) = timeout_listener.accept().expect("accept timeout request");
            let mut request = [0_u8; 4096];
            assert!(stream.read(&mut request).expect("read timeout request") > 0);
            std::thread::sleep(Duration::from_millis(750));
        });

        let cancel_listener = TcpListener::bind("127.0.0.1:0").expect("bind cancel fixture");
        let cancel_address = cancel_listener
            .local_addr()
            .expect("cancel fixture address");
        let (accepted_sender, accepted) = oneshot::channel();
        let cancel_worker = std::thread::spawn(move || {
            let (mut stream, _) = cancel_listener.accept().expect("accept cancel request");
            let mut request = [0_u8; 4096];
            assert!(stream.read(&mut request).expect("read cancel request") > 0);
            let _ = accepted_sender.send(());
            std::thread::sleep(Duration::from_millis(750));
        });

        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(crate::observability::runtime::RuntimeLogService::open(
            root.path(),
        ));
        crate::observability::runtime::bootstrap::with_test_service(
            Arc::clone(&service),
            || async {
                let timeout_transport = MonitoringTransport::new(
                    MonitoringTransportConfig::loopback_test(format!("http://{timeout_address}")),
                );
                let timeout = timeout_transport
                    .execute_buffered(request(), CancellationToken::new())
                    .await
                    .expect_err("delayed loopback headers must time out");
                assert!(matches!(
                    timeout.kind,
                    MonitoringTransportFailureKind::FirstHeaderTimeout
                        | MonitoringTransportFailureKind::DnsOrConnectOrTls
                ));

                let cancel_transport = MonitoringTransport::new(
                    MonitoringTransportConfig::loopback_test(format!("http://{cancel_address}")),
                );
                let cancellation = CancellationToken::new();
                let running = tokio::spawn({
                    let transport = cancel_transport.clone();
                    let cancellation = cancellation.clone();
                    async move { transport.execute_buffered(request(), cancellation).await }
                });
                accepted.await.expect("loopback cancel request accepted");
                cancellation.cancel();
                let cancelled = running
                    .await
                    .expect("cancelled monitoring task joins")
                    .expect_err("cancelled loopback request must fail");
                assert_eq!(cancelled.kind, MonitoringTransportFailureKind::Cancelled);
            },
        )
        .await;
        timeout_worker.join().expect("timeout fixture joins");
        cancel_worker.join().expect("cancel fixture joins");
        service.flush();

        let page = crate::observability::runtime::RuntimeLogReader::new(root.path()).read_page(
            0,
            50,
            1024 * 1024,
        );
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        assert!(page.lines.iter().any(|line| {
            serde_json::from_slice::<crate::observability::runtime::RuntimeEvent>(line.as_bytes())
                .ok()
                .is_some_and(|event| event.event_code.as_str() == "monitoring.transport.failed")
        }));
    }
}
