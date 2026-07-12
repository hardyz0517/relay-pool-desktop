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
        cache_creation_tokens: None,
        cache_read_tokens: None,
        billing_mode: None,
        estimated_input_cost: None,
        estimated_output_cost: None,
        estimated_total_cost: None,
        base_input_cost: None,
        base_output_cost: None,
        base_fixed_cost: None,
        base_total_cost: None,
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
        today_request_count: input.today_request_count,
        total_request_count: input.total_request_count,
        today_consumption: input.today_consumption,
        total_consumption: input.total_consumption,
        today_token_count: input.today_token_count,
        total_token_count: input.total_token_count,
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

    #[test]
    fn pricing_diagnostics_context_returns_source_chain() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_rule_id: Some("rule-1"),
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: Some("binding-1"),
            rate_multiplier: Some(0.8),
            normalization_status: Some("base_price_with_group_rate"),
            price_confidence: Some(0.9),
            base_input_price: None,
            base_output_price: None,
            base_fixed_price: None,
            estimated_input_price: Some(0.3),
            estimated_output_price: Some(1.8),
            fixed_price: None,
            price_currency: Some("USD"),
            pricing_source: Some("model_base_price"),
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::Priced);
        assert_eq!(
            context.source_chain,
            vec![
                "pricing_rule:rule-1".to_string(),
                "model:gpt-5.4-mini".to_string(),
                "group_binding:binding-1".to_string(),
                "pricing_source:model_base_price".to_string(),
            ]
        );
    }

    #[test]
    fn request_cost_keeps_actual_and_one_x_base_costs_separate() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_rule_id: None,
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: Some("binding-1"),
            rate_multiplier: Some(0.1),
            normalization_status: Some("base_price_with_group_rate"),
            price_confidence: Some(0.9),
            base_input_price: Some(10.0),
            base_output_price: Some(20.0),
            base_fixed_price: None,
            estimated_input_price: Some(1.0),
            estimated_output_price: Some(2.0),
            fixed_price: None,
            price_currency: Some("USD"),
            pricing_source: Some("model_base_price"),
            collected_at: Some("1000"),
        };

        let cost = request_cost_from_pricing_parts(Some(parts), Some(100), Some(100), Some(200));

        assert_option_f64_close(cost.estimated_total_cost, 0.0003);
        assert_option_f64_close(cost.base_total_cost, 0.003);
        assert_eq!(cost.cost_status, "priced");
    }

    #[test]
    fn pricing_diagnostics_context_marks_missing_expected_rate() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_rule_id: None,
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: Some("binding-1"),
            rate_multiplier: None,
            normalization_status: None,
            price_confidence: Some(0.5),
            base_input_price: None,
            base_output_price: None,
            base_fixed_price: None,
            estimated_input_price: None,
            estimated_output_price: None,
            fixed_price: None,
            price_currency: Some("USD"),
            pricing_source: None,
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::MissingRate);
        assert_eq!(context.reason.as_deref(), Some("pricing_not_available"));
    }

    #[test]
    fn pricing_diagnostics_context_marks_base_price_only() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.4-mini"),
            pricing_rule_id: None,
            pricing_model: Some("gpt-5.4-mini"),
            group_binding_id: None,
            rate_multiplier: Some(1.0),
            normalization_status: Some("base_price_only"),
            price_confidence: Some(0.8),
            base_input_price: Some(0.375),
            base_output_price: Some(2.25),
            base_fixed_price: None,
            estimated_input_price: Some(0.375),
            estimated_output_price: Some(2.25),
            fixed_price: None,
            price_currency: Some("USD"),
            pricing_source: Some("model_base_price"),
            collected_at: Some("1000"),
        };

        let context = pricing_context_from_pricing_parts(&parts);

        assert_eq!(context.pricing_status, PricingStatus::BasePriceOnly);
        assert_eq!(
            context.source_chain,
            vec![
                "model:gpt-5.4-mini".to_string(),
                "pricing_source:model_base_price".to_string(),
            ]
        );
    }

    #[test]
    fn usage_aware_cost_preserves_cache_tokens_and_billing_mode() {
        let parts = RequestPricingParts {
            station_key_id: "key-1",
            station_id: Some("station-1"),
            model: Some("gpt-5.5"),
            pricing_rule_id: Some("rule-fixed"),
            pricing_model: Some("gpt-5.5"),
            group_binding_id: Some("group-1"),
            rate_multiplier: Some(1.0),
            normalization_status: Some("complete"),
            price_confidence: Some(1.0),
            base_input_price: None,
            base_output_price: None,
            base_fixed_price: Some(0.01),
            estimated_input_price: None,
            estimated_output_price: None,
            fixed_price: Some(0.01),
            price_currency: Some("USD"),
            pricing_source: Some("pricing_rule"),
            collected_at: Some("1000"),
        };
        let usage = RequestUsage {
            input_tokens: Some(100),
            output_tokens: Some(20),
            total_tokens: Some(120),
            request_count: Some(1),
            cache_creation_tokens: Some(7),
            cache_read_tokens: Some(70),
            media_count: None,
            duration_seconds: None,
            size_tier: None,
        };

        let cost = request_cost_from_pricing_parts_and_usage(Some(parts), &usage);

        assert_eq!(cost.cache_creation_tokens, Some(7));
        assert_eq!(cost.cache_read_tokens, Some(70));
        assert_eq!(cost.billing_mode.as_deref(), Some("per_request"));
        assert_option_f64_close(cost.estimated_total_cost, 0.01);
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
            base_input_cost: None,
            base_output_cost: None,
            base_fixed_cost: None,
            base_total_cost: None,
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
    let base_input_price = one_x_price(
        context.base_input_price,
        context.estimated_input_price,
        context.effective_rate_multiplier,
    );
    let base_output_price = one_x_price(
        context.base_output_price,
        context.estimated_output_price,
        context.effective_rate_multiplier,
    );
    let base_fixed_price = one_x_price(
        context.base_fixed_price,
        context.estimated_fixed_price,
        context.effective_rate_multiplier,
    );
    let base_input_cost = base_input_price.map(|price| price * input_tokens / 1_000_000.0);
    let base_output_cost = base_output_price.map(|price| price * output_tokens / 1_000_000.0);
    let base_fixed_cost = base_fixed_price;
    let base_total_cost = sum_costs([base_input_cost, base_output_cost, base_fixed_cost]);

    RequestCostBreakdown {
        input_cost,
        output_cost,
        fixed_cost,
        total_cost,
        base_input_cost,
        base_output_cost,
        base_fixed_cost,
        base_total_cost,
        currency: Some(context.currency.clone()),
        pricing_status: context.pricing_status.clone(),
        pricing_context_json: serialize_pricing_context(context),
    }
}

