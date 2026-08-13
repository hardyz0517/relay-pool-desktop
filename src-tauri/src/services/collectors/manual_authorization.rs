use http::{header, HeaderMap};
use serde_json::Value;

const INTERACTIVE_MARKERS: &[&str] = &[
    "captcha",
    "geetest",
    "g-recaptcha",
    "grecaptcha",
    "h-captcha",
    "hcaptcha",
    "cf-turnstile",
    "turnstile",
    "challenge-platform",
    "verify you are human",
    "verification required",
    "human verification",
    "login required",
    "please log in",
    "please login",
    "session expired",
    "login expired",
    "登录已过期",
    "会话已过期",
    "请先登录",
    "未登录",
    "人机验证",
    "安全验证",
];

const INTERACTIVE_JSON_FLAGS: &[&str] = &[
    "captcha_required",
    "manual_required",
    "requires_2fa",
    "verification_required",
];

pub(crate) const ERROR_CODE: &str = "manual_authorization_required";
pub(crate) const MESSAGE: &str = "当前登录状态已失效，请重新进行窗口授权";
pub(crate) const RECOMMENDED_ACTION: &str = "reauthorize";

/// Detects a response that cannot be completed by the non-interactive
/// collector. The result deliberately does not identify a vendor: callers
/// only need the stable recovery action, `reauthorize`.
pub(crate) fn response_requires_manual_authorization(
    status: u16,
    headers: &HeaderMap,
    final_url: &str,
    body: &[u8],
) -> bool {
    if headers
        .get("cf-mitigated")
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| value.eq_ignore_ascii_case("challenge"))
    {
        return true;
    }

    if let Ok(value) = serde_json::from_slice::<Value>(body) {
        if json_has_manual_authorization_flag(&value)
            || (!(200..300).contains(&status) && json_has_interactive_message(&value))
        {
            return true;
        }
    }

    let body_sample = String::from_utf8_lossy(&body[..body.len().min(32 * 1024)]).to_lowercase();
    let url = final_url.to_lowercase();
    let content_type = headers
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_lowercase();
    let is_html = content_type.contains("text/html")
        || body_sample.contains("<!doctype html")
        || body_sample.contains("<html");
    let has_interactive_marker = INTERACTIVE_MARKERS
        .iter()
        .any(|marker| body_sample.contains(marker));
    let redirected_to_interactive_page = ["/login", "/signin", "/challenge", "/captcha", "/verify"]
        .iter()
        .any(|marker| url.contains(marker));
    let has_login_form = body_sample.contains("<form")
        && (body_sample.contains("type=\"password\"") || body_sample.contains("type='password'"));

    is_html && (has_interactive_marker || redirected_to_interactive_page || has_login_form)
}

fn json_has_manual_authorization_flag(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.iter().any(|(key, value)| {
            (INTERACTIVE_JSON_FLAGS
                .iter()
                .any(|candidate| key.eq_ignore_ascii_case(candidate))
                && value.as_bool() == Some(true))
                || json_has_manual_authorization_flag(value)
        }),
        Value::Array(values) => values.iter().any(json_has_manual_authorization_flag),
        _ => false,
    }
}

fn json_has_interactive_message(value: &Value) -> bool {
    match value {
        Value::Object(map) => map.values().any(json_has_interactive_message),
        Value::Array(values) => values.iter().any(json_has_interactive_message),
        Value::String(value) => {
            let value = value.to_lowercase();
            INTERACTIVE_MARKERS
                .iter()
                .any(|marker| value.contains(marker))
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::HeaderValue;
    use serde_json::json;

    #[test]
    fn detects_explicit_challenge_header_and_generic_json_flags() {
        let mut headers = HeaderMap::new();
        headers.insert("cf-mitigated", HeaderValue::from_static("challenge"));
        assert!(response_requires_manual_authorization(
            403,
            &headers,
            "https://relay.example/api/data",
            b""
        ));

        let body = serde_json::to_vec(&json!({"data": {"captcha_required": true}}))
            .expect("serialize fixture");
        assert!(response_requires_manual_authorization(
            403,
            &HeaderMap::new(),
            "https://relay.example/api/data",
            &body
        ));
    }

    #[test]
    fn detects_interactive_html_but_not_an_unexplained_forbidden_response() {
        let mut html_headers = HeaderMap::new();
        html_headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/html"));
        assert!(response_requires_manual_authorization(
            403,
            &html_headers,
            "https://relay.example/security-check",
            b"<html><form><div class='geetest'></div></form></html>"
        ));
        assert!(!response_requires_manual_authorization(
            403,
            &HeaderMap::new(),
            "https://relay.example/api/data",
            br#"{"error":"forbidden"}"#
        ));
    }

    #[test]
    fn does_not_classify_incidental_marker_text_in_a_successful_json_payload() {
        let body = serde_json::to_vec(&json!({"groupName": "captcha research"}))
            .expect("serialize fixture");

        assert!(!response_requires_manual_authorization(
            200,
            &HeaderMap::new(),
            "https://relay.example/api/groups",
            &body,
        ));
    }
}
