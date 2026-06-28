use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKey {
    pub id: String,
    pub station_id: String,
    pub name: String,
    pub api_key_masked: String,
    pub api_key_present: bool,
    pub enabled: bool,
    pub priority: i64,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub status: String,
    pub last_checked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyPoolItem {
    pub id: String,
    pub station_id: String,
    pub station_name: String,
    pub station_type: String,
    pub station_base_url: String,
    pub name: String,
    pub api_key_masked: String,
    pub api_key_present: bool,
    pub enabled: bool,
    pub priority: i64,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub status: String,
    pub last_checked_at: Option<String>,
    pub last_used_at: Option<String>,
    pub note: Option<String>,
    pub capability_summary: Vec<String>,
    pub model_scope_summary: String,
    pub only_use_as_backup: bool,
    pub cooldown_until: Option<String>,
    pub success_rate: Option<f64>,
    pub avg_latency_ms: Option<i64>,
    pub consecutive_failures: i64,
    pub last_error_summary: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStationKeyInput {
    pub station_id: String,
    pub name: String,
    pub api_key: String,
    pub enabled: bool,
    pub priority: Option<i64>,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationKeyInput {
    pub id: String,
    pub station_id: String,
    pub name: String,
    pub api_key: Option<String>,
    pub enabled: bool,
    pub priority: i64,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub status: String,
    pub note: Option<String>,
}
