use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests compile proxy normalization without proxy status consumers"
    )
)]
pub enum ProxyLifecycle {
    #[default]
    Stopped,
    Starting,
    Running,
    Draining,
    Stopping,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests compile proxy normalization without proxy status consumers"
    )
)]
pub struct ProxyStatus {
    pub running: bool,
    pub lifecycle: ProxyLifecycle,
    pub bind_addr: String,
    pub port: u16,
    pub started_at: Option<String>,
    pub last_error: Option<String>,
    pub active_requests: u32,
    pub request_count: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests compile shared models without the request-log projection"
    )
)]
pub struct RequestLog {
    pub id: String,
    pub request_id: Option<String>,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub stream: bool,
    pub status: String,
    pub lifecycle_status: Option<String>,
    pub station_key_id: Option<String>,
    pub station_id: Option<String>,
    pub upstream_base_url: Option<String>,
    pub fallback_count: i64,
    pub error_message: Option<String>,
    pub route_policy: Option<String>,
    pub route_reason: Option<String>,
    pub rejected_candidates_json: Option<String>,
    pub body_bytes: Option<i64>,
    pub attempt_count: Option<i64>,
    pub route_wait_ms: Option<i64>,
    pub upstream_headers_ms: Option<i64>,
    pub failure_source: Option<String>,
    pub attempts_json: Option<String>,
    pub completion_source: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub reasoning_effort: Option<String>,
    pub first_token_ms: Option<i64>,
    pub billing_mode: Option<String>,
    pub estimated_input_cost: Option<f64>,
    pub estimated_output_cost: Option<f64>,
    pub estimated_total_cost: Option<f64>,
    pub base_input_cost: Option<f64>,
    pub base_output_cost: Option<f64>,
    pub base_fixed_cost: Option<f64>,
    pub base_total_cost: Option<f64>,
    pub cost_currency: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub pricing_source: Option<String>,
    pub cost_status: Option<String>,
    pub group_binding_id: Option<String>,
    pub normalization_status: Option<String>,
    pub balance_scope: Option<String>,
    pub economic_context_json: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "source-included persistence tests compile proxy normalization without routing format consumers"
    )
)]
pub enum UpstreamApiFormat {
    Auto,
    OpenAiChatCompletions,
    OpenAiResponses,
    CustomOpenAiCompatible,
}

pub fn normalize_proxy_mode(value: &str, allow_inherit: bool) -> String {
    match value.trim() {
        "direct" => "direct".to_string(),
        "system" => "system".to_string(),
        "manual" => "manual".to_string(),
        "inherit" if allow_inherit => "inherit".to_string(),
        _ if allow_inherit => "inherit".to_string(),
        _ => "direct".to_string(),
    }
}

pub fn normalize_proxy_url(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_mode_normalization_respects_inheritance_scope() {
        assert_eq!(normalize_proxy_mode(" inherit ", true), "inherit");
        assert_eq!(normalize_proxy_mode("inherit", false), "direct");
        assert_eq!(normalize_proxy_mode("unknown", true), "inherit");
        assert_eq!(normalize_proxy_mode("unknown", false), "direct");
    }

    #[test]
    fn proxy_url_normalization_trims_and_rejects_empty_values() {
        assert_eq!(
            normalize_proxy_url(Some(" http://127.0.0.1:7890 ".to_string())),
            Some("http://127.0.0.1:7890".to_string())
        );
        assert_eq!(normalize_proxy_url(Some("  ".to_string())), None);
        assert_eq!(normalize_proxy_url(None), None);
    }
}
