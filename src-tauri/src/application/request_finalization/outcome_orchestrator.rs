#![allow(dead_code)]

use serde_json::json;

use crate::application::{
    request_finalization::outcome::{
        AttemptCostSnapshot, AttemptCostStatus, AttemptUsageStatus, FrozenPricingBasis,
        RequestCostAggregate, RequestCostAggregateStatus,
    },
    request_lifecycle::{
        ports::{AttemptCostCommitRecord, RequestCostAggregateCommitRecord},
        request::AttemptId,
    },
};

pub(crate) fn attempt_cost_commit_record(
    cost: &AttemptCostSnapshot,
    created_at_ms: i64,
) -> AttemptCostCommitRecord {
    let tokens = cost.usage.tokens.as_ref();
    AttemptCostCommitRecord {
        request_id: cost.attempt_id.request_id.clone(),
        ordinal: cost.attempt_id.ordinal,
        pricing_context_id: cost.pricing.pricing_context_id.clone(),
        pricing_basis: pricing_basis_str(cost.pricing.basis).to_string(),
        pricing_status_label: cost.pricing.status_label.clone(),
        usage_status: usage_status_str(cost.usage.status).to_string(),
        input_tokens: tokens.map(|usage| usage.input_tokens),
        output_tokens: tokens.map(|usage| usage.output_tokens),
        total_tokens: tokens.map(|usage| usage.total_tokens),
        cache_creation_tokens: tokens.and_then(|usage| usage.cache_creation_tokens),
        cache_read_tokens: tokens.and_then(|usage| usage.cache_read_tokens),
        cost_status: cost_status_str(cost.status).to_string(),
        currency: cost.currency.clone(),
        total_cost_micro: cost.total_cost_micro,
        created_at_ms,
    }
}

pub(crate) fn request_cost_aggregate_commit_record(
    request_id: impl Into<String>,
    aggregate: &RequestCostAggregate,
    written_at_ms: i64,
) -> RequestCostAggregateCommitRecord {
    let incomplete = aggregate
        .incomplete_attempts
        .iter()
        .map(|gap| {
            json!({
                "request_id": gap.attempt_id.request_id,
                "ordinal": gap.attempt_id.ordinal,
                "status": cost_status_str(gap.status),
            })
        })
        .collect::<Vec<_>>();
    let (compatibility_currency, compatibility_total_cost_micro) = aggregate
        .compatibility_total
        .as_ref()
        .map_or((None, None), |money| {
            (Some(money.currency.clone()), Some(money.amount_micro))
        });
    RequestCostAggregateCommitRecord {
        request_id: request_id.into(),
        status: aggregate_status_str(&aggregate.status).to_string(),
        totals_by_currency_json: serde_json::to_string(&aggregate.totals_by_currency_micro)
            .expect("BTreeMap string/i64 serializes"),
        compatibility_currency,
        compatibility_total_cost_micro,
        incomplete_attempts_json: serde_json::to_string(&incomplete)
            .expect("incomplete attempts serialize"),
        written_at_ms,
    }
}

pub(crate) fn interrupted_attempt_cost(
    attempt_id: AttemptId,
    created_at_ms: i64,
) -> AttemptCostCommitRecord {
    AttemptCostCommitRecord {
        request_id: attempt_id.request_id,
        ordinal: attempt_id.ordinal,
        pricing_context_id: "trace_incomplete".to_string(),
        pricing_basis: "unpriced".to_string(),
        pricing_status_label: "trace_incomplete".to_string(),
        usage_status: "missing_usage".to_string(),
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cache_creation_tokens: None,
        cache_read_tokens: None,
        cost_status: "missing_usage".to_string(),
        currency: None,
        total_cost_micro: None,
        created_at_ms,
    }
}

fn pricing_basis_str(value: FrozenPricingBasis) -> &'static str {
    match value {
        FrozenPricingBasis::ExactPrice => "exact_price",
        FrozenPricingBasis::MultiplierProxy => "multiplier_proxy",
        FrozenPricingBasis::Unpriced => "unpriced",
        FrozenPricingBasis::NotApplicable => "not_applicable",
    }
}

fn usage_status_str(value: AttemptUsageStatus) -> &'static str {
    match value {
        AttemptUsageStatus::Complete => "complete",
        AttemptUsageStatus::MissingUsage => "missing_usage",
        AttemptUsageStatus::StreamUsageMissing => "stream_usage_missing",
        AttemptUsageStatus::NotApplicable => "not_applicable",
    }
}

fn cost_status_str(value: AttemptCostStatus) -> &'static str {
    match value {
        AttemptCostStatus::Priced => "priced",
        AttemptCostStatus::MissingUsage => "missing_usage",
        AttemptCostStatus::StreamUsageMissing => "stream_usage_missing",
        AttemptCostStatus::Unpriced => "unpriced",
        AttemptCostStatus::PricingIncomplete => "pricing_incomplete",
        AttemptCostStatus::NotApplicable => "not_applicable",
    }
}

fn aggregate_status_str(value: &RequestCostAggregateStatus) -> &'static str {
    match value {
        RequestCostAggregateStatus::NoAttempts => "no_attempts",
        RequestCostAggregateStatus::CompleteSingleCurrency { .. } => "complete_single_currency",
        RequestCostAggregateStatus::CompleteMixedCurrency { .. } => "complete_mixed_currency",
        RequestCostAggregateStatus::Incomplete => "incomplete",
        RequestCostAggregateStatus::NotApplicable => "not_applicable",
    }
}
