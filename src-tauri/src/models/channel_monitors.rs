use serde::{Deserialize, Serialize};

use crate::models::pricing::RequestCostEstimate;

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

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorRunCursor {
    pub started_at_ms: i64,
    pub id: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorRunPage {
    pub items: Vec<ChannelMonitorRun>,
    pub next_cursor: Option<ChannelMonitorRunCursor>,
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

#[derive(Debug, Clone)]
pub(crate) struct MonitorProbeUsageEvidence {
    pub(crate) prompt_tokens: Option<i64>,
    pub(crate) completion_tokens: Option<i64>,
    pub(crate) total_tokens: Option<i64>,
    pub(crate) cache_creation_tokens: Option<i64>,
    pub(crate) cache_read_tokens: Option<i64>,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitorRequestPricingEvidence {
    pub(crate) estimate: RequestCostEstimate,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) normalization_status: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletedMonitorRequestEvidence {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) endpoint: String,
    pub(crate) model: String,
    pub(crate) stream: bool,
    pub(crate) reasoning_effort: Option<String>,
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) upstream_base_url: String,
    pub(crate) first_token_ms: Option<i64>,
    pub(crate) usage: Option<MonitorProbeUsageEvidence>,
    pub(crate) pricing: MonitorRequestPricingEvidence,
}

#[derive(Debug, Clone)]
pub(crate) struct CompletedMonitorProbe {
    pub(crate) run: CreateChannelMonitorRunInput,
    pub(crate) request: Option<CompletedMonitorRequestEvidence>,
}
