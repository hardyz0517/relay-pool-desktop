use std::time::{Duration, Instant};

use http::{header, HeaderName, HeaderValue, Method, StatusCode};
use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::{
    observability::correlation,
    outbound::{
        AsyncOutboundClient, OutboundFailure, OutboundFailureKind, OutboundHeaderPolicy,
        OutboundHeaders, OutboundRequest, ProxyPolicy, RequestBudget, SecretHeaderValue,
    },
    services::{
        channel_monitors::{
            redaction::redact_monitor_text,
            templates::{normalize_monitor_method, RenderedMonitorRequest},
        },
        proxy::observability::{ObservedUsage, SseUsageObserver},
        station_endpoints::build_api_url,
    },
};

const MAX_RESPONSE_EXCERPT_BYTES: usize = 4096;

#[derive(Debug, Clone)]
pub struct MonitorProbeUsage {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
}

impl From<ObservedUsage> for MonitorProbeUsage {
    fn from(usage: ObservedUsage) -> Self {
        Self {
            prompt_tokens: usage.input_tokens,
            completion_tokens: usage.output_tokens,
            total_tokens: usage.total_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cache_read_tokens: usage.cache_read_tokens,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MonitorProbeResult {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub latency_ms: i64,
    pub first_token_ms: Option<i64>,
    pub error_summary: Option<String>,
    pub usage: Option<MonitorProbeUsage>,
}

pub async fn run_monitor_probe(
    outbound: &AsyncOutboundClient,
    base_url: &str,
    api_key: &str,
    request: &RenderedMonitorRequest,
    timeout_seconds: i64,
    cancellation_token: CancellationToken,
) -> MonitorProbeResult {
    let started_at = Instant::now();
    let Some(url) = build_probe_url(base_url, &request.path) else {
        return failed_result(
            started_at,
            None,
            "Invalid monitor request path; expected same-origin absolute path",
        );
    };
    let method = match normalize_monitor_method(&request.method).and_then(|method| {
        method
            .parse::<Method>()
            .map_err(|_| "Invalid monitor request method".to_string())
    }) {
        Ok(method) => method,
        Err(error) => return failed_result(started_at, None, &error),
    };
    if let Some((name, _)) = request
        .headers
        .iter()
        .find(|(name, _)| !is_valid_header_name(name))
    {
        return failed_result(
            started_at,
            None,
            &format!("Invalid monitor request header name: {name}"),
        );
    }
    if let Some((name, _)) = request
        .headers
        .iter()
        .find(|(_, value)| !is_valid_header_value(value))
    {
        return failed_result(
            started_at,
            None,
            &format!("Invalid monitor request header value for: {name}"),
        );
    }

    let accept_header = if request.stream {
        "text/event-stream"
    } else {
        "application/json"
    };
    let outbound_request = match build_outbound_probe_request(
        method,
        url,
        api_key,
        request,
        accept_header,
        probe_timeout(timeout_seconds),
    ) {
        Ok(request) => request,
        Err(error) => {
            return failed_result(started_at, None, &format!("Network probe failed: {error}"));
        }
    };

    if request.stream {
        streaming_response_result(started_at, outbound, outbound_request, cancellation_token).await
    } else {
        match outbound.execute(outbound_request, cancellation_token).await {
            Ok(response) => response_result(started_at, response.status, &response.body),
            Err(error) => {
                failed_result(started_at, None, &format!("Network probe failed: {error}"))
            }
        }
    }
}

fn build_outbound_probe_request(
    method: Method,
    url: String,
    api_key: &str,
    request: &RenderedMonitorRequest,
    accept_header: &'static str,
    timeout: Duration,
) -> Result<OutboundRequest, OutboundFailure> {
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers.insert_sensitive(
        header::AUTHORIZATION,
        SecretHeaderValue::new(format!("Bearer {api_key}")),
        &policy,
    )?;
    for (name, value) in &request.headers {
        if is_forbidden_header(name) {
            continue;
        }
        headers.insert_public(
            HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidHeader))?,
            HeaderValue::from_str(value)
                .map_err(|_| OutboundFailure::new(OutboundFailureKind::InvalidHeader))?,
            &policy,
        )?;
    }
    headers.insert_public(
        header::ACCEPT,
        HeaderValue::from_static(accept_header),
        &policy,
    )?;
    Ok(OutboundRequest {
        method,
        url,
        correlation_id: correlation::current_id_string(),
        headers,
        body: request.body.clone(),
        proxy: ProxyPolicy::Direct,
        budget: RequestBudget::from_now(timeout),
        retry_policy: Default::default(),
    })
}

fn build_probe_url(base_url: &str, path: &str) -> Option<String> {
    if path != path.trim()
        || path.chars().any(|ch| ch.is_whitespace() || ch.is_control())
        || !path.starts_with('/')
        || path.starts_with("//")
        || path.contains("://")
        || has_dot_segment(path)
    {
        return None;
    }

    build_api_url(base_url, path).ok()
}

fn has_dot_segment(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment == "." || segment == "..")
}

