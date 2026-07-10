#![allow(dead_code)]

use crate::models::pricing::{
    BalanceSnapshot, PricingRule, PricingStatus, RequestCostBreakdown, RequestCostEstimate,
    RequestKind, RequestUsage, ResolvedPricingContext, UpsertBalanceSnapshotInput,
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

    #[test]
    fn cost_calculator_includes_fixed_price() {
        let context = crate::models::pricing::ResolvedPricingContext {
            station_key_id: "key-1".to_string(),
            station_id: "station-1".to_string(),
            requested_model: "gpt-5-mini".to_string(),
            resolved_model: "gpt-5-mini".to_string(),
            request_kind: crate::models::pricing::RequestKind::Text,
            group_binding_id: None,
            base_input_price: Some(0.25),
            base_output_price: Some(2.0),
            base_fixed_price: Some(0.05),
            currency: "USD".to_string(),
            unit: "per_1m_tokens".to_string(),
            base_price_source: Some("model_base_prices".to_string()),
            effective_rate_multiplier: Some(1.0),
            rate_source: None,
            rate_collected_at: None,
            estimated_input_price: Some(0.25),
            estimated_output_price: Some(2.0),
            estimated_fixed_price: Some(0.05),
            pricing_status: crate::models::pricing::PricingStatus::BasePriceOnly,
            confidence: 0.75,
            source_chain: vec!["model_base_prices:gpt-5-mini".to_string()],
            reason: Some("no_multiplier_expected".to_string()),
            resolved_at: "1000".to_string(),
        };
        let usage = crate::models::pricing::RequestUsage {
            input_tokens: Some(1_000),
            output_tokens: Some(500),
            total_tokens: Some(1_500),
            request_count: Some(1),
            cache_creation_tokens: None,
            cache_read_tokens: None,
            media_count: None,
            duration_seconds: None,
            size_tier: None,
        };

        let cost = calculate_request_cost(&context, &usage);

        assert_eq!(
            cost.pricing_status,
            crate::models::pricing::PricingStatus::BasePriceOnly
        );
        assert_eq!(cost.currency.as_deref(), Some("USD"));
        assert_option_f64_close(cost.input_cost, 0.00025);
        assert_option_f64_close(cost.output_cost, 0.001);
        assert_option_f64_close(cost.fixed_cost, 0.05);
        assert_option_f64_close(cost.total_cost, 0.05125);
    }

    fn assert_option_f64_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("expected numeric cost");
        assert!(
            (actual - expected).abs() < 0.000000001,
            "expected {expected}, got {actual}"
        );
    }
}

pub fn calculate_request_cost(
    context: &ResolvedPricingContext,
    usage: &RequestUsage,
) -> RequestCostBreakdown {
    if !context.pricing_status.can_have_numeric_cost() {
        return RequestCostBreakdown {
            input_cost: None,
            output_cost: None,
            fixed_cost: None,
            total_cost: None,
            currency: Some(context.currency.clone()),
            pricing_status: context.pricing_status.clone(),
            pricing_context_json: serialize_pricing_context(context),
        };
    }

    let input_tokens = usage.input_tokens.unwrap_or(0).max(0) as f64;
    let output_tokens = usage.output_tokens.unwrap_or(0).max(0) as f64;
    let input_cost = context
        .estimated_input_price
        .map(|price| price * input_tokens / 1_000_000.0);
    let output_cost = context
        .estimated_output_price
        .map(|price| price * output_tokens / 1_000_000.0);
    let fixed_cost = context.estimated_fixed_price;
    let total_cost = sum_costs([input_cost, output_cost, fixed_cost]);

    RequestCostBreakdown {
        input_cost,
        output_cost,
        fixed_cost,
        total_cost,
        currency: Some(context.currency.clone()),
        pricing_status: context.pricing_status.clone(),
        pricing_context_json: serialize_pricing_context(context),
    }
}

pub struct RequestPricingParts<'a> {
    pub station_key_id: &'a str,
    pub station_id: Option<&'a str>,
    pub model: Option<&'a str>,
    pub pricing_rule_id: Option<&'a str>,
    pub pricing_model: Option<&'a str>,
    pub group_binding_id: Option<&'a str>,
    pub rate_multiplier: Option<f64>,
    pub normalization_status: Option<&'a str>,
    pub price_confidence: Option<f64>,
    pub estimated_input_price: Option<f64>,
    pub estimated_output_price: Option<f64>,
    pub fixed_price: Option<f64>,
    pub price_currency: Option<&'a str>,
    pub pricing_source: Option<&'a str>,
    pub collected_at: Option<&'a str>,
}

