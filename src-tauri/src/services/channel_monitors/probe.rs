use std::{
    io::Read,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::services::{
    channel_monitors::{
        redaction::{redact_monitor_json, redact_monitor_text},
        templates::{normalize_monitor_method, RenderedMonitorRequest},
    },
    proxy::observability::{ObservedUsage, SseUsageObserver},
    station_endpoints::build_api_url,
};

const MAX_RESPONSE_EXCERPT_BYTES: u64 = 4096;

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
    pub response_excerpt_redacted: Option<String>,
    pub usage: Option<MonitorProbeUsage>,
}

pub fn run_monitor_probe(
    base_url: &str,
    api_key: &str,
    request: &RenderedMonitorRequest,
    timeout_seconds: i64,
) -> MonitorProbeResult {
    let started_at = Instant::now();
    let Some(url) = build_probe_url(base_url, &request.path) else {
        return failed_result(
            started_at,
            None,
            "Invalid monitor request path; expected same-origin absolute path",
            None,
        );
    };
    let method = match normalize_monitor_method(&request.method) {
        Ok(method) => method,
        Err(error) => return failed_result(started_at, None, &error, None),
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
            None,
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
            None,
        );
    }

    let timeout = probe_timeout(timeout_seconds);
    let agent = ureq::AgentBuilder::new()
        .timeout(timeout)
        .timeout_connect(timeout)
        .timeout_read(timeout)
        .timeout_write(timeout)
        .build();
    let mut upstream = agent
        .request(&method, &url)
        .timeout(timeout)
        .set("Authorization", &format!("Bearer {api_key}"));

    for (name, value) in &request.headers {
        if !is_forbidden_header(name) {
            upstream = upstream.set(name, value);
        }
    }

    let response = if request.body.is_empty() {
        upstream.call()
    } else {
        upstream.send_bytes(&request.body)
    };

    match response {
        Ok(response) => response_result(started_at, response, request.stream),
        Err(ureq::Error::Status(_, response)) => {
            response_result(started_at, response, request.stream)
        }
        Err(error) => failed_result(
            started_at,
            None,
            &format!("Network probe failed: {error}"),
            None,
        ),
    }
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

fn response_result(
    started_at: Instant,
    response: ureq::Response,
    stream: bool,
) -> MonitorProbeResult {
    if stream {
        return streaming_response_result(started_at, response);
    }
    let status_code = response.status();
    let body = response_body(response);
    let response_json = body
        .as_ref()
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok());
    let excerpt = response_excerpt_from_body(body.as_deref(), response_json.as_ref());
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
        response_excerpt_redacted: excerpt,
        usage: response_json.as_ref().and_then(parse_monitor_probe_usage),
    }
}

fn streaming_response_result(started_at: Instant, response: ureq::Response) -> MonitorProbeResult {
    let status_code = response.status();
    let mut reader = response.into_reader();
    let mut observer = SseUsageObserver::default();
    let mut excerpt_bytes = Vec::new();
    let mut first_token_ms = None;
    let mut read_error = None;
    let mut buffer = [0_u8; 8192];

    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                read_error = Some(redact_monitor_text(&format!(
                    "Failed to read streaming monitor response: {error}"
                )));
                break;
            }
        };
        if first_token_ms.is_none() {
            first_token_ms = Some(elapsed_ms(started_at));
        }
        observer.push(&buffer[..count]);
        let remaining = MAX_RESPONSE_EXCERPT_BYTES as usize - excerpt_bytes.len();
        excerpt_bytes.extend_from_slice(&buffer[..count.min(remaining)]);
    }

    let ok = status_code < 400 && read_error.is_none();
    let error_summary = read_error.or_else(|| {
        (!ok).then(|| redact_monitor_text(&format!("Upstream returned HTTP {status_code}")))
    });
    let excerpt = response_excerpt_from_body(Some(&excerpt_bytes), None);

    MonitorProbeResult {
        ok,
        status_code: Some(status_code),
        latency_ms: elapsed_ms(started_at),
        first_token_ms,
        error_summary,
        response_excerpt_redacted: excerpt,
        usage: observer.usage().cloned().map(MonitorProbeUsage::from),
    }
}

fn response_body(response: ureq::Response) -> Option<Vec<u8>> {
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_EXCERPT_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    Some(bytes)
}

fn response_excerpt_from_body(body: Option<&[u8]>, parsed_json: Option<&Value>) -> Option<String> {
    let bytes = body?;
    if bytes.is_empty() {
        return None;
    }
    if let Some(value) = parsed_json {
        return serde_json::to_string(&redact_monitor_json(value)).ok();
    }
    Some(redact_monitor_text(&String::from_utf8_lossy(bytes)))
}

fn parse_monitor_probe_usage(value: &Value) -> Option<MonitorProbeUsage> {
    ObservedUsage::from_json(value).map(MonitorProbeUsage::from)
}

