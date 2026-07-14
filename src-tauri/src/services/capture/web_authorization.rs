use std::time::{Duration, Instant};

use serde_json::Value;

use crate::services::station_endpoints::build_management_url;

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

pub(crate) fn is_newapi_completion_candidate(
    request_path: &str,
    status: Option<i64>,
    response_json: Option<&Value>,
) -> bool {
    matches!(status, Some(200..=299))
        && request_path
            .split('?')
            .next()
            .unwrap_or(request_path)
            .trim_end_matches('/')
            .eq_ignore_ascii_case("/api/user/self")
        && response_json.and_then(extract_verified_user_id).is_some()
}

pub(crate) fn verify_newapi_cookie_session(
    management_base_url: &str,
    cookie_header: &str,
    timeout: Duration,
) -> Result<VerifiedWebAuthorizationSession, String> {
    let cookie_header = cookie_header.trim();
    if cookie_header.is_empty() {
        return Err("Web authorization did not capture a usable Cookie header.".to_string());
    }

    let url = build_management_url(management_base_url, "/api/user/self")?;
    let started = Instant::now();
    let response = match ureq::get(&url)
        .timeout(timeout)
        .set("Cookie", cookie_header)
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
    let user_id = extract_verified_user_id(&payload)
        .ok_or_else(|| "Web authorization self probe did not return a user id.".to_string())?;

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
    }
}

#[cfg(test)]
mod verification_tests {
    use super::*;
    use std::{
        io::{Read, Write},
        net::TcpListener,
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

    #[test]
    fn verifies_cookie_session_with_newapi_self_endpoint() {
        let base_url = serve_once(response(r#"{"success":true,"data":{"id":42,"quota":1}}"#));

        let verified = verify_newapi_cookie_session(
            &base_url,
            "session=abc",
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
            std::time::Duration::from_secs(5),
        )
        .expect_err("missing user id should fail");

        assert!(error.contains("user id"));
    }
}
