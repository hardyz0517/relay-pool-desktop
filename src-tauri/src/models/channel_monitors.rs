use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorRequestTemplate {
    pub id: String,
    pub name: String,
    pub endpoint_kind: String,
    pub method: String,
    pub path: String,
    pub request_body_json: String,
    pub enabled: bool,
    pub built_in: bool,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelMonitorTemplateInput {
    pub name: String,
    pub endpoint_kind: String,
    pub method: String,
    pub path: String,
    pub request_body_json: String,
    pub enabled: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelMonitorTemplateInput {
    pub id: String,
    pub name: String,
    pub endpoint_kind: String,
    pub method: String,
    pub path: String,
    pub request_body_json: String,
    pub enabled: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitor {
    pub id: String,
    pub name: String,
    pub target_type: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub enabled: bool,
    pub pause_on_zero_balance: bool,
    pub balance_paused: bool,
    pub proxy_mode: String,
    pub proxy_url: Option<String>,
    pub protocol_kind: String,
    pub client_profile_id: String,
    pub client_profile_version: i64,
    pub primary_model: String,
    pub retry_max_attempts_per_model: i64,
    pub retry_initial_backoff_ms: i64,
    pub retry_max_backoff_ms: i64,
    pub risk_daily_probe_budget: i64,
    pub health_policy_mode: String,
    pub health_failure_threshold: i64,
    pub health_recovery_threshold: i64,
    pub attempt_timeout_ms: i64,
    pub execution_timeout_ms: i64,
    pub schedule_revision: i64,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub consecutive_failure_threshold: i64,
    pub fallback_models: Vec<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelMonitorInput {
    pub name: String,
    pub target_type: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub enabled: bool,
    pub pause_on_zero_balance: bool,
    pub proxy_mode: String,
    pub proxy_url: Option<String>,
    pub protocol_kind: String,
    pub client_profile_id: String,
    pub client_profile_version: i64,
    pub primary_model: String,
    pub retry_max_attempts_per_model: i64,
    pub retry_initial_backoff_ms: i64,
    pub retry_max_backoff_ms: i64,
    pub risk_daily_probe_budget: i64,
    pub health_policy_mode: String,
    pub health_failure_threshold: i64,
    pub health_recovery_threshold: i64,
    pub attempt_timeout_ms: i64,
    pub execution_timeout_ms: i64,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub consecutive_failure_threshold: i64,
    pub fallback_models: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateChannelMonitorInput {
    pub id: String,
    pub name: String,
    pub target_type: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub enabled: bool,
    pub pause_on_zero_balance: bool,
    pub proxy_mode: String,
    pub proxy_url: Option<String>,
    pub protocol_kind: String,
    pub client_profile_id: String,
    pub client_profile_version: i64,
    pub primary_model: String,
    pub retry_max_attempts_per_model: i64,
    pub retry_initial_backoff_ms: i64,
    pub retry_max_backoff_ms: i64,
    pub risk_daily_probe_budget: i64,
    pub health_policy_mode: String,
    pub health_failure_threshold: i64,
    pub health_recovery_threshold: i64,
    pub attempt_timeout_ms: i64,
    pub execution_timeout_ms: i64,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub consecutive_failure_threshold: i64,
    pub fallback_models: Vec<String>,
    pub note: Option<String>,
}