fn probe_timeout(timeout_seconds: i64) -> Duration {
    Duration::from_secs(timeout_seconds.max(1) as u64)
}

fn response_result(started_at: Instant, status: StatusCode, body: &[u8]) -> MonitorProbeResult {
    let status_code = status.as_u16();
    let response_json =
        serde_json::from_slice::<Value>(&body[..body.len().min(MAX_RESPONSE_EXCERPT_BYTES)]).ok();
    let ok = status_code < 400;
    let error_summary = if ok {
        None
    } else {
        Some(redact_monitor_text(&format!(
            "Upstream returned HTTP {status_code}"
        )))
    };

    MonitorProbeResult {
        ok,
        status_code: Some(status_code),
        latency_ms: elapsed_ms(started_at),
        first_token_ms: None,
        error_summary,
        usage: response_json.as_ref().and_then(parse_monitor_probe_usage),
    }
}

async fn streaming_response_result(
    started_at: Instant,
    outbound: &AsyncOutboundClient,
    request: OutboundRequest,
    cancellation_token: CancellationToken,
) -> MonitorProbeResult {
    let mut observer = SseUsageObserver::default();
    let mut first_token_ms = None;
    let response = outbound
        .execute_stream(request, cancellation_token, |chunk| {
            if first_token_ms.is_none() {
                first_token_ms = Some(elapsed_ms(started_at));
            }
            observer.push(chunk);
            Ok(())
        })
        .await;

    let response = match response {
        Ok(response) => response,
        Err(error) => {
            return failed_result(started_at, None, &format!("Network probe failed: {error}"));
        }
    };

    let status_code = response.status.as_u16();
    let ok = status_code < 400;
    let error_summary =
        (!ok).then(|| redact_monitor_text(&format!("Upstream returned HTTP {status_code}")));
    MonitorProbeResult {
        ok,
        status_code: Some(status_code),
        latency_ms: elapsed_ms(started_at),
        first_token_ms,
        error_summary,
        usage: observer.usage().cloned().map(MonitorProbeUsage::from),
    }
}

fn parse_monitor_probe_usage(value: &Value) -> Option<MonitorProbeUsage> {
    ObservedUsage::from_json(value).map(MonitorProbeUsage::from)
}

fn failed_result(
    started_at: Instant,
    status_code: Option<u16>,
    error_summary: &str,
) -> MonitorProbeResult {
    MonitorProbeResult {
        ok: false,
        status_code,
        latency_ms: elapsed_ms(started_at),
        first_token_ms: None,
        error_summary: Some(redact_monitor_text(error_summary)),
        usage: None,
    }
}

fn elapsed_ms(started_at: Instant) -> i64 {
    started_at.elapsed().as_millis().min(i64::MAX as u128) as i64
}

fn is_forbidden_header(name: &str) -> bool {
    matches!(
        name.trim().to_ascii_lowercase().as_str(),
        "authorization" | "cookie" | "set-cookie"
    )
}

fn is_valid_header_name(name: &str) -> bool {
    !name.is_empty() && name.chars().all(is_http_token_char)
}

fn is_http_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '!' | '#'
                | '$'
                | '%'
                | '&'
                | '\''
                | '*'
                | '+'
                | '-'
                | '.'
                | '^'
                | '_'
                | '`'
                | '|'
                | '~'
        )
}

