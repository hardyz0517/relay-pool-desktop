use std::{
    collections::{HashMap, HashSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};

use bytes::Bytes;
use http::{header, HeaderMap, Method, StatusCode};
use reqwest::Url;
use tokio_util::sync::CancellationToken;

use crate::observability::correlation;
use crate::outbound::{
    error::{OutboundFailure, OutboundFailureKind},
    policy::{OutboundHeaderPolicy, OutboundHeaders, RequestBudget, TimeoutPolicy},
    proxy::{ManualProxy, ProxyPolicy, TransportPoolKey},
};

#[derive(Clone, Debug)]
pub struct AsyncOutboundClientConfig {
    pub timeouts: TimeoutPolicy,
    pub header_policy: OutboundHeaderPolicy,
    pub success_body_max_bytes: usize,
    pub error_body_max_bytes: usize,
    pub max_attempts: usize,
    pub redirect_max_hops: usize,
    pub https_downgrade_allowed: bool,
}

impl AsyncOutboundClientConfig {
    pub fn architecture_budget() -> Self {
        Self {
            timeouts: TimeoutPolicy::provider_default(),
            header_policy: OutboundHeaderPolicy::provider_default(),
            success_body_max_bytes: 8_388_608,
            error_body_max_bytes: 65_536,
            max_attempts: 2,
            redirect_max_hops: 5,
            https_downgrade_allowed: false,
        }
    }
}

#[derive(Clone)]
pub struct AsyncOutboundClient {
    clients: Arc<Mutex<HashMap<TransportPoolKey, reqwest::Client>>>,
    created_clients: Arc<AtomicUsize>,
    config: AsyncOutboundClientConfig,
}

impl AsyncOutboundClient {
    pub fn new(config: AsyncOutboundClientConfig) -> Self {
        assert!(config.max_attempts > 0, "max_attempts must be positive");
        Self {
            clients: Arc::new(Mutex::new(HashMap::new())),
            created_clients: Arc::new(AtomicUsize::new(0)),
            config,
        }
    }

    pub async fn execute(
        &self,
        request: OutboundRequest,
        cancellation_token: CancellationToken,
    ) -> Result<OutboundResponse, OutboundFailure> {
        let url = validate_url(&request.url)?;
        let redacted_start_url = redact_url(&url);
        let mut attempts = 0_usize;
        loop {
            let Some(remaining) = request.budget.remaining() else {
                return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                    .with_url(redacted_start_url.clone()));
            };
            let Some(total_timeout) = min_non_zero(remaining, self.config.timeouts.total_timeout)
            else {
                return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                    .with_url(redacted_start_url.clone()));
            };
            let result = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                        .with_url(redacted_start_url.clone()));
                }
                result = tokio::time::timeout(
                    total_timeout,
                    self.execute_once(&request, &url, &redacted_start_url, cancellation_token.clone()),
                ) => {
                    match result {
                        Ok(result) => result,
                        Err(_) => Err(OutboundFailure::new(OutboundFailureKind::TotalTimeout)
                            .with_url(redacted_start_url.clone())),
                    }
                }
            };
            match result {
                Ok(response)
                    if should_retry(response.status) && attempts + 1 < self.config.max_attempts =>
                {
                    attempts += 1;
                    let Some(retry_after) = response.evidence.retry_after else {
                        return Ok(response);
                    };
                    let Some(remaining) = request.budget.remaining() else {
                        return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                            .with_url(redacted_start_url.clone()));
                    };
                    if retry_after > remaining {
                        return Err(OutboundFailure::new(
                            OutboundFailureKind::RetryAfterExceedsBudget,
                        )
                        .with_url(redacted_start_url.clone())
                        .with_retry_after(Some(retry_after)));
                    }
                    tokio::select! {
                        _ = cancellation_token.cancelled() => {
                            return Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                                .with_url(redacted_start_url.clone()));
                        }
                        _ = tokio::time::sleep(retry_after) => {}
                    }
                }
                other => return other,
            }
        }
    }

    pub async fn execute_stream<H>(
        &self,
        request: OutboundRequest,
        cancellation_token: CancellationToken,
        mut on_chunk: H,
    ) -> Result<OutboundStreamResponse, OutboundFailure>
    where
        H: FnMut(&[u8]) -> Result<(), OutboundFailure> + Send,
    {
        let url = validate_url(&request.url)?;
        let redacted_start_url = redact_url(&url);
        let Some(remaining) = request.budget.remaining() else {
            return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                .with_url(redacted_start_url.clone()));
        };
        let Some(total_timeout) = min_non_zero(remaining, self.config.timeouts.total_timeout)
        else {
            return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                .with_url(redacted_start_url.clone()));
        };
        let cancellation_wait = cancellation_token.clone();
        tokio::select! {
            _ = cancellation_wait.cancelled() => {
                Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                    .with_url(redacted_start_url))
            }
            result = tokio::time::timeout(
                total_timeout,
                self.execute_once_stream(&request, &url, &redacted_start_url, cancellation_token, &mut on_chunk),
            ) => {
                match result {
                    Ok(result) => result,
                    Err(_) => Err(OutboundFailure::new(OutboundFailureKind::TotalTimeout)
                        .with_url(redacted_start_url)),
                }
            }
        }
    }

    pub fn metrics(&self) -> OutboundClientMetrics {
        OutboundClientMetrics {
            pool_size: self.clients.lock().expect("outbound client pool").len(),
            client_instances_created: self.created_clients.load(Ordering::SeqCst),
        }
    }

    fn client_for_policy(&self, policy: &ProxyPolicy) -> Result<reqwest::Client, OutboundFailure> {
        let key = policy.pool_key();
        let mut clients = self.clients.lock().expect("outbound client pool");
        if let Some(client) = clients.get(&key) {
            return Ok(client.clone());
        }

        let mut builder = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(self.config.timeouts.connect_timeout)
            .read_timeout(self.config.timeouts.body_read_timeout)
            .timeout(self.config.timeouts.total_timeout);

        match policy {
            ProxyPolicy::Direct => {
                builder = builder.no_proxy();
            }
            ProxyPolicy::System => {}
            ProxyPolicy::Manual(proxy) => {
                builder = builder.no_proxy().proxy(build_proxy(proxy)?);
            }
        }

        let client = builder
            .build()
            .map_err(|_| OutboundFailure::new(OutboundFailureKind::TransportPolicy))?;
        clients.insert(key, client.clone());
        self.created_clients.fetch_add(1, Ordering::SeqCst);
        Ok(client)
    }

    async fn execute_once(
        &self,
        request: &OutboundRequest,
        start_url: &Url,
        redacted_start_url: &str,
        cancellation_token: CancellationToken,
    ) -> Result<OutboundResponse, OutboundFailure> {
        let client = self.client_for_policy(&request.proxy)?;
        let mut current_url = start_url.clone();
        let mut redirect_chain = Vec::new();
        let mut visited = HashSet::new();
        let body = Bytes::copy_from_slice(&request.body);

        for hop in 0..=self.config.redirect_max_hops {
            let Some(remaining) = request.budget.remaining() else {
                return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                    .with_url(redacted_start_url.to_string()));
            };
            let timeout = min_non_zero(remaining, self.config.timeouts.first_byte_timeout)
                .ok_or_else(|| OutboundFailure::new(OutboundFailureKind::BudgetExhausted))?;
            let preserve_sensitive = same_origin(start_url, &current_url);
            let headers = if hop == 0 {
                request.headers.materialize(&self.config.header_policy)?
            } else {
                request
                    .headers
                    .materialize_for_redirect(&self.config.header_policy, preserve_sensitive)?
            };
            let reqwest_method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                .map_err(|_| OutboundFailure::new(OutboundFailureKind::TransportPolicy))?;
            let builder = client
                .request(reqwest_method, current_url.clone())
                .headers(headers)
                .body(body.clone())
                .timeout(remaining);
            let response = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                        .with_url(redacted_start_url.to_string()));
                }
                result = tokio::time::timeout(timeout, builder.send()) => {
                    match result {
                        Ok(Ok(response)) => response,
                        Ok(Err(error)) if error.is_connect() || error.is_timeout() => {
                            return Err(OutboundFailure::new(OutboundFailureKind::ConnectTimeout)
                                .with_url(redacted_start_url.to_string()));
                        }
                        Ok(Err(_)) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::RequestFailed)
                                .with_url(redacted_start_url.to_string()));
                        }
                        Err(_) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::FirstByteTimeout)
                                .with_url(redacted_start_url.to_string()));
                        }
                    }
                }
            };

            let status = response.status();
            if is_redirect(status) {
                let Some(next_url) = redirect_url(&current_url, response.headers())? else {
                    return Err(OutboundFailure::new(OutboundFailureKind::RedirectBlocked)
                        .with_url(redacted_start_url.to_string()));
                };
                validate_redirect(&current_url, &next_url, self.config.https_downgrade_allowed)?;
                let redacted_next = redact_url(&next_url);
                if !visited.insert(redacted_next.clone()) {
                    return Err(OutboundFailure::new(OutboundFailureKind::RedirectLoop)
                        .with_url(redacted_start_url.to_string()));
                }
                redirect_chain.push(redacted_next);
                current_url = next_url;
                continue;
            }

            let retry_after = retry_after(response.headers());
            let headers = response.headers().clone();
            let body_limit = if status.is_success() {
                self.config.success_body_max_bytes
            } else {
                self.config.error_body_max_bytes
            };
            let body = self
                .read_limited_body(
                    response,
                    body_limit,
                    redacted_start_url,
                    cancellation_token.clone(),
                )
                .await?;
            return Ok(OutboundResponse {
                status,
                headers,
                body: body.clone(),
                evidence: OutboundEvidence {
                    start_url: redacted_start_url.to_string(),
                    final_url: redact_url(&current_url),
                    redirect_chain,
                    retry_after,
                    body_redaction: redact_body_preview(body.len()),
                    header_redaction: request.headers.redaction(),
                },
            });
        }

        Err(
            OutboundFailure::new(OutboundFailureKind::RedirectLimitExceeded)
                .with_url(redacted_start_url.to_string()),
        )
    }

    async fn execute_once_stream<H>(
        &self,
        request: &OutboundRequest,
        start_url: &Url,
        redacted_start_url: &str,
        cancellation_token: CancellationToken,
        on_chunk: &mut H,
    ) -> Result<OutboundStreamResponse, OutboundFailure>
    where
        H: FnMut(&[u8]) -> Result<(), OutboundFailure> + Send,
    {
        let client = self.client_for_policy(&request.proxy)?;
        let mut current_url = start_url.clone();
        let mut redirect_chain = Vec::new();
        let mut visited = HashSet::new();
        let body = Bytes::copy_from_slice(&request.body);

        for hop in 0..=self.config.redirect_max_hops {
            let Some(remaining) = request.budget.remaining() else {
                return Err(OutboundFailure::new(OutboundFailureKind::BudgetExhausted)
                    .with_url(redacted_start_url.to_string()));
            };
            let timeout = min_non_zero(remaining, self.config.timeouts.first_byte_timeout)
                .ok_or_else(|| OutboundFailure::new(OutboundFailureKind::BudgetExhausted))?;
            let preserve_sensitive = same_origin(start_url, &current_url);
            let headers = if hop == 0 {
                request.headers.materialize(&self.config.header_policy)?
            } else {
                request
                    .headers
                    .materialize_for_redirect(&self.config.header_policy, preserve_sensitive)?
            };
            let reqwest_method = reqwest::Method::from_bytes(request.method.as_str().as_bytes())
                .map_err(|_| OutboundFailure::new(OutboundFailureKind::TransportPolicy))?;
            let builder = client
                .request(reqwest_method, current_url.clone())
                .headers(headers)
                .body(body.clone())
                .timeout(remaining);
            let response = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                        .with_url(redacted_start_url.to_string()));
                }
                result = tokio::time::timeout(timeout, builder.send()) => {
                    match result {
                        Ok(Ok(response)) => response,
                        Ok(Err(error)) if error.is_connect() || error.is_timeout() => {
                            return Err(OutboundFailure::new(OutboundFailureKind::ConnectTimeout)
                                .with_url(redacted_start_url.to_string()));
                        }
                        Ok(Err(_)) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::RequestFailed)
                                .with_url(redacted_start_url.to_string()));
                        }
                        Err(_) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::FirstByteTimeout)
                                .with_url(redacted_start_url.to_string()));
                        }
                    }
                }
            };

            let status = response.status();
            if is_redirect(status) {
                let Some(next_url) = redirect_url(&current_url, response.headers())? else {
                    return Err(OutboundFailure::new(OutboundFailureKind::RedirectBlocked)
                        .with_url(redacted_start_url.to_string()));
                };
                validate_redirect(&current_url, &next_url, self.config.https_downgrade_allowed)?;
                let redacted_next = redact_url(&next_url);
                if !visited.insert(redacted_next.clone()) {
                    return Err(OutboundFailure::new(OutboundFailureKind::RedirectLoop)
                        .with_url(redacted_start_url.to_string()));
                }
                redirect_chain.push(redacted_next);
                current_url = next_url;
                continue;
            }

            let retry_after = retry_after(response.headers());
            let headers = response.headers().clone();
            let body_limit = if status.is_success() {
                self.config.success_body_max_bytes
            } else {
                self.config.error_body_max_bytes
            };
            let body_bytes = self
                .read_stream_body(
                    response,
                    body_limit,
                    redacted_start_url,
                    cancellation_token.clone(),
                    on_chunk,
                )
                .await?;
            return Ok(OutboundStreamResponse {
                status,
                headers,
                body_bytes,
                evidence: OutboundEvidence {
                    start_url: redacted_start_url.to_string(),
                    final_url: redact_url(&current_url),
                    redirect_chain,
                    retry_after,
                    body_redaction: redact_body_preview(body_bytes),
                    header_redaction: request.headers.redaction(),
                },
            });
        }

        Err(
            OutboundFailure::new(OutboundFailureKind::RedirectLimitExceeded)
                .with_url(redacted_start_url.to_string()),
        )
    }

    async fn read_limited_body(
        &self,
        mut response: reqwest::Response,
        body_limit: usize,
        redacted_url: &str,
        cancellation_token: CancellationToken,
    ) -> Result<Bytes, OutboundFailure> {
        let mut body = Vec::new();
        loop {
            let Some(remaining) = body_read_remaining(self.config.timeouts.body_read_timeout)
            else {
                return Err(OutboundFailure::new(OutboundFailureKind::BodyTimeout)
                    .with_url(redacted_url.to_string()));
            };
            let chunk = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                        .with_url(redacted_url.to_string()));
                }
                result = tokio::time::timeout(remaining, response.chunk()) => {
                    match result {
                        Ok(Ok(chunk)) => chunk,
                        Ok(Err(error)) if error.is_timeout() => {
                            return Err(OutboundFailure::new(OutboundFailureKind::BodyTimeout)
                                .with_url(redacted_url.to_string()));
                        }
                        Ok(Err(_)) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::RequestFailed)
                                .with_url(redacted_url.to_string()));
                        }
                        Err(_) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::BodyTimeout)
                                .with_url(redacted_url.to_string()));
                        }
                    }
                }
            };
            let Some(chunk) = chunk else {
                return Ok(Bytes::from(body));
            };
            if body.len() + chunk.len() > body_limit {
                return Err(
                    OutboundFailure::new(OutboundFailureKind::BodyLimitExceeded {
                        limit_bytes: body_limit,
                    })
                    .with_url(redacted_url.to_string()),
                );
            }
            body.extend_from_slice(&chunk);
        }
    }

    async fn read_stream_body<H>(
        &self,
        mut response: reqwest::Response,
        body_limit: usize,
        redacted_url: &str,
        cancellation_token: CancellationToken,
        on_chunk: &mut H,
    ) -> Result<usize, OutboundFailure>
    where
        H: FnMut(&[u8]) -> Result<(), OutboundFailure> + Send,
    {
        let mut body_bytes = 0_usize;
        loop {
            let Some(remaining) = body_read_remaining(self.config.timeouts.body_read_timeout)
            else {
                return Err(OutboundFailure::new(OutboundFailureKind::BodyTimeout)
                    .with_url(redacted_url.to_string()));
            };
            let chunk = tokio::select! {
                _ = cancellation_token.cancelled() => {
                    return Err(OutboundFailure::new(OutboundFailureKind::Cancelled)
                        .with_url(redacted_url.to_string()));
                }
                result = tokio::time::timeout(remaining, response.chunk()) => {
                    match result {
                        Ok(Ok(chunk)) => chunk,
                        Ok(Err(error)) if error.is_timeout() => {
                            return Err(OutboundFailure::new(OutboundFailureKind::BodyTimeout)
                                .with_url(redacted_url.to_string()));
                        }
                        Ok(Err(_)) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::RequestFailed)
                                .with_url(redacted_url.to_string()));
                        }
                        Err(_) => {
                            return Err(OutboundFailure::new(OutboundFailureKind::BodyTimeout)
                                .with_url(redacted_url.to_string()));
                        }
                    }
                }
            };
            let Some(chunk) = chunk else {
                return Ok(body_bytes);
            };
            if body_bytes + chunk.len() > body_limit {
                return Err(
                    OutboundFailure::new(OutboundFailureKind::BodyLimitExceeded {
                        limit_bytes: body_limit,
                    })
                    .with_url(redacted_url.to_string()),
                );
            }
            on_chunk(&chunk)?;
            body_bytes += chunk.len();
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct OutboundClientMetrics {
    pub pool_size: usize,
    pub client_instances_created: usize,
}

