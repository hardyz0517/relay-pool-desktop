use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub local_proxy_port: u16,
    pub local_key_masked: String,
    pub default_routing_strategy: String,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub tray_behavior: String,
    pub developer_mode_enabled: bool,
    pub data_dir: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsInput {
    pub local_proxy_port: u16,
    pub default_routing_strategy: String,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub tray_behavior: String,
    pub developer_mode_enabled: bool,
}
