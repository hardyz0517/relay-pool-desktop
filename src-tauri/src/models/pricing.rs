use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricingRule {
    pub id: String,
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
    pub normalization_status: String,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
    pub valid_from: Option<String>,
    pub valid_until: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ModelBasePrice {
    pub id: String,
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
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BalanceSnapshot {
    pub id: String,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertPricingRuleInput {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertModelBasePriceInput {
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UpsertBalanceSnapshotInput {
    pub id: Option<String>,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub scope: String,
    pub value: Option<f64>,
    pub currency: String,
    pub credit_unit: Option<String>,
    pub used_value: Option<f64>,
    pub total_value: Option<f64>,
    pub low_balance_threshold: Option<f64>,
    pub status: String,
    pub source: String,
    pub confidence: f64,
    pub collected_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RequestCostEstimate {
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub estimated_input_cost: Option<f64>,
    pub estimated_output_cost: Option<f64>,
    pub estimated_total_cost: Option<f64>,
    pub cost_currency: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub pricing_source: Option<String>,
    pub cost_status: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pricing_rule_serializes_camel_case() {
        let rule = PricingRule {
            id: "price-1".to_string(),
            station_id: "station-1".to_string(),
            station_key_id: Some("key-1".to_string()),
            group_binding_id: Some("binding-1".to_string()),
            group_name: Some("pro".to_string()),
            tier_label: None,
            model: "gpt-4o-mini".to_string(),
            input_price: Some(0.15),
            output_price: Some(0.6),
            fixed_price: None,
            rate_multiplier: Some(0.8),
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            price_type: "token".to_string(),
            base_price_source: Some("model_api".to_string()),
            normalization_status: "complete".to_string(),
            source: "manual".to_string(),
            confidence: 0.9,
            enabled: true,
            note: None,
            collected_at: Some("1000".to_string()),
            valid_from: Some("1000".to_string()),
            valid_until: None,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        };

        let json = serde_json::to_value(rule).expect("json");
        assert_eq!(json["stationId"], "station-1");
        assert_eq!(json["groupBindingId"], "binding-1");
        assert_eq!(json["inputPrice"], 0.15);
        assert_eq!(json["priceType"], "token");
        assert_eq!(json["normalizationStatus"], "complete");
    }

    #[test]
    fn model_base_price_serializes_camel_case() {
        let price = ModelBasePrice {
            id: "base-1".to_string(),
            provider: "openai".to_string(),
            model: "gpt-5-mini".to_string(),
            input_price: Some(0.25),
            output_price: Some(2.0),
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            source_url: "https://developers.openai.com/api/docs/pricing".to_string(),
            source_label: "OpenAI API pricing".to_string(),
            source_checked_at: Some("2026-07-08".to_string()),
            enabled: true,
            built_in: true,
            note: None,
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        };

        let json = serde_json::to_value(price).expect("json");
        assert_eq!(json["inputPrice"], 0.25);
        assert_eq!(
            json["sourceUrl"],
            "https://developers.openai.com/api/docs/pricing"
        );
        assert_eq!(json["builtIn"], true);
    }

    #[test]
    fn request_cost_estimate_serializes_camel_case() {
        let estimate = RequestCostEstimate {
            prompt_tokens: Some(10),
            completion_tokens: Some(5),
            total_tokens: Some(15),
            estimated_input_cost: Some(0.1),
            estimated_output_cost: Some(0.2),
            estimated_total_cost: Some(0.3),
            cost_currency: Some("USD".to_string()),
            pricing_rule_id: Some("price-1".to_string()),
            pricing_source: Some("manual".to_string()),
            cost_status: "estimated".to_string(),
        };

        let json = serde_json::to_value(estimate).expect("json");
        assert_eq!(json["promptTokens"], 10);
        assert_eq!(json["estimatedTotalCost"], 0.3);
        assert_eq!(json["costStatus"], "estimated");
    }
}