pub struct OutboundRequest {
    pub method: Method,
    pub url: String,
    pub correlation_id: Option<String>,
    pub headers: OutboundHeaders,
    pub body: Vec<u8>,
    pub proxy: ProxyPolicy,
    pub budget: RequestBudget,
}

impl OutboundRequest {
    pub fn get(url: impl Into<String>, budget: RequestBudget) -> Self {
        Self {
            method: Method::GET,
            url: url.into(),
            correlation_id: correlation::current_id_string(),
            headers: OutboundHeaders::new(),
            body: Vec::new(),
            proxy: ProxyPolicy::Direct,
            budget,
        }
    }
}

#[derive(Debug)]
pub struct OutboundResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: Bytes,
    pub evidence: OutboundEvidence,
}

#[derive(Debug)]
pub struct OutboundStreamResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body_bytes: usize,
    pub evidence: OutboundEvidence,
}

#[derive(Debug, PartialEq, Eq)]
pub struct OutboundEvidence {
    pub start_url: String,
    pub final_url: String,
    pub redirect_chain: Vec<String>,
    pub retry_after: Option<Duration>,
    pub body_redaction: String,
    pub header_redaction: crate::outbound::policy::HeaderRedaction,
}

fn validate_url(input: &str) -> Result<Url, OutboundFailure> {
    if input.chars().any(char::is_control) {
        return Err(OutboundFailure::new(OutboundFailureKind::InvalidUrl));
    }
    let url =
        Url::parse(input).map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidUrl))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(OutboundFailure::new(OutboundFailureKind::InvalidUrl));
    }
    Ok(url)
}

