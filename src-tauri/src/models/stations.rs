use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Station {
    pub id: String,
    pub name: String,
    pub station_type: String,
    pub website_url: String,
    pub api_base_url: String,
    pub endpoint_revision: i64,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub api_key_masked: String,
    pub api_key_present: bool,
    pub key_count: i64,
    pub enabled: bool,
    pub priority: i64,
    pub credit_per_cny: f64,
    pub balance_raw: Option<f64>,
    pub balance_cny: Option<f64>,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub last_checked_at: Option<String>,
    pub last_pricing_fetched_at: Option<String>,
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationEndpointHealth {
    pub station_id: String,
    pub endpoint_revision: i64,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub checked_at: Option<String>,
    pub error_summary: Option<String>,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointPingResult {
    pub station_id: String,
    pub ok: bool,
    pub status: String,
    pub latency_ms: Option<i64>,
    pub checked_at: String,
    pub error_summary: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateStationInput {
    pub name: String,
    pub station_type: String,
    pub website_url: String,
    pub api_base_url: String,
    pub api_key: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub enabled: bool,
    pub credit_per_cny: f64,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStationInput {
    pub id: String,
    pub name: String,
    pub station_type: String,
    pub website_url: String,
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub enabled: bool,
    pub credit_per_cny: f64,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub note: Option<String>,
}
