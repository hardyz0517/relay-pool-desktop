use std::time::{Duration, Instant};

use serde_json::Value;

use crate::services::collectors::url::join_url;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedWebAuthorizationSession {
    pub cookie_header: String,
    pub newapi_user_id: String,
    pub session_source: String,
}

impl VerifiedWebAuthorizationSession {
    pub(crate) fn new(cookie_header: String, newapi_user_id: String) -> Self {
        Self {
            cookie_header,
            newapi_user_id,
            session_source: "web_authorization".to_string(),
        }
    }
}

pub(crate) fn build_cookie_header_from_pairs(pairs: &[(String, String)]) -> Option<String> {
    let mut parts = Vec::new();
    for (name, value) in pairs {
        let name = name.trim();
        let value = value.trim();
        if !name.is_empty() && !value.is_empty() {
            parts.push(format!("{name}={value}"));
        }
    }
    (!parts.is_empty()).then(|| parts.join("; "))
}

pub(crate) fn extract_verified_user_id(payload: &Value) -> Option<String> {
    super::extract_newapi_user_id(payload)
}

fn self_payload_reports_success(payload: &Value) -> bool {
    match payload.get("success") {
        Some(success) => success.as_bool() == Some(true),
        None => true,
    }
}

pub(crate) fn is_newapi_completion_candidate(
    request_path: &str,
    status: Option<i64>,
    response_json: Option<&Value>,
) -> bool {
    if !matches!(status, Some(200..=299)) {
        return false;
    }

    let path = request_path
        .split('?')
        .next()
        .unwrap_or(request_path)
        .trim_end_matches('/');
    let normalized_path = path.to_ascii_lowercase();
    let is_self_probe = normalized_path == "/api/user/self";
    let oauth_provider = normalized_path.strip_prefix("/api/oauth/");
    let is_oauth_callback = oauth_provider.is_some_and(|provider| {
        !provider.is_empty() && !provider.contains('/') && !provider.eq_ignore_ascii_case("state")
    });
    let Some(payload) = response_json else {
        return false;
    };

    ((is_self_probe && self_payload_reports_success(payload))
        || (is_oauth_callback && payload.get("success").and_then(Value::as_bool) == Some(true)))
        && extract_verified_user_id(payload).is_some()
}

