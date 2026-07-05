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
    let normalization_status = input
        .normalization_status
        .clone()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    let is_group_rate_only = normalization_status.as_deref() == Some("group_rate_only");
    UpsertPricingRuleInput {
        station_key_id: input
            .station_key_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        group_binding_id: input
            .group_binding_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        group_name: input
            .group_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        tier_label: input
            .tier_label
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        model: input.model.trim().to_string(),
        input_price: if is_group_rate_only {
            None
        } else {
            input.input_price
        },
        output_price: if is_group_rate_only {
            None
        } else {
            input.output_price
        },
        fixed_price: if is_group_rate_only {
            None
        } else {
            input.fixed_price
        },
        rate_multiplier: input.rate_multiplier,
        currency: normalize_currency(input.currency),
        unit: normalize_unit(input.unit),
        price_type: input.price_type.trim().to_string(),
        base_price_source: input
            .base_price_source
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        normalization_status,
        source: input.source.trim().to_string(),
        confidence: clamp_confidence(input.confidence),
        enabled: input.enabled,
        note: input
            .note
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        collected_at: input
            .collected_at
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        valid_from: input
            .valid_from
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        valid_until: input
            .valid_until
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        ..input
    }
}

pub fn sanitize_balance_snapshot_input(
    input: UpsertBalanceSnapshotInput,
) -> UpsertBalanceSnapshotInput {
    UpsertBalanceSnapshotInput {
        station_id: input.station_id,
        station_key_id: input
            .station_key_id
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        scope: input.scope.trim().to_string(),
        value: input.value,
        currency: normalize_currency(input.currency),
        credit_unit: input
            .credit_unit
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        used_value: input.used_value,
        total_value: input.total_value,
        low_balance_threshold: input.low_balance_threshold,
        status: input.status.trim().to_string(),
        source: input.source.trim().to_string(),
        confidence: clamp_confidence(input.confidence),
        collected_at: input
            .collected_at
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
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
            format!(
                "{}: {}/{} {} {}",
                rule.model, input, output, rule.currency, rule.unit
            )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_rate_only_is_not_complete_pricing() {
        let sanitized = sanitize_pricing_rule_input(UpsertPricingRuleInput {
            id: None,
            station_id: "station-1".to_string(),
            station_key_id: Some(" key-1 ".to_string()),
            group_binding_id: Some(" group-1 ".to_string()),
            group_name: Some(" pro ".to_string()),
            tier_label: None,
            model: " gpt-5.4 ".to_string(),
            input_price: Some(0.1),
            output_price: Some(0.2),
            fixed_price: Some(0.3),
            rate_multiplier: Some(0.8),
            currency: " USD ".to_string(),
            unit: " per_1m_tokens ".to_string(),
            price_type: " token ".to_string(),
            base_price_source: Some(" model_api ".to_string()),
            normalization_status: Some(" group_rate_only ".to_string()),
            source: " collector ".to_string(),
            confidence: 2.0,
            enabled: true,
            note: Some(" ".to_string()),
            collected_at: Some(" 1000 ".to_string()),
            valid_from: Some(" 1000 ".to_string()),
            valid_until: Some(" 2000 ".to_string()),
        });

        assert_eq!(sanitized.model, "gpt-5.4");
        assert_eq!(sanitized.station_key_id.as_deref(), Some("key-1"));
        assert_eq!(sanitized.group_binding_id.as_deref(), Some("group-1"));
        assert_eq!(
            sanitized.normalization_status.as_deref(),
            Some("group_rate_only")
        );
        assert_eq!(sanitized.rate_multiplier, Some(0.8));
        assert_eq!(sanitized.input_price, None);
        assert_eq!(sanitized.output_price, None);
        assert_eq!(sanitized.fixed_price, None);
        assert_eq!(sanitized.confidence, 1.0);
        assert_eq!(sanitized.note, None);
    }
}