fn failed_result(
    started_at: Instant,
    status_code: Option<u16>,
    error_summary: &str,
    response_excerpt_redacted: Option<String>,
) -> MonitorProbeResult {
    MonitorProbeResult {
        ok: false,
        status_code,
        latency_ms: elapsed_ms(started_at),
        first_token_ms: None,
        error_summary: Some(redact_monitor_text(error_summary)),
        response_excerpt_redacted,
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

    #[test]
    fn sends_probe_with_authorization_and_parses_success_response() {
        let (origin, received) = spawn_upstream(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 28\r\n\r\n{\"ok\":true,\"token\":\"secret\"}",
        );
        let mut headers = HashMap::new();
        headers.insert("x-monitor".to_string(), "yes".to_string());
        let request = RenderedMonitorRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers,
            body: br#"{"model":"gpt-test"}"#.to_vec(),
            stream: false,
            reasoning_effort: None,
        };
        let base_url = format!("{origin}/v1");

        let result = run_monitor_probe(&base_url, "sk-probe-key", &request, 2);
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert_eq!(result.status_code, Some(200));
        assert_eq!(result.error_summary, None);
        assert!(result
            .response_excerpt_redacted
            .as_deref()
            .unwrap()
            .contains("[REDACTED]"));
        assert!(raw_request.starts_with("POST /v1/chat/completions HTTP/1.1"));
        assert!(raw_request.contains("Authorization: Bearer sk-probe-key"));
        assert!(raw_request.contains("x-monitor: yes"));
        assert!(raw_request.contains(r#"{"model":"gpt-test"}"#));
    }

    #[test]
    fn sends_probe_with_complete_api_namespace_without_duplicate_v1() {
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

        let result = run_monitor_probe(&base_url, "sk-probe-key", &request, 2);
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert!(raw_request.starts_with("POST /api/v3/chat/completions HTTP/1.1"));
    }

    #[test]
    fn streaming_probe_records_first_token_and_final_usage() {
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

        let result = run_monitor_probe(&base_url, "sk-probe-key", &request, 2);
        received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert!(result.first_token_ms.is_some());
        let usage = result.usage.expect("stream usage");
        assert_eq!(usage.prompt_tokens, Some(9));
        assert_eq!(usage.completion_tokens, Some(4));
        assert_eq!(usage.total_tokens, Some(13));
        assert_eq!(usage.cache_read_tokens, Some(3));
    }

    #[test]
    fn ignores_template_authorization_and_cookie_headers() {
        let (origin, received) =
            spawn_upstream("HTTP/1.1 204 No Content\r\nContent-Length: 0\r\n\r\n");
        let mut headers = HashMap::new();
        headers.insert(
            "authorization".to_string(),
            "Bearer sk-template".to_string(),
        );
        headers.insert("Cookie".to_string(), "session=secret".to_string());
        headers.insert("x-safe".to_string(), "safe".to_string());
        let request = RenderedMonitorRequest {
            method: "GET".to_string(),
            path: "/v1/models".to_string(),
            headers,
            body: Vec::new(),
            stream: false,
            reasoning_effort: None,
        };
        let base_url = format!("{origin}/v1");

        let result = run_monitor_probe(&base_url, "sk-real-key", &request, 2);
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert!(result.ok);
        assert!(raw_request.contains("Authorization: Bearer sk-real-key"));
        assert!(!raw_request.contains("sk-template"));
        assert!(!raw_request.contains("session=secret"));
        assert!(raw_request.contains("x-safe: safe"));
    }

    #[test]
    fn rejects_path_that_would_override_host() {
        let request = RenderedMonitorRequest {
            method: "GET".to_string(),
            path: "https://evil.example/v1/models".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
            stream: false,
            reasoning_effort: None,
        };

        let result = run_monitor_probe("http://127.0.0.1:9", "sk-real-key", &request, 1);

        assert!(!result.ok);
        assert_eq!(result.status_code, None);
        assert!(result.error_summary.unwrap().contains("path"));
    }

    #[test]
    fn rejects_paths_with_whitespace_or_dot_segments() {
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

            let result = run_monitor_probe("http://127.0.0.1:9", "sk-real-key", &request, 1);

            assert!(!result.ok, "{path} should be rejected");
            assert_eq!(result.status_code, None);
        }
    }

    #[test]
    fn rejects_invalid_or_unsupported_methods_at_probe_boundary() {
        for method in ["TRACE", "BAD METHOD", "POST\r\nX-Bad: yes"] {
            let request = RenderedMonitorRequest {
                method: method.to_string(),
                path: "/v1/models".to_string(),
                headers: HashMap::new(),
                body: Vec::new(),
                stream: false,
                reasoning_effort: None,
            };

            let result = run_monitor_probe("http://127.0.0.1:9", "sk-real-key", &request, 1);

            assert!(!result.ok, "{method} should be rejected");
            assert_eq!(result.status_code, None);
            assert!(result.error_summary.unwrap().contains("method"));
        }
    }

    #[test]
    fn rejects_invalid_forwarded_headers_at_probe_boundary() {
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

            let result = run_monitor_probe("http://127.0.0.1:9", "sk-real-key", &request, 1);

            assert!(!result.ok, "{name:?}: {value:?} should be rejected");
            assert_eq!(result.status_code, None);
            assert!(result.error_summary.unwrap().contains("header"));
        }
    }

    #[test]
    fn normalizes_probe_timeout_to_minimum_one_second() {
        assert_eq!(probe_timeout(-5), Duration::from_secs(1));
        assert_eq!(probe_timeout(0), Duration::from_secs(1));
        assert_eq!(probe_timeout(3), Duration::from_secs(3));
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
