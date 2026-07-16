use serde::{Deserialize, Serialize};

use crate::models::{
    channel_monitors::{ChannelMonitor, ChannelMonitorRun},
    group_facts::{GroupRateRecord, StationGroupBinding},
    pricing::PricingRule,
    proxy::RequestLog,
    routing::{StationKeyCapabilities, StationKeyHealth, UpdateStationKeyCapabilitiesInput},
    station_keys::{KeyPoolItem, StationKey},
    stations::Station,
};

pub type StationKeyStatus = String;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SaveStationKeyMode {
    Create,
    Update,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StationKeyGroupSelectionKind {
    Keep,
    Clear,
    Set,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyGroupSelection {
    pub kind: StationKeyGroupSelectionKind,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStationKeyWithDefaultsInput {
    pub mode: SaveStationKeyMode,
    pub id: Option<String>,
    pub station_id: String,
    pub name: String,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub schedulable: Option<bool>,
    pub priority: Option<i64>,
    pub tier_label: Option<String>,
    pub balance_scope: Option<String>,
    pub status: Option<StationKeyStatus>,
    pub note: Option<String>,
    pub group_selection: StationKeyGroupSelection,
    pub capabilities: Option<UpdateStationKeyCapabilitiesInput>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveStationKeyWithDefaultsResult {
    pub station_key: StationKey,
    pub capabilities: StationKeyCapabilities,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationGroupOption {
    pub value: String,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<String>,
    pub group_category_override: Option<String>,
    pub effective_group_category: String,
    pub rate_source: Option<String>,
    pub selectable_for_remote_key: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMonitorRunsLoadStatus {
    Ok,
    Failed,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorSummary {
    pub monitor: ChannelMonitor,
    pub recent_runs: Vec<ChannelMonitorRun>,
    pub runs_load_status: ChannelMonitorRunsLoadStatus,
    pub latest_run: Option<ChannelMonitorRun>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusTimelinePoint {
    pub status: String,
    pub latency_ms: Option<i64>,
    pub endpoint_ping_ms: Option<i64>,
    pub checked_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusWindowSummary {
    pub window: String,
    pub total_count: i64,
    pub success_count: i64,
    pub failure_count: i64,
    pub warning_count: i64,
    pub availability_percent: Option<f64>,
    pub avg_latency_ms: Option<i64>,
    pub avg_endpoint_ping_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub latest_status: Option<String>,
    pub latest_error_message: Option<String>,
    pub timeline: Vec<ChannelStatusTimelinePoint>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusSummary {
    pub monitor: ChannelMonitor,
    pub recent: ChannelStatusWindowSummary,
    pub last24h: ChannelStatusWindowSummary,
    pub last7d: ChannelStatusWindowSummary,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusWorkspace {
    pub key_pool_items: Vec<KeyPoolItem>,
    pub request_logs: Vec<RequestLog>,
    pub station_key_health: Vec<StationKeyHealth>,
    pub channel_status_summaries: Vec<ChannelStatusSummary>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingComparisonWorkspace {
    pub stations: Vec<Station>,
    pub station_keys: Vec<StationKey>,
    pub group_bindings: Vec<StationGroupBinding>,
    pub group_rates: Vec<GroupRateRecord>,
    pub pricing_rules: Vec<PricingRule>,
    pub developer_mode_enabled: bool,
}