pub fn request_cost_from_pricing_parts(
    parts: Option<RequestPricingParts<'_>>,
    prompt_tokens: Option<i64>,
    completion_tokens: Option<i64>,
    total_tokens: Option<i64>,
) -> RequestCostEstimate {
    let Some(parts) = parts else {
        return RequestCostEstimate {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            estimated_input_cost: None,
            estimated_output_cost: None,
            estimated_total_cost: None,
            cost_currency: None,
            pricing_rule_id: None,
            pricing_source: None,
            cost_status: "usage_only".to_string(),
        };
    };
    let requested_model = parts
        .model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(parts.pricing_model)
        .unwrap_or("unknown");
    let pricing_status = pricing_status_from_parts(&parts);
    let context = ResolvedPricingContext {
        station_key_id: parts.station_key_id.to_string(),
        station_id: parts.station_id.unwrap_or("unknown").to_string(),
        requested_model: requested_model.to_string(),
        resolved_model: parts.pricing_model.unwrap_or(requested_model).to_string(),
        request_kind: RequestKind::Text,
        group_binding_id: parts.group_binding_id.map(ToString::to_string),
        base_input_price: None,
        base_output_price: None,
        base_fixed_price: None,
        currency: parts.price_currency.unwrap_or("unknown").to_string(),
        unit: "per_1m_tokens".to_string(),
        base_price_source: parts.pricing_source.map(ToString::to_string),
        effective_rate_multiplier: parts.rate_multiplier,
        rate_source: parts.pricing_source.map(ToString::to_string),
        rate_collected_at: parts.collected_at.map(ToString::to_string),
        estimated_input_price: parts.estimated_input_price,
        estimated_output_price: parts.estimated_output_price,
        estimated_fixed_price: parts.fixed_price,
        pricing_status,
        confidence: parts.price_confidence.unwrap_or(0.0),
        source_chain: pricing_parts_source_chain(&parts),
        reason: pricing_parts_reason(&parts),
        resolved_at: parts.collected_at.unwrap_or("unknown").to_string(),
    };
    let usage = RequestUsage {
        input_tokens: prompt_tokens,
        output_tokens: completion_tokens,
        total_tokens,
        request_count: Some(1),
        cache_creation_tokens: None,
        cache_read_tokens: None,
        media_count: None,
        duration_seconds: None,
        size_tier: None,
    };
    let breakdown = calculate_request_cost(&context, &usage);

    RequestCostEstimate {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        estimated_input_cost: breakdown.input_cost,
        estimated_output_cost: breakdown.output_cost,
        estimated_total_cost: breakdown.total_cost,
        cost_currency: breakdown.currency,
        pricing_rule_id: parts.pricing_rule_id.map(ToString::to_string),
        pricing_source: parts.pricing_source.map(ToString::to_string),
        cost_status: if breakdown.total_cost.is_some() {
            context.pricing_status.as_str().to_string()
        } else {
            "usage_only".to_string()
        },
    }
}

fn pricing_status_from_parts(parts: &RequestPricingParts<'_>) -> PricingStatus {
    match parts.normalization_status {
        Some("base_price_only") => PricingStatus::BasePriceOnly,
        Some("base_price_with_group_rate") => PricingStatus::Priced,
        Some("complete") => PricingStatus::Priced,
        Some("group_rate_only")
            if parts.estimated_input_price.is_none()
                && parts.estimated_output_price.is_none()
                && parts.fixed_price.is_none() =>
        {
            PricingStatus::MissingModelPrice
        }
        _ if parts.estimated_input_price.is_some()
            || parts.estimated_output_price.is_some()
            || parts.fixed_price.is_some() =>
        {
            PricingStatus::Priced
        }
        _ => PricingStatus::Unpriced,
    }
}

fn pricing_parts_source_chain(parts: &RequestPricingParts<'_>) -> Vec<String> {
    let mut chain = Vec::new();
    if let Some(rule_id) = parts.pricing_rule_id {
        chain.push(format!("pricing_rule:{rule_id}"));
    }
    if let Some(model) = parts.pricing_model {
        chain.push(format!("model:{model}"));
    }
    if let Some(group_binding_id) = parts.group_binding_id {
        chain.push(format!("group_binding:{group_binding_id}"));
    }
    if let Some(source) = parts.pricing_source {
        chain.push(format!("pricing_source:{source}"));
    }
    chain
}

fn pricing_parts_reason(parts: &RequestPricingParts<'_>) -> Option<String> {
    if parts.estimated_input_price.is_some()
        || parts.estimated_output_price.is_some()
        || parts.fixed_price.is_some()
    {
        return None;
    }
    Some(
        match parts.normalization_status {
            Some("group_rate_only") => "model_base_price_not_found",
            _ => "pricing_not_available",
        }
        .to_string(),
    )
}

fn sum_costs(values: [Option<f64>; 3]) -> Option<f64> {
    let mut total = 0.0;
    let mut has_value = false;
    for value in values.into_iter().flatten() {
        total += value;
        has_value = true;
    }
    has_value.then_some(total)
}

fn serialize_pricing_context(context: &ResolvedPricingContext) -> Option<String> {
    serde_json::to_string(context).ok()
}