fn is_valid_header_value(value: &str) -> bool {
    value.chars().all(|ch| ch == '\t' || !ch.is_ascii_control())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        thread,
        time::Duration,
    };

    #[tokio::test]
    async fn sends_probe_with_authorization_and_parses_success_response() {
        let (origin, received) = spawn_upstream(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 28\r\n\r\n{\"ok\":true,\"token\":\"secret\"}",
        );
        let mut headers = HashMap::new();
        headers.insert("x-request-id".to_string(), "monitor-probe-1".to_string());
        let request = RenderedMonitorRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers,
            body: br#"{"model":"gpt-test"}"#.to_vec(),
            stream: false,
            reasoning_effort: None,
        };
        let base_url = format!("{origin}/v1");

        let result = run_monitor_probe(
            &test_outbound_client(),
            &base_url,
            "sk-probe-key",
            &request,
            2,
            CancellationToken::new(),
        )
        .await;
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.error_summary, None);
        assert!(raw_request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        let raw_request_lower = raw_request.to_ascii_lowercase();
        assert!(raw_request_lower.contains("authorization: bearer sk-probe-key"));
        assert!(raw_request_lower.contains("x-request-id: monitor-probe-1"));
        assert!(raw_request.contains(r#"{"model":"gpt-test"}"#));
    }

    #[tokio::test]
    async fn sends_probe_with_complete_api_namespace_without_duplicate_v1() {
        let (origin, received) =
            spawn_upstream("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
        let request = RenderedMonitorRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: HashMap::new(),
            body: br#"{"model":"gpt-test"}"#.to_vec(),
            stream: false,
            reasoning_effort: None,
        };
        let base_url = format!("{origin}/api/v3");

        let result = run_monitor_probe(
            &test_outbound_client(),
            &base_url,
            "sk-probe-key",
            &request,
            2,
            CancellationToken::new(),
        )
        .await;
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert!(raw_request.starts_with("POST /api/v3/chat/completions HTTP/1.1"));
    }

    #[tokio::test]
    async fn streaming_probe_records_first_token_and_final_usage() {
        let (origin, received) = spawn_staged_upstream(&[
            "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"O\"}\n\n",
            "data: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":9,\"output_tokens\":4,\"input_tokens_details\":{\"cached_tokens\":3}}}}\n\n",
        ]);
        let request = RenderedMonitorRequest {
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            headers: HashMap::new(),
            body: br#"{"model":"gpt-test","stream":true}"#.to_vec(),
            stream: true,
            reasoning_effort: Some("minimal".to_string()),
        };
        let base_url = format!("{origin}/v1");

        let result = run_monitor_probe(
            &test_outbound_client(),
            &base_url,
            "sk-probe-key",
            &request,
            2,
            CancellationToken::new(),
        )
        .await;
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert!(raw_request
            .to_ascii_lowercase()
            .contains("accept: text/event-stream"));
        assert!(result.first_token_ms.is_some());
        let usage = result.usage.expect("stream usage");
        assert_eq!(usage.prompt_tokens, Some(9));
        assert_eq!(usage.completion_tokens, Some(4));
        assert_eq!(usage.total_tokens, Some(13));
        assert_eq!(usage.cache_read_tokens, Some(3));
    }

    #[tokio::test]
    async fn ignores_template_authorization_and_cookie_headers() {
        let (origin, received) =
            spawn_upstream("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "Bearer sk-template".to_string(),
        );
        headers.insert("Cookie".to_string(), "session=secret".to_string());
        headers.insert("x-request-id".to_string(), "safe".to_string());
        let request = RenderedMonitorRequest {
            method: "GET".to_string(),
            path: "/v1/models".to_string(),
            headers,
            body: Vec::new(),
            stream: false,
            reasoning_effort: None,
        };
        let base_url = format!("{origin}/v1");

        let result = run_monitor_probe(
            &test_outbound_client(),
            &base_url,
            "sk-real-key",
            &request,
            2,
            CancellationToken::new(),
        )
        .await;
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        let raw_request_lower = raw_request.to_ascii_lowercase();
        assert!(raw_request_lower.contains("authorization: bearer sk-real-key"));
        assert!(!raw_request.contains("sk-template"));
        assert!(!raw_request.contains("session=secret"));
        assert!(raw_request_lower.contains("x-request-id: safe"));
    }

    #[tokio::test]
    async fn rejects_path_that_would_override_host() {
        let request = RenderedMonitorRequest {
            method: "GET".to_string(),
            path: "https://evil.example/v1/models".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
            stream: false,
            reasoning_effort: None,
        };

        let result = run_monitor_probe(
            &test_outbound_client(),
            "http://127.0.0.1:9",
            "sk-real-key",
            &request,
            1,
            CancellationToken::new(),
        )
        .await;

        assert!(!result.ok);
        assert_eq!(result.status_code, None);
        assert!(result.error_summary.unwrap().contains("path"));
    }

    #[tokio::test]
    async fn rejects_paths_with_whitespace_or_dot_segments() {
        for path in [
            "/v1/models bad",
            "/../v1/models",
            "/v1/../models",
            "/./v1/models",
        ] {
            let request = RenderedMonitorRequest {
                method: "GET".to_string(),
                path: path.to_string(),
                headers: HashMap::new(),
                body: Vec::new(),
                stream: false,
                reasoning_effort: None,
            };

            let result = run_monitor_probe(
                &test_outbound_client(),
                "http://127.0.0.1:9",
                "sk-real-key",
                &request,
                1,
                CancellationToken::new(),
            )
            .await;

            assert!(!result.ok, "{path} should be rejected");
            assert_eq!(result.status_code, None);
        }
    }

    #[tokio::test]
    async fn rejects_invalid_or_unsupported_methods_at_probe_boundary() {
        for method in ["TRACE", "BAD METHOD", "POST\r\nX-Bad: yes"] {
            let request = RenderedMonitorRequest {
                method: method.to_string(),
                path: "/v1/models".to_string(),
                headers: HashMap::new(),
                body: Vec::new(),
                stream: false,
                reasoning_effort: None,
            };

            let result = run_monitor_probe(
                &test_outbound_client(),
                "http://127.0.0.1:9",
                "sk-real-key",
                &request,
                1,
                CancellationToken::new(),
            )
            .await;

            assert!(!result.ok, "{method} should be rejected");
            assert_eq!(result.status_code, None);
            assert!(result.error_summary.unwrap().contains("method"));
        }
    }

    #[tokio::test]
    async fn rejects_invalid_forwarded_headers_at_probe_boundary() {
        for (name, value) in [
            ("x-bad\r\nInjected", "safe"),
            ("x-safe", "ok\r\nX-Evil: yes"),
        ] {
            let mut headers = HashMap::new();
            headers.insert(name.to_string(), value.to_string());
            let request = RenderedMonitorRequest {
                method: "GET".to_string(),
                path: "/v1/models".to_string(),
                headers,
                body: Vec::new(),
                stream: false,
                reasoning_effort: None,
            };

            let result = run_monitor_probe(
                &test_outbound_client(),
                "http://127.0.0.1:9",
                "sk-real-key",
                &request,
                1,
                CancellationToken::new(),
            )
            .await;

            assert!(!result.ok, "{name:?}: {value:?} should be rejected");
            assert_eq!(result.status_code, None);
            assert!(result.error_summary.unwrap().contains("header"));
        }
    }

    #[tokio::test]
    async fn cancelled_probe_fails_closed_without_status() {
        let cancellation_token = CancellationToken::new();
        cancellation_token.cancel();
        let request = RenderedMonitorRequest {
            method: "GET".to_string(),
            path: "/v1/models".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
            stream: false,
            reasoning_effort: None,
        };

        let result = run_monitor_probe(
            &test_outbound_client(),
            "http://127.0.0.1:9",
            "sk-real-key",
            &request,
            1,
            cancellation_token,
        )
        .await;

        assert!(!result.ok);
        assert_eq!(result.status_code, None);
        assert!(result
            .error_summary
            .unwrap()
            .to_ascii_lowercase()
            .contains("cancelled"));
    }

    #[test]
    fn normalizes_probe_timeout_to_minimum_one_second() {
        assert_eq!(probe_timeout(-5), Duration::from_secs(1));
        assert_eq!(probe_timeout(0), Duration::from_secs(1));
        assert_eq!(probe_timeout(3), Duration::from_secs(3));
    }

    fn test_outbound_client() -> AsyncOutboundClient {
        AsyncOutboundClient::new(crate::outbound::AsyncOutboundClientConfig::architecture_budget())
    }

    fn spawn_upstream(response: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let address = listener.local_addr().expect("local addr");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let size = stream.read(&mut buffer).expect("read request");
                if size == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..size]);
                if request_is_complete(&request) {
                    break;
                }
            }
            sender
                .send(String::from_utf8_lossy(&request).to_string())
                .expect("send request");
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        (format!("http://{address}"), receiver)
    }

    fn spawn_staged_upstream(parts: &'static [&'static str]) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let address = listener.local_addr().expect("local addr");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let size = stream.read(&mut buffer).expect("read request");
                if size == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..size]);
                if request_is_complete(&request) {
                    break;
                }
            }
            sender
                .send(String::from_utf8_lossy(&request).to_string())
                .expect("send request");
            for part in parts {
                stream
                    .write_all(part.as_bytes())
                    .expect("write response part");
                stream.flush().expect("flush response part");
                thread::sleep(Duration::from_millis(20));
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn request_is_complete(request: &[u8]) -> bool {
        let Some(header_end) = request.windows(4).position(|item| item == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                if name.eq_ignore_ascii_case("content-length") {
                    value.trim().parse::<usize>().ok()
                } else {
                    None
                }
            })
            .unwrap_or(0);
        request.len() >= header_end + 4 + content_length
    }
}
