use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PricingRule {
    pub id: String,
    pub station_id: String,
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub price_type: String,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
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
    pub group_name: Option<String>,
    pub tier_label: Option<String>,
    pub model: String,
    pub input_price: Option<f64>,
    pub output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub currency: String,
    pub unit: String,
    pub price_type: String,
    pub source: String,
    pub confidence: f64,
    pub enabled: bool,
    pub note: Option<String>,
    pub collected_at: Option<String>,
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
            group_name: Some("pro".to_string()),
            tier_label: None,
            model: "gpt-4o-mini".to_string(),
            input_price: Some(0.15),
            output_price: Some(0.6),
            fixed_price: None,
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            price_type: "token".to_string(),
            source: "manual".to_string(),
            confidence: 0.9,
            enabled: true,
            note: None,
            collected_at: Some("1000".to_string()),
            created_at: "1000".to_string(),
            updated_at: "1000".to_string(),
        };

        let json = serde_json::to_value(rule).expect("json");
        assert_eq!(json["stationId"], "station-1");
        assert_eq!(json["inputPrice"], 0.15);
        assert_eq!(json["priceType"], "token");
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
