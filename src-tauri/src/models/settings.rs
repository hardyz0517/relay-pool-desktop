use serde::{Deserialize, Serialize};

use super::routing::{DispatchAlgorithmSettings, RoutingGroupFilter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub local_proxy_port: u16,
    pub local_proxy_start_on_launch: bool,
    pub local_key_masked: String,
    /// Compatibility projection of the canonical routing policy for older UI clients.
    #[serde(rename = "defaultRoutingStrategy")]
    pub routing_policy_name: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub max_rate_multiplier: Option<f64>,
    #[serde(rename = "defaultRoutingGroupFilter")]
    pub routing_group_scope: RoutingGroupFilter,
    #[serde(rename = "schedulerAdvancedSettings")]
    pub scheduler_config: DispatchAlgorithmSettings,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub balance_interval_minutes: u16,
    pub group_rate_interval_minutes: u16,
    pub pricing_refresh_interval_minutes: u16,
    pub collector_timeout_seconds: u16,
    pub collector_max_concurrency: u16,
    pub allow_depleted_fallback: bool,
    pub developer_mode_enabled: bool,
    pub tray_behavior: String,
    pub data_dir: String,
    pub pending_data_dir: Option<String>,
    pub data_dir_change_requires_restart: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSettingsInput {
    pub local_proxy_port: u16,
    #[serde(rename = "defaultRoutingStrategy")]
    pub routing_policy_name: String,
    pub collector_proxy_mode: String,
    pub collector_proxy_url: Option<String>,
    pub max_rate_multiplier: Option<Option<f64>>,
    #[serde(rename = "defaultRoutingGroupFilter")]
    pub routing_group_scope: Option<RoutingGroupFilter>,
    #[serde(rename = "schedulerAdvancedSettings")]
    pub scheduler_config: Option<DispatchAlgorithmSettings>,
    pub low_balance_threshold_cny: f64,
    pub collector_interval_minutes: u16,
    pub balance_interval_minutes: u16,
    pub group_rate_interval_minutes: u16,
    pub pricing_refresh_interval_minutes: u16,
    pub collector_timeout_seconds: u16,
    pub collector_max_concurrency: u16,
    pub allow_depleted_fallback: bool,
    pub developer_mode_enabled: bool,
    pub tray_behavior: Option<String>,
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
            "pricingRefreshIntervalMinutes": 60,
            "collectorTimeoutSeconds": 15,
            "collectorMaxConcurrency": 3,
            "allowDepletedFallback": false,
            "developerModeEnabled": false
        }))
        .expect("old clients may omit scheduler fields");

        assert!(input.routing_group_scope.is_none());
        assert!(input.scheduler_config.is_none());
    }
}
