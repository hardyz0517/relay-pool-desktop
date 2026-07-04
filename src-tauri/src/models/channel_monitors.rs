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
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub consecutive_failure_threshold: i64,
    pub fallback_models: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorRun {
    pub id: String,
    pub monitor_id: String,
    pub template_id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub http_status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub response_model: Option<String>,
    pub fallback_model: Option<String>,
    pub error_message: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateChannelMonitorRunInput {
    pub monitor_id: String,
    pub template_id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub status: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub http_status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub response_model: Option<String>,
    pub fallback_model: Option<String>,
    pub error_message: Option<String>,
}
