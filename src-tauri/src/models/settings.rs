use serde::{Deserialize, Serialize};

use super::routing::{RoutingGroupFilter, SchedulerAdvancedSettings};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub local_proxy_port: u16,
    pub local_key_masked: String,
    pub default_routing_strategy: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub max_rate_multiplier: Option<f64>,
    pub default_routing_group_filter: RoutingGroupFilter,
    pub scheduler_advanced_settings: SchedulerAdvancedSettings,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub balance_interval_minutes: u16,
    pub group_rate_interval_minutes: u16,
    pub model_list_interval_minutes: u16,
    pub pricing_refresh_interval_minutes: u16,
    pub collector_timeout_seconds: u16,
    pub collector_max_concurrency: u16,
    pub allow_depleted_fallback: bool,
    pub tray_behavior: String,
    pub developer_mode_enabled: bool,
    pub data_dir: String,
    pub pending_data_dir: Option<String>,
    pub data_dir_change_requires_restart: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsInput {
    pub local_proxy_port: u16,
    pub default_routing_strategy: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub max_rate_multiplier: Option<Option<f64>>,
    pub default_routing_group_filter: Option<RoutingGroupFilter>,
    pub scheduler_advanced_settings: Option<SchedulerAdvancedSettings>,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub balance_interval_minutes: u16,
    pub group_rate_interval_minutes: u16,
    pub model_list_interval_minutes: u16,
    pub pricing_refresh_interval_minutes: u16,
    pub collector_timeout_seconds: u16,
    pub collector_max_concurrency: u16,
    pub allow_depleted_fallback: bool,
    pub tray_behavior: String,
    pub developer_mode_enabled: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn update_settings_input_allows_missing_scheduler_fields() {
        let input: UpdateSettingsInput = serde_json::from_value(serde_json::json!({
            "localProxyPort": 8787,
            "defaultRoutingStrategy": "automatic_balanced",
            "collectorProxyMode": "direct",
            "collectorProxyUrl": null,
            "lowBalanceThresholdCny": 15.0,
            "collectorIntervalMinutes": 30,
            "balanceIntervalMinutes": 5,
            "groupRateIntervalMinutes": 20,
            "modelListIntervalMinutes": 60,
            "pricingRefreshIntervalMinutes": 60,
            "collectorTimeoutSeconds": 15,
            "collectorMaxConcurrency": 3,
            "allowDepletedFallback": false,
            "trayBehavior": "minimize-to-tray",
            "developerModeEnabled": false
        }))
        .expect("old clients may omit scheduler fields");

        assert!(input.default_routing_group_filter.is_none());
        assert!(input.scheduler_advanced_settings.is_none());
    }
}