fn redirect_url(current_url: &Url, headers: &HeaderMap) -> Result<Option<Url>, OutboundFailure> {
    let Some(location) = headers.get(header::LOCATION) else {
        return Ok(None);
    };
    let location = location
        .to_str()
        .map_err(|_| OutboundFailure::new(OutboundFailureKind::RedirectBlocked))?;
    if location.chars().any(char::is_control) {
        return Err(OutboundFailure::new(OutboundFailureKind::RedirectBlocked));
    }
    let next = current_url
        .join(location)
        .map_err(|_| OutboundFailure::new(OutboundFailureKind::RedirectBlocked))?;
    if !matches!(next.scheme(), "http" | "https")
        || !next.username().is_empty()
        || next.password().is_some()
    {
        return Err(OutboundFailure::new(OutboundFailureKind::RedirectBlocked));
    }
    Ok(Some(next))
}

fn validate_redirect(
    from: &Url,
    to: &Url,
    https_downgrade_allowed: bool,
) -> Result<(), OutboundFailure> {
    if from.scheme() == "https" && to.scheme() == "http" && !https_downgrade_allowed {
        return Err(OutboundFailure::new(OutboundFailureKind::RedirectBlocked));
    }
    Ok(())
}

fn build_proxy(proxy: &ManualProxy) -> Result<reqwest::Proxy, OutboundFailure> {
    let mut reqwest_proxy = reqwest::Proxy::all(&proxy.endpoint)
        .map_err(|_| OutboundFailure::new(OutboundFailureKind::ProxyPolicy))?;
    if let Some(credentials) = &proxy.credentials {
        reqwest_proxy =
            reqwest_proxy.basic_auth(&credentials.username, credentials.password.expose());
    }
    Ok(reqwest_proxy)
}

