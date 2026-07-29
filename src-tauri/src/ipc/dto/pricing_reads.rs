use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::pricing::PricingStatus;
use crate::models::{
    pricing::{ModelBasePrice, PricingRule, RequestKind, ResolvedPricingContext},
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

pub type PricingRuleDto = PricingRule;
pub type ModelBasePriceDto = ModelBasePrice;
pub type ResolvedPricingContextDto = ResolvedPricingContext;

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
    pub pricing_rules: Vec<PricingRuleDto>,
    pub developer_mode_enabled: bool,
}

impl From<PricingComparisonWorkspace> for PricingComparisonWorkspaceDto {
    fn from(value: PricingComparisonWorkspace) -> Self {
        Self {
            stations: value.stations.into_iter().map(StationDto::from).collect(),
            station_keys: value.station_keys,
            group_bindings: value.group_bindings,
            group_rates: value.group_rates,
            pricing_rules: value.pricing_rules,
            developer_mode_enabled: value.developer_mode_enabled,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const PRICING_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "PricingReadsDto",
    typescript: include_str!("pricing_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let rule = fixture_rule();
    let base_price = fixture_base_price();
    let context = fixture_context();
    let workspace = PricingComparisonWorkspaceDto {
        stations: Vec::new(),
        station_keys: Vec::new(),
        group_bindings: Vec::new(),
        group_rates: Vec::new(),
        pricing_rules: vec![rule.clone()],
        developer_mode_enabled: false,
    };
    vec![
        serde_json::json!({"command":"list_pricing_rules","input":{},"output":[rule]}),
        serde_json::json!({"command":"list_model_base_prices","input":{},"output":[base_price]}),
        serde_json::json!({
            "command":"resolve_station_key_pricing_context",
            "input":{"stationKeyId":"key-1","requestedModel":"fixture-model","requestKind":"text"},
            "output":context
        }),
        serde_json::json!({"command":"load_pricing_comparison_workspace","input":{},"output":workspace}),
    ]
}

#[cfg(test)]
fn fixture_rule() -> PricingRuleDto {
    PricingRule {
        id: "pricing-rule-1".into(),
        station_id: "station-1".into(),
        station_key_id: Some("key-1".into()),
        group_binding_id: None,
        group_name: None,
        tier_label: None,
        model: "fixture-model".into(),
        input_price: Some(1.0),
        output_price: Some(2.0),
        fixed_price: None,
        rate_multiplier: Some(0.8),
        currency: "USD".into(),
        unit: "M".into(),
        price_type: "token".into(),
        base_price_source: Some("fixture".into()),
        normalization_status: "complete".into(),
        source: "fixture".into(),
        confidence: 1.0,
        enabled: true,
        note: None,
        collected_at: Some("1700000000000".into()),
        valid_from: None,
        valid_until: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_base_price() -> ModelBasePriceDto {
    ModelBasePrice {
        id: "base-price-1".into(),
        provider: "fixture".into(),
        model: "fixture-model".into(),
        input_price: Some(1.0),
        output_price: Some(2.0),
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
        base_fixed_price: None,
        currency: "USD".into(),
        unit: "M".into(),
        base_price_source: Some("fixture".into()),
        effective_rate_multiplier: Some(0.8),
        rate_source: Some("fixture".into()),
        rate_collected_at: Some("1700000000000".into()),
        estimated_input_price: Some(0.8),
        estimated_output_price: Some(1.6),
        estimated_fixed_price: None,
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
}
