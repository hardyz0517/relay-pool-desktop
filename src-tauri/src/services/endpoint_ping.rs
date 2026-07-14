use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct EndpointPingProbeResult {
    pub ok: bool,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub error_summary: Option<String>,
}

pub fn ping_station_endpoint(base_url: &str, timeout: Duration) -> EndpointPingProbeResult {
    let url = endpoint_ping_url(base_url);
    let agent = ureq::AgentBuilder::new().timeout(timeout).build();
    let started_at = Instant::now();

    let response = match agent.head(&url).call() {
        Ok(response) => Ok(response),
        Err(ureq::Error::Status(_, response)) => Ok(response),
        Err(_) => match agent.get(&url).call() {
            Ok(response) => Ok(response),
            Err(ureq::Error::Status(_, response)) => Ok(response),
            Err(error) => Err(error),
        },
    };

    match response {
        Ok(response) => endpoint_ping_response_result(started_at, response),
        Err(error) => EndpointPingProbeResult {
            ok: false,
            status: "failed".to_string(),
            latency_ms: None,
            error_summary: Some(short_ping_error(&error.to_string())),
        },
    }
}

fn endpoint_ping_response_result(
    started_at: Instant,
    response: ureq::Response,
) -> EndpointPingProbeResult {
    let status_code = response.status();
    if (200..400).contains(&status_code) {
        EndpointPingProbeResult {
            ok: true,
            status: "success".to_string(),
            latency_ms: Some(started_at.elapsed().as_millis() as i64),
            error_summary: None,
        }
    } else {
        EndpointPingProbeResult {
            ok: false,
            status: "failed".to_string(),
            latency_ms: None,
            error_summary: Some(format!("HTTP {status_code}")),
        }
    }
}

pub fn endpoint_ping_url(base_url: &str) -> String {
    let trimmed = base_url.trim().trim_end_matches('/');
    trimmed.to_string()
}

fn short_ping_error(message: &str) -> String {
    const MAX_ERROR_CHARS: usize = 180;
    let compact = message.trim();

    if compact.chars().count() <= MAX_ERROR_CHARS {
        return compact.to_string();
    }

    compact.chars().take(MAX_ERROR_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    fn spawn_endpoint(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            let mut buffer = [0_u8; 512];
            let _ = stream.read(&mut buffer);
            stream.write_all(response.as_bytes()).expect("write");
        });
        format!("http://{addr}")
    }

    fn spawn_endpoint_with_head_error_then_get(response: &'static str) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let addr = listener.local_addr().expect("local addr");
        thread::spawn(move || {
            let (mut head_stream, _) = listener.accept().expect("accept head");
            let mut buffer = [0_u8; 512];
            let _ = head_stream.read(&mut buffer);
            drop(head_stream);

            let (mut get_stream, _) = listener.accept().expect("accept get");
            let mut buffer = [0_u8; 512];
            let _ = get_stream.read(&mut buffer);
            get_stream.write_all(response.as_bytes()).expect("write");
        });
        format!("http://{addr}")
    }

    #[test]
    fn endpoint_ping_uses_http_head_without_token_path() {
        let base_url = spawn_endpoint("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(result.ok);
        assert_eq!(result.status, "success");
        assert!(result.latency_ms.is_some());
        assert_eq!(result.error_summary, None);
    }

    #[test]
    fn endpoint_ping_reports_http_failure_without_model_request() {
        let base_url =
            spawn_endpoint("HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n");

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(!result.ok);
        assert_eq!(result.status, "failed");
        assert_eq!(result.latency_ms, None);
        assert!(result.error_summary.unwrap().contains("HTTP 503"));
    }

    #[test]
    fn endpoint_ping_normalizes_fallback_get_http_failure() {
        let base_url = spawn_endpoint_with_head_error_then_get(
            "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n",
        );

        let result = ping_station_endpoint(&base_url, Duration::from_secs(2));

        assert!(!result.ok);
        assert_eq!(result.status, "failed");
        assert_eq!(result.latency_ms, None);
        assert!(result.error_summary.unwrap().contains("HTTP 503"));
    }

    #[test]
    fn endpoint_ping_keeps_api_namespace_instead_of_falling_back_to_website_root() {
        let url = endpoint_ping_url("https://relay.example.com/v1/");

        assert_eq!(url, "https://relay.example.com/v1");
    }
}
