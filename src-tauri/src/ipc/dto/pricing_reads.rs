use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::pricing::PricingStatus;
use crate::models::{
    pricing::{ModelBasePrice, RequestKind, ResolvedPricingContext},
    pricing_group_monitoring::{
        canonicalize_group_refs, group_refs_hash, CanonicalGroupRef,
        PricingGroupMonitorStatusInput, PricingGroupMonitorStatusWorkspace,
        PRICING_GROUP_MONITORING_SCHEMA_VERSION,
    },
    shared_capabilities::PricingComparisonWorkspace,
};

use super::{
    collector_facts::{GroupRateRecordDto, StationGroupBindingDto},
    invalid_input,
    station_keys::StationKeyDto,
    stations::StationDto,
    TypeDescriptor,
};

const MAX_STATION_KEY_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 512;

pub type ModelBasePriceDto = ModelBasePrice;
pub type ResolvedPricingContextDto = ResolvedPricingContext;
pub type PricingGroupMonitorStatusWorkspaceDto = PricingGroupMonitorStatusWorkspace;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceSyncStateDto {
    pub source_url: String,
    pub auto_sync_enabled: bool,
    pub include_common_models: bool,
    pub selected_model_keys: Vec<String>,
    pub excluded_common_model_keys: Vec<String>,
    pub last_sync_at: Option<String>,
    pub last_sync_error: Option<String>,
    pub model_count: usize,
    pub common_model_count: usize,
    pub auto_sync_model_count: usize,
    pub file_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceCatalogEntryDto {
    pub key: String,
    pub provider: String,
    pub model: String,
    pub name: String,
    pub common: bool,
    pub release_date: Option<String>,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub cache_creation_price: Option<f64>,
    pub cache_read_price: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceSyncResultDto {
    pub state: ModelPriceSyncStateDto,
    pub imported_count: usize,
    pub skipped_count: usize,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingGroupMonitorStatusInputDto {
    pub schema_version: u32,
    pub group_refs_hash: String,
    pub groups: Vec<PricingGroupRefDto>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingGroupRefDto {
    pub station_id: String,
    pub group_binding_id: Option<String>,
    pub group_id_hash: Option<String>,
    pub group_key_hash: String,
}

impl PricingGroupMonitorStatusInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The pricing group monitoring payload is invalid.",
            )
        })?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> PricingGroupMonitorStatusInput {
        PricingGroupMonitorStatusInput {
            schema_version: self.schema_version,
            group_refs_hash: self.group_refs_hash,
            groups: self
                .groups
                .into_iter()
                .map(|group| CanonicalGroupRef {
                    station_id: group.station_id,
                    group_binding_id: group.group_binding_id,
                    group_id_hash: group.group_id_hash,
                    group_key_hash: group.group_key_hash,
                })
                .collect(),
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        let groups = self
            .groups
            .iter()
            .map(|group| CanonicalGroupRef {
                station_id: group.station_id.clone(),
                group_binding_id: group.group_binding_id.clone(),
                group_id_hash: group.group_id_hash.clone(),
                group_key_hash: group.group_key_hash.clone(),
            })
            .collect::<Vec<_>>();
        canonicalize_group_refs(&groups).map_err(|_| {
            invalid_input(
                "groups",
                "invalid_refs",
                "The group references are invalid, unresolved, duplicated, or too large.",
            )
        })?;
        let expected_hash = group_refs_hash(&groups).map_err(|_| {
            invalid_input(
                "groupRefsHash",
                "invalid_hash",
                "The group reference hash is invalid.",
            )
        })?;
        if self.schema_version != PRICING_GROUP_MONITORING_SCHEMA_VERSION
            || self.group_refs_hash != expected_hash
        {
            return Err(invalid_input(
                "groupRefsHash",
                "hash_mismatch",
                "The group reference hash does not match the normalized references.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingContextInputDto {
    pub station_key_id: String,
    pub requested_model: String,
    pub request_kind: RequestKind,
}

impl PricingContextInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The pricing context payload is invalid.",
            )
        })?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_parts(self) -> (String, String, RequestKind) {
        (
            self.station_key_id,
            self.requested_model.trim().to_owned(),
            self.request_kind,
        )
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        let valid_id = !self.station_key_id.is_empty()
            && self.station_key_id.len() <= MAX_STATION_KEY_ID_BYTES
            && self.station_key_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            });
        if !valid_id {
            return Err(invalid_input(
                "stationKeyId",
                "invalid_id",
                "The station key ID is invalid.",
            ));
        }

        if self.requested_model.trim().is_empty()
            || self.requested_model.len() > MAX_MODEL_BYTES
            || self.requested_model.chars().any(char::is_control)
        {
            return Err(invalid_input(
                "requestedModel",
                "invalid_text",
                "The requested model is invalid.",
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PricingComparisonWorkspaceDto {
    pub stations: Vec<StationDto>,
    pub station_keys: Vec<StationKeyDto>,
    pub group_bindings: Vec<StationGroupBindingDto>,
    pub group_rates: Vec<GroupRateRecordDto>,
    pub developer_mode_enabled: bool,
}

impl From<PricingComparisonWorkspace> for PricingComparisonWorkspaceDto {
    fn from(value: PricingComparisonWorkspace) -> Self {
        Self {
            stations: value.stations.into_iter().map(StationDto::from).collect(),
            station_keys: value.station_keys,
            group_bindings: value.group_bindings,
            group_rates: value.group_rates,
            developer_mode_enabled: value.developer_mode_enabled,
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
pub const PRICING_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "PricingReadsDto",
    typescript: include_str!("pricing_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let base_price = fixture_base_price();
    let catalog_entry = ModelPriceCatalogEntryDto {
        key: "openai/gpt-fixture".into(),
        provider: "openai".into(),
        model: "gpt-fixture".into(),
        name: "GPT Fixture".into(),
        common: true,
        release_date: Some("2026-08-24".into()),
        input_price: Some(1.0),
        output_price: Some(2.0),
        cache_creation_price: Some(0.5),
        cache_read_price: Some(0.1),
    };
    let context = fixture_context();
    let workspace = PricingComparisonWorkspaceDto {
        stations: Vec::new(),
        station_keys: Vec::new(),
        group_bindings: Vec::new(),
        group_rates: Vec::new(),
        developer_mode_enabled: false,
    };
    vec![
        serde_json::json!({"command":"list_model_base_prices","input":{},"output":[base_price]}),
        serde_json::json!({"command":"list_model_price_sync_catalog","input":{},"output":[catalog_entry]}),
        serde_json::json!({
            "command":"resolve_station_key_pricing_context",
            "input":{"stationKeyId":"key-1","requestedModel":"fixture-model","requestKind":"text"},
            "output":context
        }),
        serde_json::json!({"command":"load_pricing_comparison_workspace","input":{},"output":workspace}),
        serde_json::json!({
            "command":"load_pricing_group_monitor_status",
            "input":{"schemaVersion":PRICING_GROUP_MONITORING_SCHEMA_VERSION,"groupRefsHash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855","groups":[]},
            "output":{
                "schemaVersion":PRICING_GROUP_MONITORING_SCHEMA_VERSION,
                "generatedAtMs":1700000000000i64,
                "groupRefsHash":"e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
                "requestedGroupCount":0,
                "returnedGroupCount":0,
                "omittedGroupCount":0,
                "items":[]
            }
        }),
    ]
}

#[cfg(test)]
fn fixture_base_price() -> ModelBasePriceDto {
    ModelBasePrice {
        id: "base-price-1".into(),
        provider: "fixture".into(),
        model: "fixture-model".into(),
        input_price: Some(1.0),
        output_price: Some(2.0),
        input_price_priority: None,
        output_price_priority: None,
        cache_creation_price: Some(1.25),
        cache_creation_price_priority: None,
        cache_creation_price_above_1hr: None,
        cache_read_price: Some(0.1),
        cache_read_price_priority: None,
        long_context_input_token_threshold: None,
        long_context_input_cost_multiplier: None,
        long_context_output_cost_multiplier: None,
        supports_service_tier: false,
        supports_prompt_caching: true,
        currency: "USD".into(),
        unit: "M".into(),
        source_url: "https://provider.invalid/pricing".into(),
        source_label: "Fixture pricing".into(),
        source_checked_at: Some("2026-07-24".into()),
        enabled: true,
        built_in: false,
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_context() -> ResolvedPricingContextDto {
    ResolvedPricingContext {
        station_key_id: "key-1".into(),
        station_id: "station-1".into(),
        requested_model: "fixture-model".into(),
        resolved_model: "fixture-model".into(),
        request_kind: RequestKind::Text,
        group_binding_id: None,
        base_input_price: Some(1.0),
        base_output_price: Some(2.0),
        base_cache_creation_price: Some(1.25),
        base_cache_read_price: Some(0.1),
        currency: "USD".into(),
        unit: "M".into(),
        base_price_source: Some("fixture".into()),
        effective_rate_multiplier: Some(0.8),
        rate_source: Some("fixture".into()),
        rate_collected_at: Some("1700000000000".into()),
        estimated_input_price: Some(0.8),
        estimated_output_price: Some(1.6),
        estimated_cache_creation_price: Some(1.0),
        estimated_cache_read_price: Some(0.08),
        pricing_status: PricingStatus::Priced,
        confidence: 1.0,
        source_chain: vec!["fixture".into()],
        reason: None,
        resolved_at: "1700000000000".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    fn monitor_input(group: Value) -> Value {
        serde_json::json!({
            "schemaVersion": PRICING_GROUP_MONITORING_SCHEMA_VERSION,
            "groupRefsHash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            "groups": group
        })
    }

    fn valid_input() -> Value {
        serde_json::json!({
            "stationKeyId":"key-1",
            "requestedModel":"fixture-model",
            "requestKind":"text"
        })
    }

    #[test]
    fn pricing_context_rejects_unknown_fields_invalid_ids_and_open_enums() {
        let mut unknown = valid_input();
        unknown["unexpected"] = serde_json::json!(true);
        let mut invalid_id = valid_input();
        invalid_id["stationKeyId"] = serde_json::json!("bad id");
        let mut invalid_kind = valid_input();
        invalid_kind["requestKind"] = serde_json::json!("audio");

        for value in [unknown, invalid_id, invalid_kind] {
            let error = PricingContextInputDto::parse(value).expect_err("invalid pricing input");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn pricing_context_rejects_empty_oversized_and_control_character_models() {
        for model in [
            "   ".to_owned(),
            "x".repeat(MAX_MODEL_BYTES + 1),
            "fixture\nmodel".to_owned(),
        ] {
            let mut value = valid_input();
            value["requestedModel"] = serde_json::json!(model);
            let error = PricingContextInputDto::parse(value).expect_err("invalid model");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn pricing_monitor_status_rejects_unknown_fields_duplicates_and_hash_mismatch() {
        let valid = monitor_input(serde_json::json!([]));
        PricingGroupMonitorStatusInputDto::parse(valid).expect("empty monitor input");

        let mut unknown = monitor_input(serde_json::json!([]));
        unknown["unexpected"] = serde_json::json!(true);
        let error = PricingGroupMonitorStatusInputDto::parse(unknown)
            .expect_err("unknown monitor field must be rejected");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);

        let duplicate = monitor_input(serde_json::json!([
            {
                "stationId": "station-1",
                "groupBindingId": "binding-1",
                "groupIdHash": null,
                "groupKeyHash": "ignored"
            },
            {
                "stationId": "station-1",
                "groupBindingId": "binding-1",
                "groupIdHash": null,
                "groupKeyHash": "ignored"
            }
        ]));
        let error = PricingGroupMonitorStatusInputDto::parse(duplicate)
            .expect_err("duplicate monitor refs must be rejected");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);

        let mut mismatch = monitor_input(serde_json::json!([]));
        mismatch["groupRefsHash"] = serde_json::json!("not-a-sha256");
        let error = PricingGroupMonitorStatusInputDto::parse(mismatch)
            .expect_err("hash mismatch must be rejected");
        assert_eq!(error.code, CommandErrorCode::InvalidInput);
    }

    #[test]
    fn pricing_monitor_status_serialization_contains_summary_only_fields() {
        let fixture = serialization_fixtures()
            .into_iter()
            .find(|value| value["command"] == "load_pricing_group_monitor_status")
            .expect("monitor serialization fixture");
        let encoded = serde_json::to_string(&fixture).expect("serialize fixture");
        for secret in ["apiKey", "cookie", "authorization", "token", "responseBody"] {
            assert!(!encoded
                .to_ascii_lowercase()
                .contains(&secret.to_ascii_lowercase()));
        }
        assert_eq!(fixture["output"]["omittedGroupCount"], 0);
    }
}
