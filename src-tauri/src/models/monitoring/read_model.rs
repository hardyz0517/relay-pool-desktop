use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusWorkspaceWindow {
    Recent,
    Last24h,
    Last7d,
    Last30d,
}

impl Default for ChannelStatusWorkspaceWindow {
    fn default() -> Self {
        Self::Last24h
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusSortField {
    MonitorName,
    LatestCheckedAt,
    Availability,
    Latency,
    Status,
}

impl Default for ChannelStatusSortField {
    fn default() -> Self {
        Self::MonitorName
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusSortDirection {
    Asc,
    Desc,
}

impl Default for ChannelStatusSortDirection {
    fn default() -> Self {
        Self::Asc
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelStatusFilter {
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub outcome: Option<ChannelStatusOutcome>,
    #[serde(default)]
    pub station_id: Option<String>,
    #[serde(default)]
    pub protocol_kind: Option<String>,
    #[serde(default)]
    pub client_profile_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelStatusSort {
    #[serde(default)]
    pub field: ChannelStatusSortField,
    #[serde(default)]
    pub direction: ChannelStatusSortDirection,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelStatusCursor {
    pub row_key: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelStatusWorkspaceInput {
    #[serde(default)]
    pub window: ChannelStatusWorkspaceWindow,
    #[serde(default)]
    pub timezone_id: Option<String>,
    #[serde(default)]
    pub filter: ChannelStatusFilter,
    #[serde(default)]
    pub sort: ChannelStatusSort,
    #[serde(default)]
    pub cursor: Option<ChannelStatusCursor>,
    #[serde(default = "default_workspace_limit")]
    pub limit: Option<u32>,
}

fn default_workspace_limit() -> Option<u32> {
    Some(200)
}

impl Default for ChannelStatusWorkspaceInput {
    fn default() -> Self {
        Self {
            window: ChannelStatusWorkspaceWindow::default(),
            timezone_id: None,
            filter: ChannelStatusFilter::default(),
            sort: ChannelStatusSort::default(),
            cursor: None,
            limit: Some(200),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusWorkspaceV2 {
    pub schema_version: u32,
    pub generated_at_ms: i64,
    pub window: ChannelStatusWorkspaceWindow,
    pub timezone: ChannelStatusTimezone,
    pub bucket_layout: ChannelStatusBucketLayout,
    pub aggregate: ChannelStatusAggregate,
    pub freshness: ChannelStatusFreshness,
    pub page: ChannelStatusPage,
    pub rows: Vec<ChannelStatusRow>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusTimezone {
    pub id: String,
    pub source: ChannelStatusTimezoneSource,
    pub requested_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusTimezoneSource {
    Iana,
    UtcFallback,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusBucketLayout {
    pub recent_limit: u32,
    pub hourly: Vec<ChannelStatusBucketBoundary>,
    pub daily: Vec<ChannelStatusBucketBoundary>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusBucketBoundary {
    pub kind: ChannelStatusBucketKind,
    pub start_ms: i64,
    pub end_ms: i64,
    pub label: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusBucketKind {
    Hour,
    Day,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusPage {
    pub limit: u32,
    pub returned: u32,
    pub next_cursor: Option<ChannelStatusCursor>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusRow {
    pub row_key: String,
    pub monitor: ChannelStatusMonitor,
    pub target: ChannelStatusTarget,
    pub latest: Option<ChannelStatusLatestResult>,
    pub running: Option<ChannelStatusRunningExecution>,
    pub recent: Vec<ChannelStatusRecentPoint>,
    pub hourly_buckets: Vec<ChannelStatusBucket>,
    pub daily_buckets: Vec<ChannelStatusBucket>,
    pub selected_window: ChannelStatusWindowSummaryV2,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusMonitor {
    pub id: String,
    pub name: String,
    pub target_type: String,
    pub enabled: bool,
    pub pause_on_zero_balance: bool,
    pub balance_paused: bool,
    pub protocol_kind: String,
    pub client_profile_id: String,
    pub client_profile_version: i64,
    pub primary_model: String,
    pub fallback_models: Vec<String>,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub next_due_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusTarget {
    pub station_id: String,
    pub station_name: Option<String>,
    pub station_key_id: Option<String>,
    pub station_key_name: Option<String>,
    pub group_name: Option<String>,
    pub effective_group_category: Option<String>,
    pub endpoint_ping: Option<ChannelStatusEndpointPing>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusEndpointPing {
    pub status: String,
    pub latency_ms: Option<i64>,
    pub checked_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusLatestResult {
    pub target_result_id: String,
    pub execution_id: String,
    pub outcome: ChannelStatusOutcome,
    pub failure_kind: Option<String>,
    pub terminal_reason: Option<String>,
    pub http_status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub semantic_confidence: String,
    pub used_fallback: bool,
    pub attempt_count: i64,
    pub effective_model: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusRunningExecution {
    pub execution_id: String,
    pub status: String,
    pub trigger_kind: String,
    pub trigger_request_id: Option<String>,
    pub planned_at_ms: i64,
    pub started_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusRecentPoint {
    pub target_result_id: String,
    pub execution_id: String,
    pub outcome: ChannelStatusOutcome,
    pub failure_kind: Option<String>,
    pub terminal_reason: Option<String>,
    pub http_status: Option<i64>,
    pub latency_ms: Option<i64>,
    pub checked_at_ms: Option<i64>,
    pub used_fallback: bool,
    pub semantic_confidence: String,
    pub attempt_count: i64,
    pub effective_model: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusOutcome {
    Available,
    Degraded,
    Unavailable,
    Skipped,
    Missing,
}

impl ChannelStatusOutcome {
    pub fn from_probe_outcome(value: &str) -> Self {
        match value {
            "available" => Self::Available,
            "degraded" => Self::Degraded,
            "unavailable" => Self::Unavailable,
            "skipped" => Self::Skipped,
            _ => Self::Missing,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusBucket {
    pub kind: ChannelStatusBucketKind,
    pub start_ms: i64,
    pub end_ms: i64,
    pub state: ChannelStatusBucketState,
    pub counts: ChannelStatusBucketCounts,
    pub strict_availability_bps: Option<u32>,
    pub effective_availability_bps: Option<u32>,
    pub p50_latency_ms: Option<i64>,
    pub p95_latency_ms: Option<i64>,
    pub failure_counts: BTreeMap<String, u32>,
    pub exclusion_counts: BTreeMap<String, u32>,
    pub dirty: bool,
    pub corrupt: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatusBucketState {
    Missing,
    Dirty,
    SkippedOnly,
    Available,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusBucketCounts {
    pub total: u32,
    pub available: u32,
    pub degraded: u32,
    pub unavailable: u32,
    pub skipped: u32,
    pub excluded: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusWindowSummaryV2 {
    pub window: ChannelStatusWorkspaceWindow,
    pub bucket_kind: ChannelStatusBucketKind,
    pub start_ms: i64,
    pub end_ms: i64,
    pub counts: ChannelStatusBucketCounts,
    pub strict_availability_bps: Option<u32>,
    pub effective_availability_bps: Option<u32>,
    pub latest_outcome: ChannelStatusOutcome,
    pub latest_checked_at_ms: Option<i64>,
    pub dirty: bool,
    pub corrupt: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusAggregate {
    pub total_rows: u32,
    pub returned_rows: u32,
    pub running_rows: u32,
    pub available_rows: u32,
    pub degraded_rows: u32,
    pub unavailable_rows: u32,
    pub skipped_rows: u32,
    pub missing_rows: u32,
    pub dirty_rows: u32,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusFreshness {
    pub newest_result_at_ms: Option<i64>,
    pub oldest_result_at_ms: Option<i64>,
    pub has_dirty_rollups: bool,
    pub has_corrupt_rollups: bool,
    pub running_execution_count: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RunChannelMonitorNowInputV2 {
    pub monitor_id: String,
    pub trigger_request_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RunChannelMonitorReceipt {
    pub execution_id: String,
    pub monitor_id: String,
    pub status: String,
    pub trigger_request_id: String,
    pub reused_existing: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelChannelMonitorExecutionInput {
    pub execution_id: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CancelChannelMonitorExecutionReceipt {
    pub execution_id: String,
    pub status: String,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorExecutionIdInput {
    pub execution_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorExecutionCursor {
    pub started_at_ms: i64,
    pub execution_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorExecutionListInput {
    #[serde(default)]
    pub monitor_id: Option<String>,
    #[serde(default)]
    pub station_key_id: Option<String>,
    #[serde(default)]
    pub status: Option<String>,
    #[serde(default)]
    pub cursor: Option<ChannelMonitorExecutionCursor>,
    #[serde(default = "default_execution_limit")]
    pub limit: Option<u32>,
}

fn default_execution_limit() -> Option<u32> {
    Some(100)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorExecutionPage {
    pub items: Vec<ChannelMonitorExecutionSummaryV2>,
    pub next_cursor: Option<ChannelMonitorExecutionCursor>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorExecutionSummaryV2 {
    pub execution_id: String,
    pub monitor_id: String,
    pub status: String,
    pub trigger_kind: String,
    pub trigger_request_id: Option<String>,
    pub planned_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub finished_at_ms: Option<i64>,
    pub target_count: i64,
    pub available_count: i64,
    pub degraded_count: i64,
    pub unavailable_count: i64,
    pub skipped_count: i64,
    pub summary_outcome: Option<String>,
    pub summary_failure_kind: Option<String>,
    pub created_at_ms: i64,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorExecutionDetail {
    pub execution: ChannelMonitorExecutionSummaryV2,
    pub targets: Vec<ChannelMonitorTargetResultRecord>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorTargetResultRecord {
    pub target_result_id: String,
    pub execution_id: String,
    pub monitor_id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub terminal_outcome: String,
    pub terminal_failure_kind: Option<String>,
    pub terminal_reason: Option<String>,
    pub requested_model: String,
    pub effective_model: Option<String>,
    pub used_fallback: bool,
    pub attempt_count: i64,
    pub decisive_attempt_id: Option<String>,
    pub protocol_kind: Option<String>,
    pub resolved_adapter_kind: String,
    pub resolved_dialect: Option<String>,
    pub client_profile_id: String,
    pub client_profile_version: i64,
    pub request_profile_hash: Option<String>,
    pub traffic_equivalence: String,
    pub latency_ms: Option<i64>,
    pub availability_eligible: bool,
    pub latency_eligible: bool,
    pub exclusion_reason: Option<String>,
    pub technical_health_effect: String,
    pub semantic_confidence: String,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorAttemptCursor {
    pub started_at_ms: i64,
    pub attempt_id: String,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorAttemptHistoryInput {
    pub execution_id: String,
    #[serde(default)]
    pub station_key_id: Option<String>,
    #[serde(default)]
    pub cursor: Option<ChannelMonitorAttemptCursor>,
    #[serde(default = "default_attempt_limit")]
    pub limit: Option<u32>,
}

fn default_attempt_limit() -> Option<u32> {
    Some(100)
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorAttemptPage {
    pub items: Vec<ChannelMonitorAttemptRecord>,
    pub next_cursor: Option<ChannelMonitorAttemptCursor>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMonitorAttemptRecord {
    pub attempt_id: String,
    pub execution_id: String,
    pub monitor_id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub model: String,
    pub model_role: String,
    pub model_index: i64,
    pub attempt_number: i64,
    pub protocol_kind: String,
    pub client_profile_id: String,
    pub client_profile_version: i64,
    pub request_profile_hash: String,
    pub transport_mode: String,
    pub started_at_ms: i64,
    pub finished_at_ms: Option<i64>,
    pub latency_ms: Option<i64>,
    pub http_status: Option<i64>,
    pub outcome: String,
    pub failure_kind: Option<String>,
    pub retryable: bool,
    pub response_model: Option<String>,
    pub content_extracted: bool,
    pub validation_passed: bool,
    pub output_bytes: i64,
    pub error_summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringCapabilityCatalog {
    pub protocols: Vec<MonitoringProtocolCapability>,
    pub profiles: Vec<MonitoringClientProfileCapability>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringProtocolCapability {
    pub id: String,
    pub enabled: bool,
    pub streaming: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct MonitoringClientProfileCapability {
    pub id: String,
    pub version: u32,
    pub enabled: bool,
    pub cli_compat: bool,
    pub supported_protocols: Vec<String>,
    pub method: String,
    pub path: String,
    pub header_names: Vec<String>,
    pub body_defaults: Vec<String>,
    pub profile_hash: String,
}
