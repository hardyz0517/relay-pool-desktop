use serde_json::Value;

pub mod runtime;

#[derive(Debug, Clone)]
pub struct RouteCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub upstream_base_url: String,
    pub api_key: String,
    pub priority: i64,
}

pub fn enabled_candidates(mut candidates: Vec<RouteCandidate>) -> Vec<RouteCandidate> {
    candidates.sort_by_key(|candidate| candidate.priority);
    candidates
}

pub fn extract_chat_request_metadata(body: &Value) -> (Option<String>, bool) {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let stream = body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (model, stream)
}

pub fn build_upstream_url(base_url: &str, path: &str) -> String {
    let base = base_url.trim_end_matches('/');
    let path = path.trim_start_matches('/');
    if base.ends_with("/v1") && path.starts_with("v1/") {
        return format!("{}/{}", base, path.trim_start_matches("v1/"));
    }
    format!(
        "{}/{}",
        base,
        path
    )
}

pub fn should_fallback(status: u16) -> bool {
    status == 429 || (500..=599).contains(&status)
}

pub fn openai_error(message: &str, status: &str) -> Value {
    serde_json::json!({
        "error": {
            "message": message,
            "type": "relay_pool_error",
            "param": Value::Null,
            "code": status,
        }
    })
}

pub fn redact_error_message(message: &str) -> String {
    let mut output = message.to_string();
    for marker in ["sk-", "Bearer ", "token=", "session=", "authorization"] {
        if let Some(index) = output.to_lowercase().find(&marker.to_lowercase()) {
            output.truncate(index);
            output.push_str("[REDACTED]");
            return output;
        }
    }
    if output.len() > 160 {
        output.truncate(160);
        output.push_str("...");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enabled_candidates_sorts_by_priority() {
        let candidates = vec![
            candidate("key-b", 20),
            candidate("key-a", 0),
            candidate("key-c", 10),
        ];

        let sorted = enabled_candidates(candidates);

        let ids: Vec<_> = sorted.iter().map(|item| item.station_key_id.as_str()).collect();
        assert_eq!(ids, vec!["key-a", "key-c", "key-b"]);
    }

    #[test]
    fn extract_chat_request_metadata_detects_model_and_stream() {
        let body = serde_json::json!({
            "model": "gpt-test",
            "stream": true,
            "messages": []
        });

        let (model, stream) = extract_chat_request_metadata(&body);

        assert_eq!(model.as_deref(), Some("gpt-test"));
        assert!(stream);
    }

    #[test]
    fn openai_error_uses_compatible_shape() {
        let value = openai_error("No enabled keys", "no_enabled_keys");

        assert_eq!(value["error"]["message"], "No enabled keys");
        assert_eq!(value["error"]["type"], "relay_pool_error");
        assert_eq!(value["error"]["code"], "no_enabled_keys");
    }

    #[test]
    fn redact_error_message_masks_key_like_content() {
        let message = "upstream rejected sk-real-secret-value";

        let redacted = redact_error_message(message);

        assert!(!redacted.contains("sk-real-secret-value"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn build_upstream_url_normalizes_slashes() {
        let url = build_upstream_url("https://station.example/api/", "/v1/chat/completions");

        assert_eq!(url, "https://station.example/api/v1/chat/completions");
    }

    #[test]
    fn build_upstream_url_avoids_duplicate_v1_segment() {
        let url = build_upstream_url("https://station.example/v1", "/v1/models");

        assert_eq!(url, "https://station.example/v1/models");
    }

    #[test]
    fn should_fallback_only_for_retryable_upstream_statuses() {
        assert!(should_fallback(429));
        assert!(should_fallback(500));
        assert!(should_fallback(503));
        assert!(!should_fallback(400));
        assert!(!should_fallback(401));
        assert!(!should_fallback(200));
    }

    fn candidate(id: &str, priority: i64) -> RouteCandidate {
        RouteCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            upstream_base_url: "https://example.test/v1".to_string(),
            api_key: format!("sk-{id}"),
            priority,
        }
    }
}
