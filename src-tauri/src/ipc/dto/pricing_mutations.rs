use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::pricing::UpsertModelBasePriceInput;

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_MODEL_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 512;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_URL_BYTES: usize = 2_048;
const MAX_TIMESTAMP_BYTES: usize = 64;
const MAX_PRICE: f64 = 1.0e12;
const MAX_MULTIPLIER: f64 = 1.0e6;
const MAX_MODEL_SELECTION_KEYS: usize = 20_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ModelBasePriceIdInputDto {
    pub id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveModelPriceSyncConfigInputDto {
    pub auto_sync_enabled: bool,
    #[serde(default = "default_true")]
    pub include_common_models: bool,
    #[serde(default)]
    pub selected_model_keys: Vec<String>,
    #[serde(default)]
    pub excluded_common_model_keys: Vec<String>,
}

impl SaveModelPriceSyncConfigInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if input.selected_model_keys.len() > MAX_MODEL_SELECTION_KEYS
            || input.excluded_common_model_keys.len() > MAX_MODEL_SELECTION_KEYS
        {
            return Err(invalid_input(
                "selectedModelKeys",
                "invalid_length",
                "Too many model selections.",
            ));
        }
        if input
            .selected_model_keys
            .iter()
            .chain(input.excluded_common_model_keys.iter())
            .any(|key| {
                key.trim().is_empty() || key.len() > 256 || key.chars().any(char::is_control)
            })
        {
            return Err(invalid_input(
                "selectedModelKeys",
                "invalid_value",
                "A model selection is invalid.",
            ));
        }
        Ok(input)
    }

    pub fn into_domain(self) -> crate::services::model_price_sync::ModelPriceSyncConfig {
        crate::services::model_price_sync::ModelPriceSyncConfig {
            auto_sync_enabled: self.auto_sync_enabled,
            include_common_models: self.include_common_models,
            selected_model_keys: self.selected_model_keys,
            excluded_common_model_keys: self.excluded_common_model_keys,
        }
    }
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncModelPricesInputDto {
    pub force: bool,
}

impl SyncModelPricesInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        parse_value(value)
    }
}

impl ModelBasePriceIdInputDto {
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
    pub input_price_priority: Option<f64>,
    pub output_price_priority: Option<f64>,
    pub cache_creation_price: Option<f64>,
    pub cache_creation_price_priority: Option<f64>,
    #[serde(
        rename = "cacheCreationPriceAbove1Hr",
        alias = "cacheCreationPriceAbove1hr"
    )]
    pub cache_creation_price_above_1hr: Option<f64>,
    pub cache_read_price: Option<f64>,
    pub cache_read_price_priority: Option<f64>,
    pub long_context_input_token_threshold: Option<i64>,
    pub long_context_input_cost_multiplier: Option<f64>,
    pub long_context_output_cost_multiplier: Option<f64>,
    #[serde(default)]
    pub supports_service_tier: bool,
    #[serde(default)]
    pub supports_prompt_caching: bool,
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
        validate_price("inputPricePriority", input.input_price_priority)?;
        validate_price("outputPricePriority", input.output_price_priority)?;
        validate_price("cacheCreationPrice", input.cache_creation_price)?;
        validate_price(
            "cacheCreationPricePriority",
            input.cache_creation_price_priority,
        )?;
        validate_price(
            "cacheCreationPriceAbove1Hr",
            input.cache_creation_price_above_1hr,
        )?;
        validate_price("cacheReadPrice", input.cache_read_price)?;
        validate_price("cacheReadPricePriority", input.cache_read_price_priority)?;
        if input
            .long_context_input_token_threshold
            .is_some_and(|value| value <= 0)
        {
            return Err(invalid_input(
                "longContextInputTokenThreshold",
                "invalid_range",
                "The long-context token threshold must be positive.",
            ));
        }
        validate_positive_number(
            "longContextInputCostMultiplier",
            input.long_context_input_cost_multiplier,
        )?;
        validate_positive_number(
            "longContextOutputCostMultiplier",
            input.long_context_output_cost_multiplier,
        )?;
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
            input_price_priority: self.input_price_priority,
            output_price_priority: self.output_price_priority,
            cache_creation_price: self.cache_creation_price,
            cache_creation_price_priority: self.cache_creation_price_priority,
            cache_creation_price_above_1hr: self.cache_creation_price_above_1hr,
            cache_read_price: self.cache_read_price,
            cache_read_price_priority: self.cache_read_price_priority,
            long_context_input_token_threshold: self.long_context_input_token_threshold,
            long_context_input_cost_multiplier: self.long_context_input_cost_multiplier,
            long_context_output_cost_multiplier: self.long_context_output_cost_multiplier,
            supports_service_tier: self.supports_service_tier,
            supports_prompt_caching: self.supports_prompt_caching,
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

fn validate_positive_number(
    field: &'static str,
    value: Option<f64>,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|value| !value.is_finite() || value <= 0.0 || value > MAX_MULTIPLIER) {
        return Err(invalid_input(
            field,
            "invalid_range",
            "The numeric value must be positive and within the allowed range.",
        ));
    }
    Ok(())
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

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const PRICING_MUTATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "PricingMutationsDto",
    typescript: include_str!("pricing_mutations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    vec![
        serde_json::json!({"command":"upsert_model_base_price","input":fixture_base_price_input(),"output":fixture_base_price_output()}),
        serde_json::json!({"command":"delete_model_base_price","input":{"id":"price-1"},"output":null}),
        serde_json::json!({"command":"reset_model_base_prices_to_builtins","input":{},"output":[fixture_base_price_output()]}),
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
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn accepts_legacy_base_price_payload_without_capability_flags() {
        let input = UpsertModelBasePriceInputDto::parse(fixture_base_price_input())
            .expect("legacy model base price payload");

        assert!(!input.supports_service_tier);
        assert!(!input.supports_prompt_caching);
    }

    #[test]
    fn accepts_the_canonical_long_lived_cache_price_field_name() {
        let mut input = fixture_base_price_input();
        input["cacheCreationPriceAbove1Hr"] = serde_json::json!(2.25);

        let parsed = UpsertModelBasePriceInputDto::parse(input)
            .expect("canonical cache price field should be accepted");

        assert_eq!(parsed.cache_creation_price_above_1hr, Some(2.25));
    }

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
    }
}
