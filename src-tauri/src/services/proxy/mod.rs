use crate::models::proxy::UpstreamApiFormat;
use serde_json::Value;

pub mod adapters;
pub mod error;
pub mod http_request;
pub mod ingress;
pub mod legacy_runtime;
pub mod limits;
mod local_auth;
pub mod observability;
pub mod request;
pub mod responses_chat_fallback;
pub mod router;
pub mod routing_affinity;
pub mod routing_failure;
pub mod routing_health;
pub mod routing_policy;
pub mod routing_probe;
pub mod routing_snapshot;
pub mod routing_types;
pub mod runtime;
pub mod scheduler;

#[cfg(test)]
mod contract_tests;
#[cfg(test)]
mod test_support;

#[derive(Debug, Clone)]
pub struct RouteCandidate {
    pub station_key_id: String,
    pub station_id: String,
    pub station_endpoint_revision: i64,
    pub upstream_base_url: String,
    pub api_key: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub upstream_api_format: UpstreamApiFormat,
    pub priority: i64,
    pub max_concurrency: i64,
    pub load_factor: Option<i64>,
    pub schedulable: bool,
}

pub fn enabled_candidates(mut candidates: Vec<RouteCandidate>) -> Vec<RouteCandidate> {
    candidates.retain(|candidate| candidate.schedulable);
    candidates.sort_by_key(|candidate| candidate.priority);
    candidates
}

pub fn extract_chat_request_metadata(body: &Value) -> (Option<String>, bool) {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let stream = body.get("stream").and_then(Value::as_bool).unwrap_or(false);
    (model, stream)
}

pub fn should_fallback(status: u16) -> bool {
    if status < 400 {
        return false;
    }
    let failure = routing_failure::classify_route_failure(
        routing_failure::RouteFailureInput::http_status(status, false),
    );
    failure.retryable_before_output
        || matches!(
            failure.action,
            routing_failure::RouteFailureAction::HardFail
        )
}

pub fn openai_error(message: &str, status: &str) -> Value {
    let message = crate::services::secrets::mask::redact_text(message);
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
    let mut output = crate::services::secrets::mask::redact_text(message);
    if output.len() > 160 {
        let boundary = output
            .char_indices()
            .map(|(index, _)| index)
            .take_while(|index| *index <= 160)
            .last()
            .unwrap_or(0);
        output.truncate(boundary);
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

        let ids: Vec<_> = sorted
            .iter()
            .map(|item| item.station_key_id.as_str())
            .collect();
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
    fn openai_error_redacts_secret_like_message() {
        let value = openai_error(
            "upstream said Bearer sk-p8-secret-plaintext-canary",
            "upstream_error",
        );
        let text = serde_json::to_string(&value).expect("json");

        assert!(!text.contains("sk-p8-secret-plaintext-canary"));
        assert!(text.contains("[REDACTED]"));
    }

    #[test]
    fn redact_error_message_masks_key_like_content() {
        let message = "upstream rejected sk-real-secret-value";

        let redacted = redact_error_message(message);

        assert!(!redacted.contains("sk-real-secret-value"));
        assert!(redacted.contains("[REDACTED]"));
    }

    #[test]
    fn redact_error_message_truncates_utf8_without_panicking() {
        let message = "上游返回了很长的中文错误信息。".repeat(20);

        let redacted = redact_error_message(&message);

        assert!(redacted.ends_with("..."));
        assert!(redacted.len() <= 163);
    }

    #[test]
    fn should_fallback_only_for_retryable_upstream_statuses() {
        assert!(should_fallback(401));
        assert!(should_fallback(402));
        assert!(should_fallback(403));
        assert!(should_fallback(429));
        assert!(should_fallback(500));
        assert!(should_fallback(503));
        assert!(!should_fallback(400));
        assert!(should_fallback(404));
        assert!(!should_fallback(200));
    }

    fn candidate(id: &str, priority: i64) -> RouteCandidate {
        RouteCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            station_endpoint_revision: 1,
            upstream_base_url: "https://example.test/v1".to_string(),
            api_key: format!("sk-{id}"),
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            upstream_api_format: UpstreamApiFormat::Auto,
            priority,
            max_concurrency: 0,
            load_factor: None,
            schedulable: true,
        }
    }
}
