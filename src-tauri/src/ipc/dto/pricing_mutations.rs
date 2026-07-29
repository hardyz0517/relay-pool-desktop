use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::pricing::{UpsertModelBasePriceInput, UpsertPricingRuleInput};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 512;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_URL_BYTES: usize = 2_048;
const MAX_TIMESTAMP_BYTES: usize = 64;
const MAX_PRICE: f64 = 1.0e12;
const MAX_MULTIPLIER: f64 = 1.0e6;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PricingRuleIdInputDto {
    pub id: String,
}

impl PricingRuleIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertModelBasePriceInputDto {
    pub id: Option<String>,
    pub provider: String,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub source_url: String,
    pub source_label: String,
    pub source_checked_at: Option<String>,
    pub enabled: bool,
    pub built_in: bool,
    pub note: Option<String>,
}

impl UpsertModelBasePriceInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_optional_id("id", input.id.as_deref())?;
        validate_text("provider", &input.provider, MAX_TEXT_BYTES, false)?;
        validate_text("model", &input.model, MAX_MODEL_BYTES, false)?;
        validate_price("inputPrice", input.input_price)?;
        validate_price("outputPrice", input.output_price)?;
        validate_text("currency", &input.currency, 16, false)?;
        validate_text("unit", &input.unit, 32, false)?;
        validate_optional_http_url("sourceUrl", &input.source_url)?;
        validate_text("sourceLabel", &input.source_label, MAX_TEXT_BYTES, false)?;
        validate_optional_timestamp("sourceCheckedAt", input.source_checked_at.as_deref())?;
        validate_optional_text("note", input.note.as_deref(), MAX_NOTE_BYTES)?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpsertModelBasePriceInput {
        UpsertModelBasePriceInput {
            id: self.id,
            provider: self.provider,
            model: self.model,
            input_price: self.input_price,
            output_price: self.output_price,
            currency: self.currency,
            unit: self.unit,
            source_url: self.source_url,
            source_label: self.source_label,
            source_checked_at: self.source_checked_at,
            enabled: self.enabled,
            built_in: self.built_in,
            note: self.note,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertPricingRuleInputDto {
    pub id: Option<String>,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub group_binding_id: Option<String>,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub rate_multiplier: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub price_type: String,
    pub base_price_source: Option<String>,
    pub normalization_status: Option<String>,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
}

impl UpsertPricingRuleInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_optional_id("id", input.id.as_deref())?;
        validate_id("stationId", &input.station_id)?;
        validate_optional_id("stationKeyId", input.station_key_id.as_deref())?;
        validate_optional_id("groupBindingId", input.group_binding_id.as_deref())?;
        validate_optional_text("groupName", input.group_name.as_deref(), MAX_TEXT_BYTES)?;
        validate_optional_text("tierLabel", input.tier_label.as_deref(), MAX_TEXT_BYTES)?;
        validate_text("model", &input.model, MAX_MODEL_BYTES, false)?;
        validate_price("inputPrice", input.input_price)?;
        validate_price("outputPrice", input.output_price)?;
        validate_price("fixedPrice", input.fixed_price)?;
        validate_bounded_number("rateMultiplier", input.rate_multiplier, MAX_MULTIPLIER)?;
        validate_text("currency", &input.currency, 16, false)?;
        validate_text("unit", &input.unit, 32, false)?;
        validate_text("priceType", &input.price_type, 64, false)?;
        validate_optional_text("basePriceSource", input.base_price_source.as_deref(), 128)?;
        validate_optional_text(
            "normalizationStatus",
            input.normalization_status.as_deref(),
            128,
        )?;
        validate_text("source", &input.source, 128, false)?;
        if !input.confidence.is_finite() || !(0.0..=1.0).contains(&input.confidence) {
            return Err(invalid_input(
                "confidence",
                "invalid_range",
                "The confidence must be between zero and one.",
            ));
        }
        validate_optional_text("note", input.note.as_deref(), MAX_NOTE_BYTES)?;
        validate_optional_timestamp("collectedAt", input.collected_at.as_deref())?;
        validate_optional_timestamp("validFrom", input.valid_from.as_deref())?;
        validate_optional_timestamp("validUntil", input.valid_until.as_deref())?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpsertPricingRuleInput {
        UpsertPricingRuleInput {
            id: self.id,
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            group_binding_id: self.group_binding_id,
            group_name: self.group_name,
            tier_label: self.tier_label,
            model: self.model,
            input_price: self.input_price,
            output_price: self.output_price,
            fixed_price: self.fixed_price,
            rate_multiplier: self.rate_multiplier,
            currency: self.currency,
            unit: self.unit,
            price_type: self.price_type,
            base_price_source: self.base_price_source,
            normalization_status: self.normalization_status,
            source: self.source,
            confidence: self.confidence,
            enabled: self.enabled,
            note: self.note,
            collected_at: self.collected_at,
            valid_from: self.valid_from,
            valid_until: self.valid_until,
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
            "The pricing mutation payload is invalid.",
        )
    })
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

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_text(field, value, max_bytes, true)?;
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    if (!allow_empty && value.trim().is_empty())
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

fn validate_price(
    field: &'static str,
    value: Option<f64>,
) -> Result<(), crate::commands::error::CommandError> {
    validate_bounded_number(field, value, MAX_PRICE)
}

fn validate_bounded_number(
    field: &'static str,
    value: Option<f64>,
    maximum: f64,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| !value.is_finite() || value < 0.0 || value > maximum) {
        return Err(invalid_input(
            field,
            "invalid_range",
            "The numeric value is outside the allowed range.",
        ));
    }
    Ok(())
}