fn one_x_price(
    base_price: Option<f64>,
    estimated_price: Option<f64>,
    multiplier: Option<f64>,
) -> Option<f64> {
    base_price.or_else(|| {
        let multiplier = multiplier?;
        if multiplier.is_finite() && multiplier > 0.0 {
            estimated_price.map(|price| price / multiplier)
        } else {
            estimated_price
        }
    })
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
    pub base_input_price: Option<f64>,
    pub base_output_price: Option<f64>,
    pub base_fixed_price: Option<f64>,
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
    request_cost_from_pricing_parts_and_usage(parts, &usage)
}

pub fn request_cost_from_pricing_parts_and_usage(
    parts: Option<RequestPricingParts<'_>>,
    usage: &RequestUsage,
) -> RequestCostEstimate {
    let prompt_tokens = usage.input_tokens;
    let completion_tokens = usage.output_tokens;
    let total_tokens = usage.total_tokens.or_else(|| {
        prompt_tokens
            .zip(completion_tokens)
            .map(|(input, output)| input + output)
    });
    let Some(parts) = parts else {
        return RequestCostEstimate {
            prompt_tokens,
            completion_tokens,
            total_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            billing_mode: Some("token".to_string()),
            estimated_input_cost: None,
            estimated_output_cost: None,
            estimated_total_cost: None,
            base_input_cost: None,
            base_output_cost: None,
            base_fixed_cost: None,
            base_total_cost: None,
            cost_currency: None,
            pricing_rule_id: None,
            pricing_source: None,
            cost_status: "usage_only".to_string(),
        };
    };
    let context = pricing_context_from_pricing_parts(&parts);
    let breakdown = calculate_request_cost(&context, usage);
    let billing_mode = if context.estimated_fixed_price.is_some()
        && context.estimated_input_price.is_none()
        && context.estimated_output_price.is_none()
    {
        "per_request"
    } else {
        "token"
    };

    RequestCostEstimate {
        prompt_tokens,
        completion_tokens,
        total_tokens,
        cache_creation_tokens: usage.cache_creation_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        billing_mode: Some(billing_mode.to_string()),
        estimated_input_cost: breakdown.input_cost,
        estimated_output_cost: breakdown.output_cost,
        estimated_total_cost: breakdown.total_cost,
        base_input_cost: breakdown.base_input_cost,
        base_output_cost: breakdown.base_output_cost,
        base_fixed_cost: breakdown.base_fixed_cost,
        base_total_cost: breakdown.base_total_cost,
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

pub fn pricing_context_from_pricing_parts(
    parts: &RequestPricingParts<'_>,
) -> ResolvedPricingContext {
    let requested_model = parts
        .model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(parts.pricing_model)
        .unwrap_or("unknown");
    let pricing_status = pricing_status_from_parts(&parts);
    ResolvedPricingContext {
        station_key_id: parts.station_key_id.to_string(),
        station_id: parts.station_id.unwrap_or("unknown").to_string(),
        requested_model: requested_model.to_string(),
        resolved_model: parts.pricing_model.unwrap_or(requested_model).to_string(),
        request_kind: RequestKind::Text,
        group_binding_id: parts.group_binding_id.map(ToString::to_string),
        base_input_price: parts.base_input_price,
        base_output_price: parts.base_output_price,
        base_fixed_price: parts.base_fixed_price,
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
        _ if parts.group_binding_id.is_some()
            && parts.rate_multiplier.is_none()
            && parts.estimated_input_price.is_none()
            && parts.estimated_output_price.is_none()
            && parts.fixed_price.is_none() =>
        {
            PricingStatus::MissingRate
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
