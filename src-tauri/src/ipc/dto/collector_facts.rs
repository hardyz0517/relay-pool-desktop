use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{
    collector::CollectorSnapshot,
    collector_runs::CollectorRun,
    group_facts::{GroupRateRecord, StationGroupBinding, UpsertStationGroupBindingInput},
    pricing::{BalanceSnapshot, UpsertBalanceSnapshotInput},
    shared_capabilities::StationGroupOption,
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_TEXT_BYTES: usize = 512;
const MAX_JSON_BYTES: usize = 65_536;
const MAX_ABSOLUTE_VALUE: f64 = 1.0e18;
const MAX_RATE_MULTIPLIER: f64 = 1.0e6;
const MAX_STATION_ID_LIST: usize = 500;

pub type BalanceSnapshotDto = BalanceSnapshot;
pub type CollectorRunDto = CollectorRun;
pub type CollectorSnapshotDto = CollectorSnapshot;
pub type GroupRateRecordDto = GroupRateRecord;
pub type StationGroupBindingDto = StationGroupBinding;
pub type StationGroupOptionDto = StationGroupOption;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectorStationIdInputDto {
    pub station_id: String,
}

impl CollectorStationIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectorStationIdsInputDto {
    pub station_ids: Vec<String>,
}

impl CollectorStationIdsInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if input.station_ids.len() > MAX_STATION_ID_LIST {
            return Err(invalid_input(
                "stationIds",
                "too_many",
                "The station ID list exceeds the allowed size.",
            ));
        }
        let mut unique = HashSet::with_capacity(input.station_ids.len());
        for station_id in &input.station_ids {
            validate_id("stationIds", station_id)?;
            if !unique.insert(station_id) {
                return Err(invalid_input(
                    "stationIds",
                    "duplicate",
                    "The station ID list must not contain duplicates.",
                ));
            }
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceScopeDto {
    Station,
    StationKey,
}

impl BalanceScopeDto {
    fn into_string(self) -> String {
        serialize_string_enum(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BalanceStatusDto {
    Unknown,
    Normal,
    Low,
    Depleted,
}

impl BalanceStatusDto {
    fn into_string(self) -> String {
        serialize_string_enum(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertBalanceSnapshotInputDto {
    pub id: Option<String>,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: BalanceScopeDto,
    pub value: Option<f64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub today_request_count: Option<i64>,
    pub total_request_count: Option<i64>,
    pub today_consumption: Option<f64>,
    pub total_consumption: Option<f64>,
    pub today_base_consumption: Option<f64>,
    pub total_base_consumption: Option<f64>,
    pub today_token_count: Option<i64>,
    pub total_token_count: Option<i64>,
    pub today_input_token_count: Option<i64>,
    pub today_output_token_count: Option<i64>,
    pub total_input_token_count: Option<i64>,
    pub total_output_token_count: Option<i64>,
    pub account_concurrency_limit: Option<i64>,
    pub low_balance_threshold: Option<f64>,
    pub status: BalanceStatusDto,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
}

impl UpsertBalanceSnapshotInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_optional_id("id", input.id.as_deref())?;
        validate_id("stationId", &input.station_id)?;
        validate_optional_id("stationKeyId", input.station_key_id.as_deref())?;
        validate_text("currency", &input.currency, 16, false)?;
        validate_optional_text("creditUnit", input.credit_unit.as_deref(), MAX_TEXT_BYTES)?;
        for (field, value) in [
            ("value", input.value),
            ("usedValue", input.used_value),
            ("totalValue", input.total_value),
            ("todayConsumption", input.today_consumption),
            ("totalConsumption", input.total_consumption),
            ("todayBaseConsumption", input.today_base_consumption),
            ("totalBaseConsumption", input.total_base_consumption),
            ("lowBalanceThreshold", input.low_balance_threshold),
        ] {
            validate_optional_finite(field, value, MAX_ABSOLUTE_VALUE)?;
        }
        for (field, value) in [
            ("todayRequestCount", input.today_request_count),
            ("totalRequestCount", input.total_request_count),
            ("todayTokenCount", input.today_token_count),
            ("totalTokenCount", input.total_token_count),
            ("todayInputTokenCount", input.today_input_token_count),
            ("todayOutputTokenCount", input.today_output_token_count),
            ("totalInputTokenCount", input.total_input_token_count),
            ("totalOutputTokenCount", input.total_output_token_count),
            ("accountConcurrencyLimit", input.account_concurrency_limit),
        ] {
            validate_optional_non_negative(field, value)?;
        }
        validate_text("source", &input.source, MAX_TEXT_BYTES, false)?;
        validate_probability("confidence", input.confidence)?;
        validate_optional_text("collectedAt", input.collected_at.as_deref(), MAX_TEXT_BYTES)?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpsertBalanceSnapshotInput {
        UpsertBalanceSnapshotInput {
            id: self.id,
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            scope: self.scope.into_string(),
            value: self.value,
            currency: self.currency,
            credit_unit: self.credit_unit,
            used_value: self.used_value,
            total_value: self.total_value,
            today_request_count: self.today_request_count,
            total_request_count: self.total_request_count,
            today_consumption: self.today_consumption,
            total_consumption: self.total_consumption,
            today_base_consumption: self.today_base_consumption,
            total_base_consumption: self.total_base_consumption,
            today_token_count: self.today_token_count,
            total_token_count: self.total_token_count,
            today_input_token_count: self.today_input_token_count,
            today_output_token_count: self.today_output_token_count,
            total_input_token_count: self.total_input_token_count,
            total_output_token_count: self.total_output_token_count,
            account_concurrency_limit: self.account_concurrency_limit,
            low_balance_threshold: self.low_balance_threshold,
            status: self.status.into_string(),
            source: self.source,
            confidence: self.confidence,
            collected_at: self.collected_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingKindDto {
    StationGroup,
    KeyBinding,
}

impl BindingKindDto {
    fn into_string(self) -> String {
        serialize_string_enum(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BindingStatusDto {
    Available,
    Bound,
    Missing,
    Disabled,
    ManualLegacy,
}

impl BindingStatusDto {
    fn into_string(self) -> String {
        serialize_string_enum(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationGroupCategoryDto {
    Gpt,
    Claude,
    Gemini,
    Grok,
    ImageGeneration,
    Embedding,
    Rerank,
    Unknown,
}

impl StationGroupCategoryDto {
    fn into_string(self) -> String {
        serialize_string_enum(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertStationGroupBindingInputDto {
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub binding_kind: BindingKindDto,
    pub parent_group_binding_id: Option<String>,
    pub group_key_hash: String,
    pub group_id_hash: Option<String>,
    pub group_name: String,
    pub binding_status: BindingStatusDto,
    pub default_rate_multiplier: Option<f64>,
    pub user_rate_multiplier: Option<f64>,
    pub effective_rate_multiplier: Option<f64>,
    pub inferred_group_category: Option<StationGroupCategoryDto>,
    pub group_category_override: Option<StationGroupCategoryDto>,
    pub rate_source: Option<String>,
    pub confidence: f64,
    pub last_seen_at: Option<String>,
    pub raw_json_redacted: Option<Value>,
}

impl UpsertStationGroupBindingInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        validate_optional_id("stationKeyId", input.station_key_id.as_deref())?;
        validate_optional_id(
            "parentGroupBindingId",
            input.parent_group_binding_id.as_deref(),
        )?;
        validate_id("groupKeyHash", &input.group_key_hash)?;
        validate_optional_text(
            "groupIdHash",
            input.group_id_hash.as_deref(),
            MAX_TEXT_BYTES,
        )?;
        validate_text("groupName", &input.group_name, MAX_TEXT_BYTES, false)?;
        for (field, value) in [
            ("defaultRateMultiplier", input.default_rate_multiplier),
            ("userRateMultiplier", input.user_rate_multiplier),
            ("effectiveRateMultiplier", input.effective_rate_multiplier),
        ] {
            validate_optional_finite(field, value, MAX_RATE_MULTIPLIER)?;
            if value.is_some_and(|value| value < 0.0) {
                return Err(invalid_number(field));
            }
        }
        validate_optional_text("rateSource", input.rate_source.as_deref(), MAX_TEXT_BYTES)?;
        validate_probability("confidence", input.confidence)?;
        validate_optional_text("lastSeenAt", input.last_seen_at.as_deref(), MAX_TEXT_BYTES)?;
        validate_optional_json("rawJsonRedacted", input.raw_json_redacted.as_ref())?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpsertStationGroupBindingInput {
        UpsertStationGroupBindingInput {
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            binding_kind: self.binding_kind.into_string(),
            parent_group_binding_id: self.parent_group_binding_id,
            group_key_hash: self.group_key_hash,
            group_id_hash: self.group_id_hash,
            group_name: self.group_name,
            binding_status: self.binding_status.into_string(),
            default_rate_multiplier: self.default_rate_multiplier,
            user_rate_multiplier: self.user_rate_multiplier,
            effective_rate_multiplier: self.effective_rate_multiplier,
            inferred_group_category: self
                .inferred_group_category
                .map(StationGroupCategoryDto::into_string),
            group_category_override: self
                .group_category_override
                .map(StationGroupCategoryDto::into_string),
            rate_source: self.rate_source,
            confidence: self.confidence,
            last_seen_at: self.last_seen_at,
            raw_json_redacted: self.raw_json_redacted,
        }
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The collector facts payload is invalid.",
        )
    })
}

fn serialize_string_enum<T: Serialize>(value: T) -> String {
    serde_json::to_value(value)
        .expect("string enum serializes")
        .as_str()
        .expect("string enum serializes as a string")
        .to_owned()
}

fn validate_id(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The identifier is invalid.",
        ));
    }
    Ok(())
}

fn validate_optional_id(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_id(field, value)?;
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max
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

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_text(field, value, max, false)?;
    }
    Ok(())
}

fn validate_optional_finite(
    field: &'static str,
    value: Option<f64>,
    max: f64,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| !value.is_finite() || value.abs() > max) {
        return Err(invalid_number(field));
    }
    Ok(())
}

fn validate_optional_non_negative(
    field: &'static str,
    value: Option<i64>,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| value < 0) {
        return Err(invalid_number(field));
    }
    Ok(())
}

fn validate_probability(
    field: &'static str,
    value: f64,
) -> Result<(), crate::commands::error::CommandError> {
    if !value.is_finite() || !(0.0..=1.0).contains(&value) {
        return Err(invalid_number(field));
    }
    Ok(())
}

fn invalid_number(field: &'static str) -> crate::commands::error::CommandError {
    invalid_input(
        field,
        "invalid_number",
        "The numeric value is outside the allowed range.",
    )
}

fn validate_optional_json(
    field: &'static str,
    value: Option<&Value>,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| !value.is_object()) {
        return Err(invalid_input(
            field,
            "invalid_shape",
            "The JSON value must be an object.",
        ));
    }
    if value.is_some_and(|value| value.to_string().len() > MAX_JSON_BYTES) {
        return Err(invalid_input(
            field,
            "payload_too_large",
            "The JSON value exceeds the allowed size.",
        ));
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub const COLLECTOR_FACTS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "CollectorFactsDto",
    typescript: include_str!("collector_facts.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let balance = fixture_balance_snapshot();
    let balance_input = UpsertBalanceSnapshotInputDto::parse(serde_json::json!({
        "id": null, "stationId": "station-1", "stationKeyId": null,
        "scope": "station", "value": 12.5, "currency": "CNY", "creditUnit": null,
        "usedValue": null, "totalValue": null, "todayRequestCount": null,
        "totalRequestCount": null, "todayConsumption": null, "totalConsumption": null,
        "todayBaseConsumption": null, "totalBaseConsumption": null, "todayTokenCount": null,
        "totalTokenCount": null, "todayInputTokenCount": null, "todayOutputTokenCount": null,
        "totalInputTokenCount": null, "totalOutputTokenCount": null,
        "accountConcurrencyLimit": null, "lowBalanceThreshold": 5.0,
        "status": "normal", "source": "fixture", "confidence": 0.9,
        "collectedAt": "1700000000000"
    }))
    .expect("balance fixture input");
    let binding = fixture_station_group_binding();
    let binding_input = UpsertStationGroupBindingInputDto::parse(serde_json::json!({
        "stationId": "station-1", "stationKeyId": null, "bindingKind": "station_group",
        "parentGroupBindingId": null, "groupKeyHash": "group-hash-1",
        "groupIdHash": "group-id-hash-1", "groupName": "default",
        "bindingStatus": "available", "defaultRateMultiplier": 1.0,
        "userRateMultiplier": null, "effectiveRateMultiplier": 1.0,
        "inferredGroupCategory": "gpt", "groupCategoryOverride": null,
        "rateSource": "fixture", "confidence": 0.9, "lastSeenAt": "1700000000000",
        "rawJsonRedacted": null
    }))
    .expect("binding fixture input");
    let station_input = serde_json::json!({"stationId": "station-1"});
    let station_ids_input = serde_json::json!({"stationIds": ["station-1"]});
    vec![
        serde_json::json!({"command":"list_balance_snapshots","input":{},"output":[balance.clone()]}),
        serde_json::json!({"command":"list_current_station_balance_snapshots","input":{},"output":[balance.clone()]}),
        serde_json::json!({"command":"list_balance_snapshots_for_station","input":station_input.clone(),"output":[balance.clone()]}),
        serde_json::json!({"command":"upsert_balance_snapshot","input":balance_input,"output":balance}),
        serde_json::json!({"command":"list_station_group_bindings","input":station_input.clone(),"output":[binding.clone()]}),
        serde_json::json!({"command":"list_station_group_options","input":station_input.clone(),"output":[fixture_station_group_option()]}),
        serde_json::json!({"command":"upsert_station_group_binding","input":binding_input,"output":binding}),
        serde_json::json!({"command":"list_group_rate_records","input":station_input.clone(),"output":[fixture_group_rate_record()]}),
        serde_json::json!({"command":"list_collector_runs","input":station_input.clone(),"output":[fixture_collector_run()]}),
        serde_json::json!({"command":"list_collector_snapshots","input":station_input.clone(),"output":[fixture_collector_snapshot()]}),
        serde_json::json!({"command":"get_latest_collector_snapshot","input":station_input,"output":fixture_collector_snapshot()}),
        serde_json::json!({"command":"list_latest_collector_snapshots","input":station_ids_input,"output":[fixture_collector_snapshot()]}),
    ]
}

#[cfg(test)]
fn fixture_balance_snapshot() -> BalanceSnapshot {
    BalanceSnapshot {
        id: "balance-1".into(),
        station_id: "station-1".into(),
        station_key_id: None,
        scope: "station".into(),
        value: Some(12.5),
        currency: "CNY".into(),
        credit_unit: None,
        used_value: None,
        total_value: None,
        today_request_count: None,
        total_request_count: None,
        today_consumption: None,
        total_consumption: None,
        today_base_consumption: None,
        total_base_consumption: None,
        today_token_count: None,
        total_token_count: None,
        today_input_token_count: None,
        today_output_token_count: None,
        total_input_token_count: None,
        total_output_token_count: None,
        account_concurrency_limit: None,
        low_balance_threshold: Some(5.0),
        status: "normal".into(),
        source: "fixture".into(),
        confidence: 0.9,
        collected_at: Some("1700000000000".into()),
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_station_group_binding() -> StationGroupBinding {
    StationGroupBinding {
        id: "binding-1".into(),
        station_id: "station-1".into(),
        station_key_id: None,
        binding_kind: "station_group".into(),
        parent_group_binding_id: None,
        group_key_hash: "group-hash-1".into(),
        group_id_hash: Some("group-id-hash-1".into()),
        group_name: "default".into(),
        binding_status: "available".into(),
        default_rate_multiplier: Some(1.0),
        user_rate_multiplier: None,
        effective_rate_multiplier: Some(1.0),
        inferred_group_category: Some("gpt".into()),
        group_category_override: None,
        rate_source: Some("fixture".into()),
        confidence: 0.9,
        last_seen_at: Some("1700000000000".into()),
        last_checked_at: Some("1700000000000".into()),
        last_rate_changed_at: None,
        raw_json_redacted: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_station_group_option() -> StationGroupOption {
    StationGroupOption {
        value: "binding:binding-1".into(),
        group_binding_id: Some("binding-1".into()),
        group_id_hash: Some("group-id-hash-1".into()),
        group_name: "default".into(),
        rate_multiplier: Some(1.0),
        inferred_group_category: Some("gpt".into()),
        group_category_override: None,
        effective_group_category: "gpt".into(),
        rate_source: Some("fixture".into()),
        selectable_for_remote_key: true,
    }
}

#[cfg(test)]
fn fixture_group_rate_record() -> GroupRateRecord {
    GroupRateRecord {
        id: "rate-1".into(),
        station_id: "station-1".into(),
        station_key_id: None,
        group_binding_id: Some("binding-1".into()),
        binding_kind: "station_group".into(),
        group_key_hash: "group-hash-1".into(),
        group_name: "default".into(),
        default_rate_multiplier: Some(1.0),
        user_rate_multiplier: None,
        effective_rate_multiplier: Some(1.0),
        inferred_group_category: Some("gpt".into()),
        source: "fixture".into(),
        confidence: 0.9,
        raw_json_redacted: None,
        checked_at: "1700000000000".into(),
        created_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_collector_run() -> CollectorRun {
    CollectorRun {
        id: "run-1".into(),
        station_id: "station-1".into(),
        endpoint_revision: 1,
        parent_run_id: None,
        adapter: "fixture".into(),
        task_type: "full".into(),
        status: "success".into(),
        started_at: "1700000000000".into(),
        finished_at: Some("1700000000100".into()),
        duration_ms: Some(100),
        endpoint_count: 1,
        success_count: 1,
        failure_count: 0,
        manual_action_required: false,
        error_code: None,
        error_message: None,
        snapshot_id: Some("snapshot-1".into()),
        created_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_collector_snapshot() -> CollectorSnapshot {
    CollectorSnapshot {
        id: "snapshot-1".into(),
        station_id: "station-1".into(),
        endpoint_revision: 1,
        source: "fixture".into(),
        status: "success".into(),
        fetched_at: "1700000000000".into(),
        summary_json: serde_json::json!({"status":"success"}),
        normalized_json: serde_json::json!({}),
        raw_json_redacted: None,
        error_message: None,
        created_at: "1700000000000".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn upsert_station_group_binding_accepts_remote_group_id_hash_text() {
        let input = UpsertStationGroupBindingInputDto::parse(serde_json::json!({
            "stationId": "station-1",
            "stationKeyId": null,
            "bindingKind": "station_group",
            "parentGroupBindingId": null,
            "groupKeyHash": "12cb0adf1fef391169d565e9083187ad32c6b083e260bfaffa9ce0699c9a84ad",
            "groupIdHash": "gpt特价（限时）",
            "groupName": "gpt特价（限时）",
            "bindingStatus": "available",
            "defaultRateMultiplier": 1.0,
            "userRateMultiplier": null,
            "effectiveRateMultiplier": 1.0,
            "inferredGroupCategory": "gpt",
            "groupCategoryOverride": null,
            "rateSource": "remote_scan",
            "confidence": 0.95,
            "lastSeenAt": "2026-07-31T03:00:00.000Z",
            "rawJsonRedacted": null
        }))
        .expect("remote group identity hash may be provider text");

        assert_eq!(input.group_id_hash.as_deref(), Some("gpt特价（限时）"));
    }

    #[test]
    fn rejects_unknown_fields_invalid_ids_enums_numbers_and_oversized_json() {
        for value in [
            serde_json::json!({"stationId":"station-1","unexpected":true}),
            serde_json::json!({"stationId":"bad id"}),
            serde_json::json!({"stationIds":["station-1","station-1"]}),
            serde_json::json!({"stationIds":["bad id"]}),
        ] {
            let error = if value.get("stationIds").is_some() {
                CollectorStationIdsInputDto::parse(value).expect_err("invalid station input")
            } else {
                CollectorStationIdInputDto::parse(value).expect_err("invalid station input")
            };
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }

        let mut balance = serde_json::to_value(
            UpsertBalanceSnapshotInputDto::parse(serde_json::json!({
                "id": null, "stationId": "station-1", "stationKeyId": null,
                "scope": "station", "value": null, "currency": "CNY", "creditUnit": null,
                "usedValue": null, "totalValue": null, "todayRequestCount": null,
                "totalRequestCount": null, "todayConsumption": null, "totalConsumption": null,
                "todayBaseConsumption": null, "totalBaseConsumption": null, "todayTokenCount": null,
                "totalTokenCount": null, "todayInputTokenCount": null, "todayOutputTokenCount": null,
                "totalInputTokenCount": null, "totalOutputTokenCount": null,
                "accountConcurrencyLimit": null, "lowBalanceThreshold": null,
                "status": "unknown", "source": "fixture", "confidence": 0.5, "collectedAt": null
            })).expect("valid balance")
        ).expect("serialize balance");
        balance["scope"] = serde_json::json!("account");
        assert_eq!(
            UpsertBalanceSnapshotInputDto::parse(balance)
                .expect_err("scope")
                .code,
            CommandErrorCode::InvalidInput
        );

        let mut binding = serde_json::json!({
            "stationId": "station-1", "stationKeyId": null, "bindingKind": "station_group",
            "parentGroupBindingId": null, "groupKeyHash": "group-hash-1", "groupIdHash": null,
            "groupName": "default", "bindingStatus": "available", "defaultRateMultiplier": null,
            "userRateMultiplier": null, "effectiveRateMultiplier": null,
            "inferredGroupCategory": null, "groupCategoryOverride": null, "rateSource": null,
            "confidence": 0.5, "lastSeenAt": null, "rawJsonRedacted": null
        });
        binding["confidence"] = serde_json::json!(2.0);
        assert_eq!(
            UpsertStationGroupBindingInputDto::parse(binding.clone())
                .expect_err("confidence")
                .code,
            CommandErrorCode::InvalidInput
        );
        binding["confidence"] = serde_json::json!(0.5);
        binding["rawJsonRedacted"] = serde_json::json!([]);
        assert_eq!(
            UpsertStationGroupBindingInputDto::parse(binding.clone())
                .expect_err("json shape")
                .code,
            CommandErrorCode::InvalidInput
        );
        binding["rawJsonRedacted"] = serde_json::json!({"value":"x".repeat(MAX_JSON_BYTES)});
        assert_eq!(
            UpsertStationGroupBindingInputDto::parse(binding)
                .expect_err("json size")
                .code,
            CommandErrorCode::InvalidInput
        );
    }
}