fn is_redirect(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::MOVED_PERMANENTLY
            | StatusCode::FOUND
            | StatusCode::SEE_OTHER
            | StatusCode::TEMPORARY_REDIRECT
            | StatusCode::PERMANENT_REDIRECT
    )
}

fn should_retry(status: StatusCode) -> bool {
    status == StatusCode::TOO_MANY_REQUESTS || status == StatusCode::SERVICE_UNAVAILABLE
}

fn retry_after(headers: &HeaderMap) -> Option<Duration> {
    let value = headers.get(header::RETRY_AFTER)?.to_str().ok()?.trim();
    let seconds = value.parse::<u64>().ok()?;
    Some(Duration::from_secs(seconds))
}

fn same_origin(left: &Url, right: &Url) -> bool {
    left.scheme() == right.scheme()
        && left.host_str() == right.host_str()
        && left.port_or_known_default() == right.port_or_known_default()
}

fn min_non_zero(left: Duration, right: Duration) -> Option<Duration> {
    let result = left.min(right);
    if result.is_zero() {
        None
    } else {
        Some(result)
    }
}

fn body_read_remaining(timeout: Duration) -> Option<Duration> {
    if timeout.is_zero() {
        None
    } else {
        Some(timeout)
    }
}

fn redact_url(url: &Url) -> String {
    let mut redacted = url.clone();
    redacted.set_query(None);
    redacted.set_fragment(None);
    redacted.to_string()
}

fn redact_body_preview(len: usize) -> String {
    format!("<body redacted: {len} bytes>")
}
