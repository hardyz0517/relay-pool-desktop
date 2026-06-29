#![allow(dead_code)]

use crate::models::pricing::{
    BalanceSnapshot, PricingRule, RequestCostEstimate, UpsertBalanceSnapshotInput,
    UpsertPricingRuleInput,
};

pub fn normalize_currency(value: String) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

pub fn normalize_unit(value: String) -> String {
    let normalized = value.trim();
    if normalized.is_empty() {
        "unknown".to_string()
    } else {
        normalized.to_string()
    }
}

pub fn clamp_confidence(value: f64) -> f64 {
    value.clamp(0.0, 1.0)
}

pub fn request_cost_unknown() -> RequestCostEstimate {
    RequestCostEstimate {
        prompt_tokens: None,
        completion_tokens: None,
        total_tokens: None,
        estimated_input_cost: None,
        estimated_output_cost: None,
        estimated_total_cost: None,
        cost_currency: None,
        pricing_rule_id: None,
        pricing_source: None,
        cost_status: "unknown_usage".to_string(),
    }
}

pub fn sanitize_pricing_rule_input(input: UpsertPricingRuleInput) -> UpsertPricingRuleInput {
    UpsertPricingRuleInput {
        group_name: input.group_name.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        tier_label: input.tier_label.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        model: input.model.trim().to_string(),
        input_price: input.input_price,
        output_price: input.output_price,
        fixed_price: input.fixed_price,
        currency: normalize_currency(input.currency),
        unit: normalize_unit(input.unit),
        price_type: input.price_type.trim().to_string(),
        source: input.source.trim().to_string(),
        confidence: clamp_confidence(input.confidence),
        enabled: input.enabled,
        note: input.note.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        collected_at: input.collected_at.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        ..input
    }
}

pub fn sanitize_balance_snapshot_input(input: UpsertBalanceSnapshotInput) -> UpsertBalanceSnapshotInput {
    UpsertBalanceSnapshotInput {
        station_id: input.station_id,
        station_key_id: input.station_key_id.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        scope: input.scope.trim().to_string(),
        value: input.value,
        currency: normalize_currency(input.currency),
        credit_unit: input.credit_unit.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        used_value: input.used_value,
        total_value: input.total_value,
        low_balance_threshold: input.low_balance_threshold,
        status: input.status.trim().to_string(),
        source: input.source.trim().to_string(),
        confidence: clamp_confidence(input.confidence),
        collected_at: input.collected_at.map(|value| value.trim().to_string()).filter(|value| !value.is_empty()),
        id: input.id,
    }
}

pub fn summarize_pricing_rules(rules: &[PricingRule]) -> Vec<String> {
    rules
        .iter()
        .map(|rule| {
            let input = rule
                .input_price
                .map(|value| format!("{value:.4}"))
                .unwrap_or_else(|| "unknown".to_string());
            let output = rule
                .output_price
                .map(|value| format!("{value:.4}"))
                .unwrap_or_else(|| "unknown".to_string());
            format!("{}: {}/{} {} {}", rule.model, input, output, rule.currency, rule.unit)
        })
        .collect()
}

pub fn summarize_balance_snapshot(snapshot: &BalanceSnapshot) -> String {
    format!(
        "{} {} {}",
        snapshot.scope,
        snapshot.currency,
        snapshot
            .value
            .map(|value| format!("{value:.2}"))
            .unwrap_or_else(|| "unknown".to_string())
    )
}
