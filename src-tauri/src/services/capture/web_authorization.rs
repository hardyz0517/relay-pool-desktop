use serde_json::Value;

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

/// Sub2API installations protected by a browser challenge often establish a
/// usable session without going through the NewAPI `/api/user/self` endpoint.
/// Login responses are deliberately excluded: CF clearance may still be
/// written after the login response, so completion must wait for an identity
/// probe made with the settled browser session.
pub(crate) fn is_sub2api_completion_candidate(
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
        .trim_end_matches('/')
        .to_ascii_lowercase();
    let is_auth_endpoint = matches!(
        path.as_str(),
        "/api/v1/auth/me"
            | "/api/v1/auth/session"
            | "/api/v1/user/profile"
            | "/api/v1/user/info"
            | "/api/v1/user/self"
            | "/auth/me"
            | "/auth/session"
            | "/user/profile"
            | "/user/info"
            | "/user/self"
            | "/api/user/profile"
            | "/api/user/info"
            | "/api/user/self"
    );
    if !is_auth_endpoint {
        return false;
    }
    response_json.and_then(extract_verified_user_id).is_some()
        || response_json.is_some_and(contains_session_credential)
}

fn contains_session_credential(payload: &Value) -> bool {
    match payload {
        Value::Object(map) => {
            [
                "id",
                "access_token",
                "accessToken",
                "token",
                "cookie",
                "session",
                "session_cookie",
                "sessionCookie",
            ]
            .iter()
            .any(|name| map.contains_key(*name))
                || map.values().any(contains_session_credential)
        }
        Value::Array(items) => items.iter().any(contains_session_credential),
        _ => false,
    }
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

    #[test]
    fn recognizes_sub2api_identity_candidates_only() {
        let payload = json!({"data": {"id": 42}, "access_token": "token"});
        assert!(!is_sub2api_completion_candidate(
            "/api/v1/auth/login",
            Some(200),
            Some(&payload)
        ));
        assert!(is_sub2api_completion_candidate(
            "/api/v1/user/profile",
            Some(200),
            Some(&json!({"id": 42}))
        ));
        assert!(!is_sub2api_completion_candidate(
            "/api/v1/groups/available",
            Some(200),
            Some(&payload)
        ));
        assert!(is_sub2api_completion_candidate(
            "/auth/me",
            Some(200),
            Some(&json!({"user": {"email": "session@example.invalid"}, "cookie": "present"}))
        ));
    }
}
