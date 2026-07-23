use serde::{Deserialize, Serialize};

use crate::models::{
    routing::{RoutingGroupFilter, SchedulerAdvancedSettings},
    settings::AppSettings,
};

use super::TypeDescriptor;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub local_proxy_port: u16,
    pub local_proxy_start_on_launch: bool,
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
    pub developer_mode_enabled: bool,
    pub tray_behavior: String,
    pub data_dir: String,
    pub pending_data_dir: Option<String>,
    pub data_dir_change_requires_restart: bool,
}

impl From<AppSettings> for SettingsDto {
    fn from(value: AppSettings) -> Self {
        Self {
            local_proxy_port: value.local_proxy_port,
            local_proxy_start_on_launch: value.local_proxy_start_on_launch,
            local_key_masked: value.local_key_masked,
            default_routing_strategy: value.default_routing_strategy,
            collector_proxy_mode: value.collector_proxy_mode,
            collector_proxy_url: value.collector_proxy_url,
            max_rate_multiplier: value.max_rate_multiplier,
            default_routing_group_filter: value.default_routing_group_filter,
            scheduler_advanced_settings: value.scheduler_advanced_settings,
            low_balance_threshold_cny: value.low_balance_threshold_cny,
            collector_interval_minutes: value.collector_interval_minutes,
            balance_interval_minutes: value.balance_interval_minutes,
            group_rate_interval_minutes: value.group_rate_interval_minutes,
            model_list_interval_minutes: value.model_list_interval_minutes,
            pricing_refresh_interval_minutes: value.pricing_refresh_interval_minutes,
            collector_timeout_seconds: value.collector_timeout_seconds,
            collector_max_concurrency: value.collector_max_concurrency,
            allow_depleted_fallback: value.allow_depleted_fallback,
            developer_mode_enabled: value.developer_mode_enabled,
            tray_behavior: value.tray_behavior,
            data_dir: value.data_dir,
            pending_data_dir: value.pending_data_dir,
            data_dir_change_requires_restart: value.data_dir_change_requires_restart,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const SETTINGS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "SettingsDto",
    typescript: r#"export type RoutingGroupFilter =
  | "all_groups"
  | "ungrouped_only"
  | { group_binding_id: string }
  | { group_id_hash: string }
  | { group_type: "gpt" | "claude" | "gemini" | "grok" | "image_generation" };

export type SchedulerAdvancedSettingsDto = {
  topK: number;
  multiplier: number;
  priority: number;
  load: number;
  queue: number;
  errorRate: number;
  ttft: number;
  quotaHeadroom: number;
  previousResponse: number;
  sessionSticky: number;
  multiplierMinConfidence: number;
  stickyWeighted: boolean;
  stickyEscape: boolean;
  stickyEscapeTtftMs: number;
  stickyEscapeErrorRate: number;
  stickySessionTtlSeconds: number;
  stickyResponseTtlSeconds: number;
  stickyMaxWaiting: number;
  stickyWaitTimeoutSeconds: number;
  fallbackMaxWaiting: number;
  fallbackWaitTimeoutSeconds: number;
};

export type SettingsDto = {
  localProxyPort: number;
  localProxyStartOnLaunch: boolean;
  localKeyMasked: string;
  defaultRoutingStrategy: string;
  collectorProxyMode: string;
  collectorProxyUrl: string | null;
  maxRateMultiplier: number | null;
  defaultRoutingGroupFilter: RoutingGroupFilter;
  schedulerAdvancedSettings: SchedulerAdvancedSettingsDto;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  modelListIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  allowDepletedFallback: boolean;
  developerModeEnabled: boolean;
  trayBehavior: string;
  dataDir: string;
  pendingDataDir: string | null;
  dataDirChangeRequiresRestart: boolean;
};"#,
};
