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
    proxy::build_upstream_url,
};

const MAX_RESPONSE_EXCERPT_BYTES: u64 = 4096;

#[derive(Debug, Clone)]
pub struct MonitorProbeResult {
    pub ok: bool,
    pub status_code: Option<u16>,
    pub latency_ms: i64,
    pub error_summary: Option<String>,
    pub response_excerpt_redacted: Option<String>,
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
        Ok(response) => response_result(started_at, response),
        Err(ureq::Error::Status(_, response)) => response_result(started_at, response),
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

    Some(build_upstream_url(base_url, path))
}

fn has_dot_segment(path: &str) -> bool {
    path.split('/')
        .any(|segment| segment == "." || segment == "..")
}

fn probe_timeout(timeout_seconds: i64) -> Duration {
    Duration::from_secs(timeout_seconds.max(1) as u64)
}

fn response_result(started_at: Instant, response: ureq::Response) -> MonitorProbeResult {
    let status_code = response.status();
    let excerpt = response_excerpt(response);
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
        error_summary,
        response_excerpt_redacted: excerpt,
    }
}

fn response_excerpt(response: ureq::Response) -> Option<String> {
    let mut bytes = Vec::new();
    response
        .into_reader()
        .take(MAX_RESPONSE_EXCERPT_BYTES)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() {
        return None;
    }
    let text = String::from_utf8_lossy(&bytes).to_string();
    if let Ok(value) = serde_json::from_str::<Value>(&text) {
        return serde_json::to_string(&redact_monitor_json(&value)).ok();
    }
    Some(redact_monitor_text(&text))
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
        error_summary: Some(redact_monitor_text(error_summary)),
        response_excerpt_redacted,
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
        let (base_url, received) = spawn_upstream(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 28\r\n\r\n{\"ok\":true,\"token\":\"secret\"}",
        );
        let mut headers = HashMap::new();
        headers.insert("x-monitor".to_string(), "yes".to_string());
        let request = RenderedMonitorRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers,
            body: br#"{"model":"gpt-test"}"#.to_vec(),
        };

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
    fn ignores_template_authorization_and_cookie_headers() {
        let (base_url, received) =
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
        };

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
