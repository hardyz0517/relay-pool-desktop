use serde::{Deserialize, Serialize};

use crate::models::{
    settings::{
        AppSettings, UpdateSettingsInput, MAX_COLLECTOR_TIMEOUT_SECONDS,
        MIN_COLLECTOR_TIMEOUT_SECONDS,
    },
    AppStatus,
};

use super::{invalid_input, TypeDescriptor};

const MAX_PROXY_URL_BYTES: usize = 2_048;
const MAX_BALANCE_THRESHOLD_CNY: f64 = 1_000_000_000.0;
const MAX_INTERVAL_MINUTES: u16 = 10_080;
const MAX_PUBLISHED_STATUS_INTERVAL_MINUTES: u16 = 1_440;
const MAX_LOCAL_ACCESS_KEY_BYTES: usize = 4_096;
const MAX_EXTERNAL_URL_BYTES: usize = 2_048;

const fn default_published_status_interval_minutes() -> u16 {
    5
}

pub type AppStatusDto = AppStatus;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct CcswitchImportResultDto {
    pub app: String,
    pub provider_name: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateLocalAccessKeyInputDto {
    pub value: String,
}

impl UpdateLocalAccessKeyInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The local access key payload is invalid.",
            )
        })?;
        if input.value.trim().is_empty() {
            return Err(invalid_input(
                "value",
                "required",
                "A local access key is required.",
            ));
        }
        if input.value.len() > MAX_LOCAL_ACCESS_KEY_BYTES {
            return Err(invalid_input(
                "value",
                "too_long",
                "The local access key is too long.",
            ));
        }
        if input.value.chars().any(char::is_control) {
            return Err(invalid_input(
                "value",
                "invalid_value",
                "The local access key is invalid.",
            ));
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenExternalUrlInputDto {
    pub url: String,
}

impl OpenExternalUrlInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The external URL payload is invalid.",
            )
        })?;
        if input.url.trim().is_empty() {
            return Err(invalid_input(
                "url",
                "required",
                "An external URL is required.",
            ));
        }
        if input.url.len() > MAX_EXTERNAL_URL_BYTES {
            return Err(invalid_input(
                "url",
                "too_long",
                "The external URL is too long.",
            ));
        }
        if input.url.chars().any(char::is_control) {
            return Err(invalid_input(
                "url",
                "invalid_value",
                "The external URL is invalid.",
            ));
        }
        Ok(input)
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
pub struct UpdateSettingsInputDto {
    pub local_proxy_port: u16,
    pub collector_proxy_mode: CollectorProxyModeDto,
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
            collector_proxy_mode: self.collector_proxy_mode.into_string(),
            collector_proxy_url: normalize_optional(self.collector_proxy_url),
            low_balance_threshold_cny: self.low_balance_threshold_cny,
            collector_interval_minutes: self.collector_interval_minutes,
            balance_interval_minutes: self.balance_interval_minutes,
            group_rate_interval_minutes: self.group_rate_interval_minutes,
            published_status_interval_minutes: self.published_status_interval_minutes,
            pricing_refresh_interval_minutes: self.pricing_refresh_interval_minutes,
            collector_timeout_seconds: self.collector_timeout_seconds,
            collector_max_concurrency: self.collector_max_concurrency,
            developer_mode_enabled: self.developer_mode_enabled,
            show_decision_explanation: self.show_decision_explanation,
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
        if !(1..=MAX_PUBLISHED_STATUS_INTERVAL_MINUTES)
            .contains(&self.published_status_interval_minutes)
        {
            return Err(invalid_input(
                "publishedStatusIntervalMinutes",
                "out_of_range",
                "The published status interval is out of range.",
            ));
        }
        if !(MIN_COLLECTOR_TIMEOUT_SECONDS..=MAX_COLLECTOR_TIMEOUT_SECONDS)
            .contains(&self.collector_timeout_seconds)
        {
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

impl From<AppSettings> for SettingsDto {
    fn from(value: AppSettings) -> Self {
        Self {
            local_proxy_port: value.local_proxy_port,
            local_proxy_start_on_launch: value.local_proxy_start_on_launch,
            local_key_masked: value.local_key_masked,
            collector_proxy_mode: value.collector_proxy_mode,
            collector_proxy_url: value.collector_proxy_url,
            low_balance_threshold_cny: value.low_balance_threshold_cny,
            collector_interval_minutes: value.collector_interval_minutes,
            balance_interval_minutes: value.balance_interval_minutes,
            group_rate_interval_minutes: value.group_rate_interval_minutes,
            published_status_interval_minutes: value.published_status_interval_minutes,
            pricing_refresh_interval_minutes: value.pricing_refresh_interval_minutes,
            collector_timeout_seconds: value.collector_timeout_seconds,
            collector_max_concurrency: value.collector_max_concurrency,
            developer_mode_enabled: value.developer_mode_enabled,
            show_decision_explanation: value.show_decision_explanation,
            tray_behavior: value.tray_behavior,
            data_dir: value.data_dir,
            pending_data_dir: value.pending_data_dir,
            data_dir_change_requires_restart: value.data_dir_change_requires_restart,
        }
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const SETTINGS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "SettingsDto",
    typescript: r#"export type EmptyInputDto = Record<string, never>;

export type AppStatusDto = {
  proxyRunning: boolean;
  localBaseUrl: string;
};

export type CollectorProxyModeInput = "direct" | "system" | "manual";

export type UpdateSettingsInputDto = {
  localProxyPort: number;
  collectorProxyMode: CollectorProxyModeInput;
  collectorProxyUrl: string | null;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  publishedStatusIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  developerModeEnabled: boolean;
  showDecisionExplanation?: boolean;
  trayBehavior?: "close_to_tray" | "minimize_to_tray" | "disabled";
};

export type UpdateLocalAccessKeyInputDto = {
  value: string;
};

export type OpenExternalUrlInputDto = {
  url: string;
};

export type SettingsDto = {
  localProxyPort: number;
  localProxyStartOnLaunch: boolean;
  localKeyMasked: string;
  collectorProxyMode: string;
  collectorProxyUrl: string | null;
  lowBalanceThresholdCny: number;
  collectorIntervalMinutes: number;
  balanceIntervalMinutes: number;
  groupRateIntervalMinutes: number;
  publishedStatusIntervalMinutes: number;
  pricingRefreshIntervalMinutes: number;
  collectorTimeoutSeconds: number;
  collectorMaxConcurrency: number;
  developerModeEnabled: boolean;
  showDecisionExplanation: boolean;
  trayBehavior: string;
  dataDir: string;
  pendingDataDir: string | null;
  dataDirChangeRequiresRestart: boolean;
};

export type CcswitchImportResultDto = {
  app: string;
  providerName: string;
  endpoint: string;
};"#,
};

#[cfg(test)]
mod input_contract_tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    fn valid_input() -> serde_json::Value {
        serde_json::json!({
            "localProxyPort": 8787,
            "collectorProxyMode": "direct",
            "collectorProxyUrl": null,
            "lowBalanceThresholdCny": 15.0,
            "collectorIntervalMinutes": 30,
            "balanceIntervalMinutes": 5,
            "groupRateIntervalMinutes": 20,
            "publishedStatusIntervalMinutes": 5,
            "pricingRefreshIntervalMinutes": 60,
            "collectorTimeoutSeconds": 15,
            "collectorMaxConcurrency": 3,
            "showDecisionExplanation": false,
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
            ("collectorProxyMode", serde_json::json!("inherit")),
            ("collectorProxyUrl", serde_json::json!("file:///private")),
            ("localProxyPort", serde_json::json!(0)),
            ("collectorTimeoutSeconds", serde_json::json!(2)),
            ("collectorTimeoutSeconds", serde_json::json!(301)),
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
    }

    #[test]
    fn update_settings_accepts_old_clients_without_the_published_status_interval() {
        let mut input = valid_input();
        input
            .as_object_mut()
            .expect("object")
            .remove("publishedStatusIntervalMinutes");

        let parsed = UpdateSettingsInputDto::parse(input).expect("old IPC payload is accepted");
        assert_eq!(parsed.published_status_interval_minutes, 5);
    }

    #[test]
    fn bootstrap_inputs_reject_unknown_empty_long_and_control_values() {
        assert!(UpdateLocalAccessKeyInputDto::parse(serde_json::json!({
            "value": "sk-local-fixture"
        }))
        .is_ok());
        assert!(OpenExternalUrlInputDto::parse(serde_json::json!({
            "url": "https://example.test"
        }))
        .is_ok());

        for value in [
            serde_json::json!({ "value": "" }),
            serde_json::json!({ "value": "   " }),
            serde_json::json!({ "value": "abc\n123" }),
            serde_json::json!({ "value": "a".repeat(MAX_LOCAL_ACCESS_KEY_BYTES + 1) }),
            serde_json::json!({ "value": "sk-local-fixture", "unexpected": true }),
        ] {
            let error = UpdateLocalAccessKeyInputDto::parse(value).expect_err("invalid key");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }

        for value in [
            serde_json::json!({ "url": "" }),
            serde_json::json!({ "url": "   " }),
            serde_json::json!({ "url": "https://example.test/\nnext" }),
            serde_json::json!({ "url": "h".repeat(MAX_EXTERNAL_URL_BYTES + 1) }),
            serde_json::json!({ "url": "https://example.test", "unexpected": true }),
        ] {
            let error = OpenExternalUrlInputDto::parse(value).expect_err("invalid url input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }
}