pub(crate) fn verify_newapi_cookie_session(
    management_base_url: &str,
    cookie_header: &str,
    expected_user_id: &str,
    timeout: Duration,
) -> Result<VerifiedWebAuthorizationSession, String> {
    let cookie_header = cookie_header.trim();
    if cookie_header.is_empty() {
        return Err("Web authorization did not capture a usable Cookie header.".to_string());
    }
    let expected_user_id = expected_user_id.trim();
    if expected_user_id.is_empty() {
        return Err("Web authorization did not capture a usable user id.".to_string());
    }

    let url = join_url(management_base_url, "/api/user/self");
    let started = Instant::now();
    let response = match ureq::get(&url)
        .timeout(timeout)
        .set("Cookie", cookie_header)
        .set("New-Api-User", expected_user_id)
        .set("Accept", "application/json")
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(status, _)) => {
            return Err(format!(
                "Web authorization self probe returned HTTP {status} after {} ms.",
                started.elapsed().as_millis()
            ));
        }
        Err(error) => {
            return Err(format!("Web authorization self probe failed: {error}"));
        }
    };

    let text = response.into_string().unwrap_or_default();
    let payload = serde_json::from_str::<Value>(&text)
        .map_err(|error| format!("Web authorization self probe returned invalid JSON: {error}"))?;
    if !self_payload_reports_success(&payload) {
        return Err("Web authorization self probe returned an unsuccessful response.".to_string());
    }
    let user_id = extract_verified_user_id(&payload)
        .ok_or_else(|| "Web authorization self probe did not return a user id.".to_string())?;
    if user_id != expected_user_id {
        return Err("Web authorization self probe returned a different user id.".to_string());
    }

    Ok(VerifiedWebAuthorizationSession::new(
        cookie_header.to_string(),
        user_id,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn builds_cookie_header_from_non_empty_pairs() {
        let pairs = vec![
            ("session".to_string(), "abc".to_string()),
            ("".to_string(), "ignored".to_string()),
            ("theme".to_string(), "light".to_string()),
        ];

        assert_eq!(
            build_cookie_header_from_pairs(&pairs).as_deref(),
            Some("session=abc; theme=light")
        );
    }

    #[test]
    fn cookie_pairs_ignore_empty_names_and_values() {
        let pairs = vec![
            ("".to_string(), "abc".to_string()),
            ("session".to_string(), "".to_string()),
            ("session".to_string(), "abc".to_string()),
        ];

        assert_eq!(
            build_cookie_header_from_pairs(&pairs).as_deref(),
            Some("session=abc")
        );
    }

    #[test]
    fn extracts_verified_user_id_from_self_payload() {
        let payload = json!({
            "success": true,
            "data": {
                "id": 17
            }
        });

        assert_eq!(extract_verified_user_id(&payload).as_deref(), Some("17"));
    }

    #[test]
    fn verified_web_authorization_session_uses_stable_source() {
        let session =
            VerifiedWebAuthorizationSession::new("session=abc".to_string(), "42".to_string());

        assert_eq!(session.session_source, "web_authorization");
    }

    #[test]
    fn recognizes_successful_newapi_self_candidate() {
        let payload = json!({
            "success": true,
            "data": {
                "id": 42
            }
        });

        assert!(is_newapi_completion_candidate(
            "/api/user/self",
            Some(200),
            Some(&payload),
        ));
    }

    #[test]
    fn recognizes_successful_newapi_oauth_callback_candidate() {
        let payload = json!({
            "success": true,
            "data": {
                "id": 42
            }
        });

        assert!(is_newapi_completion_candidate(
            "/api/oauth/oidc",
            Some(200),
            Some(&payload),
        ));
        assert!(is_newapi_completion_candidate(
            "/api/oauth/custom-provider",
            Some(200),
            Some(&payload),
        ));
    }

    #[test]
    fn rejects_unauthenticated_or_unrelated_completion_candidates() {
        let payload = json!({
            "success": true,
            "data": {
                "id": 42
            }
        });

        assert!(!is_newapi_completion_candidate(
            "/api/user/self",
            Some(401),
            Some(&payload),
        ));
        assert!(!is_newapi_completion_candidate(
            "/api/token",
            Some(200),
            Some(&payload),
        ));
        assert!(!is_newapi_completion_candidate(
            "/api/user/self",
            Some(200),
            Some(&json!({ "success": true })),
        ));
        assert!(!is_newapi_completion_candidate(
            "/api/oauth/state",
            Some(200),
            Some(&payload),
        ));
        assert!(!is_newapi_completion_candidate(
            "/api/oauth/oidc",
            Some(200),
            Some(&json!({ "success": false, "data": { "id": 42 } })),
        ));
        assert!(!is_newapi_completion_candidate(
            "/api/user/self",
            Some(200),
            Some(&json!({ "success": false, "data": { "id": 42 } })),
        ));
    }
}

#[cfg(test)]
mod verification_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc::{self, Receiver},
        thread,
    };

    fn response(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }

    fn serve_once(response: String) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            let mut buffer = [0_u8; 4096];
            let _ = stream.read(&mut buffer);
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        format!("http://{address}")
    }

    fn serve_once_and_capture_request(response: String) -> (String, Receiver<String>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fixture");
        let address = listener.local_addr().expect("fixture address");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept request");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("set read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            while request.len() < 16 * 1024 {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        request.extend_from_slice(&buffer[..size]);
                        if request.windows(4).any(|window| window == b"\r\n\r\n") {
                            break;
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        break;
                    }
                    Err(error) => panic!("read request: {error}"),
                }
            }
            sender
                .send(String::from_utf8_lossy(&request).to_string())
                .expect("capture request");
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        (format!("http://{address}"), receiver)
    }

    fn verify_with_user_id_hint(
        base_url: &str,
        cookie_header: &str,
        user_id: &str,
        timeout: Duration,
    ) -> Result<VerifiedWebAuthorizationSession, String> {
        verify_newapi_cookie_session(base_url, cookie_header, user_id, timeout)
    }

    #[test]
    fn cookie_session_probe_sends_candidate_user_id_header() {
        let (base_url, request) = serve_once_and_capture_request(response(
            r#"{"success":true,"data":{"id":42,"quota":1}}"#,
        ));

        verify_with_user_id_hint(
            &base_url,
            "session=abc",
            "42",
            std::time::Duration::from_secs(5),
        )
        .expect("verified session");

        let request = request.recv().expect("captured request");
        assert!(request
            .lines()
            .any(|line| line.eq_ignore_ascii_case("New-Api-User: 42")));
    }

    #[test]
    fn verifies_cookie_session_with_newapi_self_endpoint() {
        let base_url = serve_once(response(r#"{"success":true,"data":{"id":42,"quota":1}}"#));

        let verified = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
            "42",
            std::time::Duration::from_secs(5),
        )
        .expect("verified session");

        assert_eq!(verified.newapi_user_id, "42");
        assert_eq!(verified.cookie_header, "session=abc");
        assert_eq!(verified.session_source, "web_authorization");
    }

    #[test]
    fn rejects_cookie_session_without_user_id() {
        let base_url = serve_once(response(r#"{"success":true,"data":{"quota":1}}"#));

        let error = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
            "42",
            std::time::Duration::from_secs(5),
        )
        .expect_err("missing user id should fail");

        assert!(error.contains("user id"));
    }

    #[test]
    fn rejects_unsuccessful_cookie_session_payload() {
        let base_url = serve_once(response(
            r#"{"success":false,"data":{"id":42},"message":"unauthorized"}"#,
        ));

        let error = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
            "42",
            std::time::Duration::from_secs(5),
        )
        .expect_err("unsuccessful envelope should fail");

        assert!(error.contains("unsuccessful"));
    }

    #[test]
    fn rejects_cookie_session_for_different_user_id() {
        let base_url = serve_once(response(r#"{"success":true,"data":{"id":43}}"#));

        let error = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
            "42",
            std::time::Duration::from_secs(5),
        )
        .expect_err("different user id should fail");

        assert!(error.contains("different user id"));
    }
}
