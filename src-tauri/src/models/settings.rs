use serde::{Deserialize, Serialize};

pub const DEFAULT_COLLECTOR_TIMEOUT_SECONDS: u16 = 60;
pub const MIN_COLLECTOR_TIMEOUT_SECONDS: u16 = 3;
pub const MAX_COLLECTOR_TIMEOUT_SECONDS: u16 = 300;

const fn default_published_status_interval_minutes() -> u16 {
    5
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub local_proxy_port: u16,
    pub local_proxy_start_on_launch: bool,
    pub local_key_masked: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub balance_interval_minutes: u16,
    pub group_rate_interval_minutes: u16,
    pub published_status_interval_minutes: u16,
    pub pricing_refresh_interval_minutes: u16,
    pub collector_timeout_seconds: u16,
    pub collector_max_concurrency: u16,
    pub developer_mode_enabled: bool,
    pub show_decision_explanation: bool,
    pub tray_behavior: String,
    pub data_dir: String,
    pub pending_data_dir: Option<String>,
    pub data_dir_change_requires_restart: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsInput {
    pub local_proxy_port: u16,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub balance_interval_minutes: u16,
    pub group_rate_interval_minutes: u16,
    #[serde(default = "default_published_status_interval_minutes")]
    pub published_status_interval_minutes: u16,
    pub pricing_refresh_interval_minutes: u16,
    pub collector_timeout_seconds: u16,
    pub collector_max_concurrency: u16,
    pub developer_mode_enabled: bool,
    #[serde(default)]
    pub show_decision_explanation: bool,
    pub tray_behavior: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_settings_input_allows_missing_newer_collector_fields() {
        let input: UpdateSettingsInput = serde_json::from_value(serde_json::json!({
            "localProxyPort": 8787,
            "collectorProxyMode": "direct",
            "collectorProxyUrl": null,
            "lowBalanceThresholdCny": 15.0,
            "collectorIntervalMinutes": 30,
            "balanceIntervalMinutes": 5,
            "groupRateIntervalMinutes": 20,
            "pricingRefreshIntervalMinutes": 60,
            "collectorTimeoutSeconds": 15,
            "collectorMaxConcurrency": 3,
            "developerModeEnabled": false
        }))
        .expect("old clients may omit scheduler fields");

        assert_eq!(input.published_status_interval_minutes, 5);
    }
}
