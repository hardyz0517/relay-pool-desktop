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
}