fn validate_optional_http_url(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_empty() {
        return Ok(());
    }
    if value.len() > MAX_URL_BYTES {
        return Err(invalid_input(field, "too_long", "The URL is too long."));
    }
    let parsed = url::Url::parse(value)
        .map_err(|_| invalid_input(field, "invalid_url", "The URL is invalid."))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err(invalid_input(
            field,
            "invalid_scheme",
            "The URL must use HTTP or HTTPS.",
        ));
    }
    Ok(())
}

fn validate_optional_timestamp(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_text(field, value, MAX_TIMESTAMP_BYTES, false)?;
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub const PRICING_MUTATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "PricingMutationsDto",
    typescript: include_str!("pricing_mutations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    vec![
        serde_json::json!({"command":"upsert_model_base_price","input":fixture_base_price_input(),"output":fixture_base_price_output()}),
        serde_json::json!({"command":"reset_model_base_prices_to_builtins","input":{},"output":[fixture_base_price_output()]}),
        serde_json::json!({"command":"upsert_pricing_rule","input":fixture_rule_input(),"output":fixture_rule_output()}),
        serde_json::json!({"command":"delete_pricing_rule","input":{"id":"rule-1"},"output":null}),
    ]
}

#[cfg(test)]
fn fixture_base_price_input() -> Value {
    serde_json::json!({"id":null,"provider":"openai","model":"fixture-model","inputPrice":1.0,"outputPrice":2.0,"currency":"USD","unit":"M","sourceUrl":"https://example.test/pricing","sourceLabel":"Fixture","sourceCheckedAt":null,"enabled":true,"builtIn":false,"note":null})
}

#[cfg(test)]
fn fixture_base_price_output() -> Value {
    serde_json::json!({"id":"price-1","provider":"openai","model":"fixture-model","inputPrice":1.0,"outputPrice":2.0,"currency":"USD","unit":"M","sourceUrl":"https://example.test/pricing","sourceLabel":"Fixture","sourceCheckedAt":null,"enabled":true,"builtIn":false,"note":null,"createdAt":"1700000000000","updatedAt":"1700000000000"})
}

#[cfg(test)]
fn fixture_rule_input() -> Value {
    serde_json::json!({"id":null,"stationId":"station-1","stationKeyId":null,"groupBindingId":null,"groupName":null,"tierLabel":null,"model":"fixture-model","inputPrice":1.0,"outputPrice":2.0,"fixedPrice":null,"rateMultiplier":1.0,"currency":"USD","unit":"M","priceType":"token","basePriceSource":null,"normalizationStatus":null,"source":"manual","confidence":1.0,"enabled":true,"note":null,"collectedAt":null,"validFrom":null,"validUntil":null})
}

#[cfg(test)]
fn fixture_rule_output() -> Value {
    let mut value = fixture_rule_input();
    value["id"] = serde_json::json!("rule-1");
    value["normalizationStatus"] = serde_json::json!("manual");
    value["createdAt"] = serde_json::json!("1700000000000");
    value["updatedAt"] = serde_json::json!("1700000000000");
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn rejects_unknown_fields_invalid_urls_and_invalid_numbers() {
        let mut base = fixture_base_price_input();
        base["sourceUrl"] = serde_json::json!("file:///private");
        assert_eq!(
            UpsertModelBasePriceInputDto::parse(base)
                .expect_err("invalid URL")
                .code,
            CommandErrorCode::InvalidInput
        );

        let mut rule = fixture_rule_input();
        rule["confidence"] = serde_json::json!(2.0);
        assert_eq!(
            UpsertPricingRuleInputDto::parse(rule)
                .expect_err("invalid confidence")
                .code,
            CommandErrorCode::InvalidInput
        );

        let mut unknown = fixture_rule_input();
        unknown["unexpected"] = serde_json::json!(true);
        assert_eq!(
            UpsertPricingRuleInputDto::parse(unknown)
                .expect_err("unknown field")
                .code,
            CommandErrorCode::InvalidInput
        );
    }
}
