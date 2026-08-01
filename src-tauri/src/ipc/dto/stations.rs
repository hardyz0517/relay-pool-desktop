use serde::{Deserialize, Serialize};

use std::collections::HashSet;

use crate::models::stations::{CreateStationInput, Station, UpdateStationInput};

use super::{invalid_input, TypeDescriptor};

pub const MAX_STATION_NAME_BYTES: usize = 128;
const MAX_STATION_ID_BYTES: usize = 128;
const MAX_URL_BYTES: usize = 2_048;
const MAX_API_KEY_BYTES: usize = 8_192;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_REORDER_STATIONS: usize = 1_000;
const MAX_CREDIT_PER_CNY: f64 = 1_000_000.0;
const MAX_BALANCE_THRESHOLD_CNY: f64 = 1_000_000_000.0;
const MAX_COLLECTION_INTERVAL_MINUTES: u16 = 10_080;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum StationTypeDto {
    Sub2api,
    Newapi,
}

impl StationTypeDto {
    fn into_string(self) -> String {
        serde_json::to_value(self)
            .expect("station type enum serializes")
            .as_str()
            .expect("station type serializes as a string")
            .to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationProxyModeDto {
    Inherit,
    Direct,
    System,
    Manual,
}

impl StationProxyModeDto {
    fn into_string(self) -> String {
        serde_json::to_value(self)
            .expect("station proxy mode enum serializes")
            .as_str()
            .expect("station proxy mode serializes as a string")
            .to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateStationInputDto {
    pub name: String,
    pub station_type: StationTypeDto,
    pub website_url: String,
    pub api_base_url: String,
    pub api_key: String,
    pub collector_proxy_mode: StationProxyModeDto,
    pub collector_proxy_url: Option<String>,
    pub enabled: bool,
    pub credit_per_cny: f64,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub note: Option<String>,
}

impl CreateStationInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value, "The station payload is invalid.")?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> Result<CreateStationInput, crate::commands::error::CommandError> {
        self.validate()?;
        Ok(CreateStationInput {
            name: self.name.trim().to_owned(),
            station_type: self.station_type.into_string(),
            website_url: self.website_url.trim().to_owned(),
            api_base_url: self.api_base_url.trim().to_owned(),
            api_key: self.api_key,
            collector_proxy_mode: self.collector_proxy_mode.into_string(),
            collector_proxy_url: normalize_optional(self.collector_proxy_url),
            enabled: self.enabled,
            credit_per_cny: self.credit_per_cny,
            low_balance_threshold_cny: self.low_balance_threshold_cny,
            collection_interval_minutes: self.collection_interval_minutes,
            note: normalize_optional(self.note),
        })
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_station_fields(
            &self.name,
            &self.website_url,
            &self.api_base_url,
            &self.api_key,
            &self.collector_proxy_mode,
            self.collector_proxy_url.as_deref(),
            self.credit_per_cny,
            self.low_balance_threshold_cny,
            self.collection_interval_minutes,
            self.note.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateStationInputDto {
    pub id: String,
    pub name: String,
    pub station_type: StationTypeDto,
    pub website_url: String,
    pub api_base_url: String,
    pub api_key: Option<String>,
    pub collector_proxy_mode: StationProxyModeDto,
    pub collector_proxy_url: Option<String>,
    pub enabled: bool,
    pub credit_per_cny: f64,
    pub low_balance_threshold_cny: Option<f64>,
    pub collection_interval_minutes: u16,
    pub note: Option<String>,
}

impl UpdateStationInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value, "The station update payload is invalid.")?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> Result<UpdateStationInput, crate::commands::error::CommandError> {
        self.validate()?;
        Ok(UpdateStationInput {
            id: self.id,
            name: self.name.trim().to_owned(),
            station_type: self.station_type.into_string(),
            website_url: self.website_url.trim().to_owned(),
            api_base_url: self.api_base_url.trim().to_owned(),
            api_key: self.api_key,
            collector_proxy_mode: self.collector_proxy_mode.into_string(),
            collector_proxy_url: normalize_optional(self.collector_proxy_url),
            enabled: self.enabled,
            credit_per_cny: self.credit_per_cny,
            low_balance_threshold_cny: self.low_balance_threshold_cny,
            collection_interval_minutes: self.collection_interval_minutes,
            note: normalize_optional(self.note),
        })
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_station_id(&self.id)?;
        validate_station_fields(
            &self.name,
            &self.website_url,
            &self.api_base_url,
            self.api_key.as_deref().unwrap_or_default(),
            &self.collector_proxy_mode,
            self.collector_proxy_url.as_deref(),
            self.credit_per_cny,
            self.low_balance_threshold_cny,
            self.collection_interval_minutes,
            self.note.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeleteStationInputDto {
    pub id: String,
}

impl DeleteStationInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value, "The station delete payload is invalid.")?;
        validate_station_id(&input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReorderStationsInputDto {
    pub station_ids: Vec<String>,
}

impl ReorderStationsInputDto {
    pub fn parse(value: serde_json::Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value, "The station reorder payload is invalid.")?;
        if input.station_ids.len() > MAX_REORDER_STATIONS {
            return Err(invalid_input(
                "stationIds",
                "too_many_items",
                "The station order contains too many items.",
            ));
        }
        let mut unique = HashSet::with_capacity(input.station_ids.len());
        for id in &input.station_ids {
            validate_station_id(id)?;
            if !unique.insert(id.as_str()) {
                return Err(invalid_input(
                    "stationIds",
                    "duplicate_item",
                    "The station order contains a duplicate ID.",
                ));
            }
        }
        Ok(input)
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: serde_json::Value,
    message: &'static str,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| invalid_input("input", "invalid_shape", message))
}

#[allow(clippy::too_many_arguments)]
fn validate_station_fields(
    name: &str,
    website_url: &str,
    api_base_url: &str,
    api_key: &str,
    proxy_mode: &StationProxyModeDto,
    proxy_url: Option<&str>,
    credit_per_cny: f64,
    low_balance_threshold_cny: Option<f64>,
    collection_interval_minutes: u16,
    note: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    validate_bounded_text("name", name, MAX_STATION_NAME_BYTES, false)?;
    validate_http_url("websiteUrl", website_url)?;
    validate_http_url("apiBaseUrl", api_base_url)?;
    if api_key.len() > MAX_API_KEY_BYTES || api_key.chars().any(char::is_control) {
        return Err(invalid_input(
            "apiKey",
            "too_long",
            "The API key is too long.",
        ));
    }
    if let Some(value) = proxy_url {
        validate_proxy_url(value)?;
    }
    if matches!(proxy_mode, StationProxyModeDto::Manual)
        && proxy_url.is_none_or(|value| value.trim().is_empty())
    {
        return Err(invalid_input(
            "collectorProxyUrl",
            "required",
            "A manual proxy URL is required.",
        ));
    }
    if !credit_per_cny.is_finite() || !(f64::EPSILON..=MAX_CREDIT_PER_CNY).contains(&credit_per_cny)
    {
        return Err(invalid_input(
            "creditPerCny",
            "out_of_range",
            "The credit ratio is out of range.",
        ));
    }
    if low_balance_threshold_cny.is_some_and(|value| {
        !value.is_finite() || !(0.0..=MAX_BALANCE_THRESHOLD_CNY).contains(&value)
    }) {
        return Err(invalid_input(
            "lowBalanceThresholdCny",
            "out_of_range",
            "The balance threshold is out of range.",
        ));
    }
    if !(1..=MAX_COLLECTION_INTERVAL_MINUTES).contains(&collection_interval_minutes) {
        return Err(invalid_input(
            "collectionIntervalMinutes",
            "out_of_range",
            "The collection interval is out of range.",
        ));
    }
    if let Some(value) = note {
        validate_bounded_text("note", value, MAX_NOTE_BYTES, true)?;
    }
    Ok(())
}

fn validate_station_id(value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.is_empty()
        || value.len() > MAX_STATION_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
    {
        return Err(invalid_input(
            "id",
            "invalid_id",
            "The station ID is invalid.",
        ));
    }
    Ok(())
}

fn validate_bounded_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    let trimmed = value.trim();
    if (!allow_empty && trimmed.is_empty())
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
    {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The text value is invalid.",
        ));
    }
    Ok(())
}

fn validate_http_url(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    if value.len() > MAX_URL_BYTES {
        return Err(invalid_input(field, "too_long", "The URL is too long."));
    }
    let parsed = url::Url::parse(value.trim())
        .map_err(|_| invalid_input(field, "invalid_url", "The URL is invalid."))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err(invalid_input(
            field,
            "invalid_scheme",
            "The URL scheme is not supported.",
        ));
    }
    Ok(())
}

fn validate_proxy_url(value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.len() > MAX_URL_BYTES {
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

fn normalize_optional(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_owned())
        .filter(|item| !item.is_empty())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct StationDto {
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

impl From<Station> for StationDto {
    fn from(value: Station) -> Self {
        Self {
            id: value.id,
            name: value.name,
            station_type: value.station_type,
            website_url: value.website_url,
            api_base_url: value.api_base_url,
            endpoint_revision: value.endpoint_revision,
            collector_proxy_mode: value.collector_proxy_mode,
            collector_proxy_url: value.collector_proxy_url,
            api_key_masked: value.api_key_masked,
            api_key_present: value.api_key_present,
            key_count: value.key_count,
            enabled: value.enabled,
            priority: value.priority,
            credit_per_cny: value.credit_per_cny,
            balance_raw: value.balance_raw,
            balance_cny: value.balance_cny,
            low_balance_threshold_cny: value.low_balance_threshold_cny,
            collection_interval_minutes: value.collection_interval_minutes,
            status: value.status,
            latency_ms: value.latency_ms,
            last_checked_at: value.last_checked_at,
            last_pricing_fetched_at: value.last_pricing_fetched_at,
            note: value.note,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const STATION_TYPE: TypeDescriptor = TypeDescriptor {
    name: "StationDto",
    typescript: r#"export type StationTypeInput = "sub2api" | "newapi";

export type StationProxyModeInput = "inherit" | "direct" | "system" | "manual";

export type CreateStationInputDto = {
  name: string;
  stationType: StationTypeInput;
  websiteUrl: string;
  apiBaseUrl: string;
  apiKey: string;
  collectorProxyMode: StationProxyModeInput;
  collectorProxyUrl: string | null;
  enabled: boolean;
  creditPerCny: number;
  lowBalanceThresholdCny: number | null;
  collectionIntervalMinutes: number;
  note: string | null;
};

export type UpdateStationInputDto = Omit<CreateStationInputDto, "apiKey"> & {
  id: string;
  apiKey: string | null;
};

export type DeleteStationInputDto = { id: string };

export type ReorderStationsInputDto = { stationIds: string[] };

export type StationDto = {
  id: string;
  name: string;
  stationType: string;
  websiteUrl: string;
  apiBaseUrl: string;
  endpointRevision: number;
  collectorProxyMode: string;
  collectorProxyUrl: string | null;
  apiKeyMasked: string;
  apiKeyPresent: boolean;
  keyCount: number;
  enabled: boolean;
  priority: number;
  creditPerCny: number;
  balanceRaw: number | null;
  balanceCny: number | null;
  lowBalanceThresholdCny: number | null;
  collectionIntervalMinutes: number;
  status: string;
  latencyMs: number | null;
  lastCheckedAt: string | null;
  lastPricingFetchedAt: string | null;
  note: string | null;
  createdAt: string;
  updatedAt: string;
};"#,
};

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn fixture() -> StationDto {
    StationDto {
        id: "station-fixture".into(),
        name: "Fixture Station".into(),
        station_type: "newapi".into(),
        website_url: "https://provider.invalid".into(),
        api_base_url: "https://provider.invalid/v1".into(),
        endpoint_revision: 1,
        collector_proxy_mode: "inherit".into(),
        collector_proxy_url: None,
        api_key_masked: "sk-fixture-...redacted".into(),
        api_key_present: true,
        key_count: 1,
        enabled: true,
        priority: 0,
        credit_per_cny: 1.0,
        balance_raw: None,
        balance_cny: None,
        low_balance_threshold_cny: Some(15.0),
        collection_interval_minutes: 5,
        status: "unchecked".into(),
        latency_ms: None,
        last_checked_at: None,
        last_pricing_fetched_at: None,
        note: None,
        created_at: "2026-01-01T00:00:00Z".into(),
        updated_at: "2026-01-01T00:00:00Z".into(),
    }
}

#[cfg(test)]
mod input_contract_tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    fn valid_create() -> serde_json::Value {
        serde_json::json!({
            "name": "Provider",
            "stationType": "newapi",
            "websiteUrl": "https://provider.invalid",
            "apiBaseUrl": "https://provider.invalid/v1",
            "apiKey": "sk-fixture",
            "collectorProxyMode": "inherit",
            "collectorProxyUrl": null,
            "enabled": true,
            "creditPerCny": 1.0,
            "lowBalanceThresholdCny": 15.0,
            "collectionIntervalMinutes": 5,
            "note": null
        })
    }

    #[test]
    fn station_create_rejects_unknown_and_oversized_fields() {
        let mut unknown = valid_create();
        unknown["unexpected"] = serde_json::json!(true);
        assert_eq!(
            CreateStationInputDto::parse(unknown)
                .expect_err("unknown field")
                .code,
            CommandErrorCode::InvalidInput
        );

        let mut oversized = valid_create();
        oversized["name"] = serde_json::json!("x".repeat(MAX_STATION_NAME_BYTES + 1));
        assert_eq!(
            CreateStationInputDto::parse(oversized)
                .expect_err("oversized name")
                .code,
            CommandErrorCode::InvalidInput
        );
    }

    #[test]
    fn station_create_rejects_invalid_url_enum_and_numeric_shapes() {
        for (field, value) in [
            ("stationType", serde_json::json!("unknown")),
            ("websiteUrl", serde_json::json!("file:///private")),
            ("apiBaseUrl", serde_json::json!("not a url")),
            ("collectorProxyMode", serde_json::json!("random")),
            ("creditPerCny", serde_json::json!(0)),
            ("collectionIntervalMinutes", serde_json::json!(0)),
        ] {
            let mut input = valid_create();
            input[field] = value;
            let error = CreateStationInputDto::parse(input).expect_err(field);
            assert_eq!(error.code, CommandErrorCode::InvalidInput, "{field}");
        }
    }

    #[test]
    fn station_update_delete_and_reorder_reject_malformed_ids() {
        let mut update = valid_create();
        update["id"] = serde_json::json!("bad id with spaces");
        update["apiKey"] = serde_json::Value::Null;
        assert_eq!(
            UpdateStationInputDto::parse(update)
                .expect_err("invalid update id")
                .code,
            CommandErrorCode::InvalidInput
        );
        assert_eq!(
            DeleteStationInputDto::parse(serde_json::json!({"id": "../station"}))
                .expect_err("invalid delete id")
                .code,
            CommandErrorCode::InvalidInput
        );
        assert_eq!(
            ReorderStationsInputDto::parse(serde_json::json!({
                "stationIds": ["station-1", "station-1"]
            }))
            .expect_err("duplicate ids")
            .code,
            CommandErrorCode::InvalidInput
        );
    }

    #[test]
    fn station_transport_inputs_convert_one_way_to_domain_inputs() {
        let create = CreateStationInputDto::parse(valid_create()).expect("valid create");
        let domain = create.into_domain().expect("domain create");
        assert_eq!(domain.station_type, "newapi");
        assert_eq!(domain.name, "Provider");
    }
}
