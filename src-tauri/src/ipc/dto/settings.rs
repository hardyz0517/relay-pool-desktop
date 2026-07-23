use serde::{Deserialize, Serialize};

use crate::models::{
    routing::{RoutingGroupFilter, SchedulerAdvancedSettings},
    settings::{AppSettings, UpdateSettingsInput},
};

use super::{invalid_input, TypeDescriptor};

const MAX_PROXY_URL_BYTES: usize = 2_048;
const MAX_RATE_MULTIPLIER: f64 = 1_000.0;
const MAX_BALANCE_THRESHOLD_CNY: f64 = 1_000_000_000.0;
const MAX_INTERVAL_MINUTES: u16 = 10_080;
const MAX_TIMEOUT_SECONDS: u16 = 300;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoutingStrategyDto {
    AutomaticBalanced,
    PriorityFallback,
    StableFirst,
    BackupOnly,
    CheapFirst,
    CostStableFirst,
}

impl RoutingStrategyDto {
    fn into_string(self) -> String {
        serde_json::to_value(self)
            .expect("routing strategy enum serializes")
            .as_str()
            .expect("routing strategy serializes as a string")
            .to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CollectorProxyModeDto {
    Direct,
    System,
    Manual,
}

impl CollectorProxyModeDto {
    fn into_string(self) -> String {
        serde_json::to_value(self)
            .expect("proxy mode enum serializes")
            .as_str()
            .expect("proxy mode serializes as a string")
            .to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchedulerAdvancedSettingsInputDto {
    pub top_k: u16,
    pub multiplier: f64,
    pub priority: f64,
    pub load: f64,
    pub queue: f64,
    pub error_rate: f64,
    pub ttft: f64,
    pub quota_headroom: f64,
    pub previous_response: f64,
    pub session_sticky: f64,
    pub multiplier_min_confidence: f64,
    pub sticky_weighted: bool,
    pub sticky_escape: bool,
    pub sticky_escape_ttft_ms: u64,
    pub sticky_escape_error_rate: f64,
    pub sticky_session_ttl_seconds: u64,
    pub sticky_response_ttl_seconds: u64,
    pub sticky_max_waiting: u64,
    pub sticky_wait_timeout_seconds: u64,
    pub fallback_max_waiting: u64,
    pub fallback_wait_timeout_seconds: u64,
}

impl From<SchedulerAdvancedSettingsInputDto> for SchedulerAdvancedSettings {
    fn from(value: SchedulerAdvancedSettingsInputDto) -> Self {
        Self {
            top_k: value.top_k,
            multiplier: value.multiplier,
            priority: value.priority,
            load: value.load,
            queue: value.queue,
            error_rate: value.error_rate,
            ttft: value.ttft,
            quota_headroom: value.quota_headroom,
            previous_response: value.previous_response,
            session_sticky: value.session_sticky,
            multiplier_min_confidence: value.multiplier_min_confidence,
            sticky_weighted: value.sticky_weighted,
            sticky_escape: value.sticky_escape,
            sticky_escape_ttft_ms: value.sticky_escape_ttft_ms,
            sticky_escape_error_rate: value.sticky_escape_error_rate,
            sticky_session_ttl_seconds: value.sticky_session_ttl_seconds,
            sticky_response_ttl_seconds: value.sticky_response_ttl_seconds,
            sticky_max_waiting: value.sticky_max_waiting,
            sticky_wait_timeout_seconds: value.sticky_wait_timeout_seconds,
            fallback_max_waiting: value.fallback_max_waiting,
            fallback_wait_timeout_seconds: value.fallback_wait_timeout_seconds,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateSettingsInputDto {
    pub local_proxy_port: u16,
    pub default_routing_strategy: RoutingStrategyDto,
    pub collector_proxy_mode: CollectorProxyModeDto,
    pub collector_proxy_url: Option<String>,
    #[serde(deserialize_with = "deserialize_required_nullable")]
    pub max_rate_multiplier: Option<f64>,
    pub default_routing_group_filter: Option<RoutingGroupFilter>,
    pub scheduler_advanced_settings: Option<SchedulerAdvancedSettingsInputDto>,
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
    pub tray_behavior: Option<String>,
}

impl UpdateSettingsInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input("input", "invalid_shape", "The settings payload is invalid.")
        })?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> Result<UpdateSettingsInput, crate::commands::error::CommandError> {
        self.validate()?;
        Ok(UpdateSettingsInput {
            local_proxy_port: self.local_proxy_port,
            default_routing_strategy: self.default_routing_strategy.into_string(),
            collector_proxy_mode: self.collector_proxy_mode.into_string(),
            collector_proxy_url: normalize_optional(self.collector_proxy_url),
            max_rate_multiplier: Some(self.max_rate_multiplier),
            default_routing_group_filter: self.default_routing_group_filter,
            scheduler_advanced_settings: self.scheduler_advanced_settings.map(Into::into),
            low_balance_threshold_cny: self.low_balance_threshold_cny,
            collector_interval_minutes: self.collector_interval_minutes,
            balance_interval_minutes: self.balance_interval_minutes,
            group_rate_interval_minutes: self.group_rate_interval_minutes,
            model_list_interval_minutes: self.model_list_interval_minutes,
            pricing_refresh_interval_minutes: self.pricing_refresh_interval_minutes,
            collector_timeout_seconds: self.collector_timeout_seconds,
            collector_max_concurrency: self.collector_max_concurrency,
            allow_depleted_fallback: self.allow_depleted_fallback,
            developer_mode_enabled: self.developer_mode_enabled,
            tray_behavior: self.tray_behavior,
        })
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        if self.local_proxy_port == 0 {
            return Err(invalid_input(
                "localProxyPort",
                "out_of_range",
                "The local proxy port is out of range.",
            ));
        }
        if let Some(url) = self.collector_proxy_url.as_deref() {
            validate_proxy_url(url)?;
        }
        if matches!(self.collector_proxy_mode, CollectorProxyModeDto::Manual)
            && self
                .collector_proxy_url
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(invalid_input(
                "collectorProxyUrl",
                "required",
                "A manual proxy URL is required.",
            ));
        }
        if self.max_rate_multiplier.is_some_and(|value| {
            !value.is_finite() || !(0.0..=MAX_RATE_MULTIPLIER).contains(&value)
        }) {
            return Err(invalid_input(
                "maxRateMultiplier",
                "out_of_range",
                "The rate multiplier is out of range.",
            ));
        }
        if !self.low_balance_threshold_cny.is_finite()
            || !(0.0..=MAX_BALANCE_THRESHOLD_CNY).contains(&self.low_balance_threshold_cny)
        {
            return Err(invalid_input(
                "lowBalanceThresholdCny",
                "out_of_range",
                "The balance threshold is out of range.",
            ));
        }
        for (field, value) in [
            ("collectorIntervalMinutes", self.collector_interval_minutes),
            ("balanceIntervalMinutes", self.balance_interval_minutes),
            ("groupRateIntervalMinutes", self.group_rate_interval_minutes),
            ("modelListIntervalMinutes", self.model_list_interval_minutes),
            (
                "pricingRefreshIntervalMinutes",
                self.pricing_refresh_interval_minutes,
            ),
        ] {
            if !(1..=MAX_INTERVAL_MINUTES).contains(&value) {
                return Err(invalid_input(
                    field,
                    "out_of_range",
                    "The interval is out of range.",
                ));
            }
        }
        if !(3..=MAX_TIMEOUT_SECONDS).contains(&self.collector_timeout_seconds) {
            return Err(invalid_input(
                "collectorTimeoutSeconds",
                "out_of_range",
                "The timeout is out of range.",
            ));
        }
        if !(1..=8).contains(&self.collector_max_concurrency) {
            return Err(invalid_input(
                "collectorMaxConcurrency",
                "out_of_range",
                "The concurrency is out of range.",
            ));
        }
        if let Some(settings) = &self.scheduler_advanced_settings {
            let settings: SchedulerAdvancedSettings = settings.clone().into();
            settings.validate().map_err(|_| {
                invalid_input(
                    "schedulerAdvancedSettings",
                    "invalid_value",
                    "The scheduler settings are invalid.",
                )
            })?;
        }
        if self.tray_behavior.as_deref().is_some_and(|value| {
            !matches!(value, "close_to_tray" | "minimize_to_tray" | "disabled")
        }) {
            return Err(invalid_input(
                "trayBehavior",
                "invalid_enum",
                "The tray behavior is invalid.",
            ));
        }
        Ok(())
    }
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
}

fn deserialize_required_nullable<'de, D>(deserializer: D) -> Result<Option<f64>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<f64>::deserialize(deserializer)
}

fn validate_proxy_url(value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.len() > MAX_PROXY_URL_BYTES {
        return Err(invalid_input(
            "collectorProxyUrl",
            "too_long",
            "The proxy URL is too long.",
        ));
    }
    let parsed = url::Url::parse(value.trim()).map_err(|_| {
        invalid_input(
            "collectorProxyUrl",
            "invalid_url",
            "The proxy URL is invalid.",
        )
    })?;
    if !matches!(parsed.scheme(), "http" | "https" | "socks5") || parsed.host().is_none() {
        return Err(invalid_input(
            "collectorProxyUrl",
            "invalid_scheme",
            "The proxy URL scheme is not supported.",
        ));
    }
    Ok(())
}

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
    typescript: r#"export type EmptyInputDto = Record<string, never>;

export type RoutingGroupFilter =
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

export type RoutingStrategyInput =
  | "automatic_balanced"
  | "priority_fallback"
  | "stable_first"
  | "backup_only"
  | "cheap_first"
  | "cost_stable_first";

export type CollectorProxyModeInput = "direct" | "system" | "manual";

export type UpdateSettingsInputDto = {
  localProxyPort: number;
  defaultRoutingStrategy: RoutingStrategyInput;
  collectorProxyMode: CollectorProxyModeInput;
  collectorProxyUrl: string | null;
  maxRateMultiplier: number | null;
  defaultRoutingGroupFilter?: RoutingGroupFilter;
  schedulerAdvancedSettings?: SchedulerAdvancedSettingsDto | null;
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
  trayBehavior?: "close_to_tray" | "minimize_to_tray" | "disabled";
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

#[cfg(test)]
mod input_contract_tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    fn valid_input() -> serde_json::Value {
        serde_json::json!({
            "localProxyPort": 8787,
            "defaultRoutingStrategy": "automatic_balanced",
            "collectorProxyMode": "direct",
            "collectorProxyUrl": null,
            "maxRateMultiplier": null,
            "defaultRoutingGroupFilter": "all_groups",
            "schedulerAdvancedSettings": null,
            "lowBalanceThresholdCny": 15.0,
            "collectorIntervalMinutes": 30,
            "balanceIntervalMinutes": 5,
            "groupRateIntervalMinutes": 20,
            "modelListIntervalMinutes": 60,
            "pricingRefreshIntervalMinutes": 60,
            "collectorTimeoutSeconds": 15,
            "collectorMaxConcurrency": 3,
            "allowDepletedFallback": false,
            "developerModeEnabled": false
        })
    }

    #[test]
    fn update_settings_rejects_unknown_fields_with_a_typed_error() {
        let mut value = valid_input();
        value["unexpected"] = serde_json::json!(true);
        let error = UpdateSettingsInputDto::parse(value).expect_err("unknown field must fail");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);
    }

    #[test]
    fn update_settings_rejects_invalid_enums_urls_and_ranges() {
        for (field, value) in [
            ("defaultRoutingStrategy", serde_json::json!("random")),
            ("collectorProxyMode", serde_json::json!("inherit")),
            ("collectorProxyUrl", serde_json::json!("file:///private")),
            ("localProxyPort", serde_json::json!(0)),
            ("collectorTimeoutSeconds", serde_json::json!(2)),
            ("collectorMaxConcurrency", serde_json::json!(9)),
        ] {
            let mut input = valid_input();
            input[field] = value;
            let error = UpdateSettingsInputDto::parse(input).expect_err(field);
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "{field}");
        }
    }

    #[test]
    fn update_settings_validates_before_domain_conversion() {
        let input = UpdateSettingsInputDto::parse(valid_input()).expect("valid transport input");
        let domain = input.into_domain().expect("valid domain input");
        assert_eq!(domain.local_proxy_port, 8787);
        assert_eq!(domain.default_routing_strategy, "automatic_balanced");
        assert_eq!(domain.max_rate_multiplier, Some(None));

        let mut missing = valid_input();
        missing
            .as_object_mut()
            .expect("object")
            .remove("maxRateMultiplier");
        assert!(UpdateSettingsInputDto::parse(missing).is_err());
    }
}
