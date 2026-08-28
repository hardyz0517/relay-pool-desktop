mod application {
    pub(crate) mod health_protection {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub(crate) struct HealthProtectionScope;
    }

    pub(crate) mod request_finalization {
        pub(crate) mod failure {
            pub(crate) use crate::failure::*;
        }
        pub(crate) mod outcome {
            pub(crate) use crate::outcome::*;
        }
    }

    pub(crate) mod request_lifecycle {
        pub(crate) mod delivery {
            pub(crate) use crate::delivery::*;
        }
        pub(crate) mod request {
            pub(crate) use crate::request::*;
        }
        pub(crate) mod attempt {
            pub(crate) use crate::attempt::*;
        }
    }
}

#[path = "../src/application/request_lifecycle/attempt.rs"]
mod attempt;
#[path = "../src/application/request_lifecycle/delivery.rs"]
mod delivery;
#[path = "../src/application/request_finalization/effect_planner.rs"]
mod effect_planner;
#[path = "../src/application/request_finalization/failure.rs"]
mod failure;
#[path = "../src/application/request_finalization/outcome.rs"]
mod outcome;
#[path = "../src/application/request_lifecycle/request.rs"]
mod request;

use delivery::DeliveryTerminal;
use effect_planner::plan_attempt_outcome_effects;
use failure::{
    planning_failure, FailureClass, FailureTarget, HealthEffect, LocalAdapterComponent,
    RetryDisposition,
};
use outcome::{
    aggregate_request_costs, AttemptCostSnapshot, AttemptCostStatus, AttemptOutcome,
    AttemptRouteSnapshot, AttemptUsageSnapshot, AttemptUsageStatus, DownstreamDeliveryOutcome,
    FrozenPricingAssessment, FrozenPricingBasis, OutcomeInvariantError, RequestCostAggregateStatus,
    TokenUsage, UpstreamProtocolOutcome,
};
use request::AttemptId;

fn attempt_id(ordinal: u16) -> AttemptId {
    AttemptId::new("req-outcome", ordinal)
}

fn exact_pricing(context: &str, currency: &str) -> FrozenPricingAssessment {
    FrozenPricingAssessment {
        pricing_context_id: context.to_string(),
        basis: FrozenPricingBasis::ExactPrice,
        currency: Some(currency.to_string()),
        input_unit_price_micro: Some(10),
        output_unit_price_micro: Some(20),
        status_label: "exact".to_string(),
    }
}

fn unpriced_pricing(context: &str) -> FrozenPricingAssessment {
    FrozenPricingAssessment {
        pricing_context_id: context.to_string(),
        basis: FrozenPricingBasis::Unpriced,
        currency: None,
        input_unit_price_micro: None,
        output_unit_price_micro: None,
        status_label: "unpriced".to_string(),
    }
}

fn complete_usage() -> AttemptUsageSnapshot {
    AttemptUsageSnapshot {
        status: AttemptUsageStatus::Complete,
        tokens: Some(TokenUsage {
            input_tokens: 10,
            output_tokens: 5,
            total_tokens: 15,
            cache_creation_tokens: None,
            cache_read_tokens: None,
        }),
    }
}

fn missing_usage() -> AttemptUsageSnapshot {
    AttemptUsageSnapshot {
        status: AttemptUsageStatus::MissingUsage,
        tokens: None,
    }
}

fn priced(ordinal: u16, context: &str, currency: &str, amount_micro: i64) -> AttemptCostSnapshot {
    AttemptCostSnapshot::priced(
        attempt_id(ordinal),
        exact_pricing(context, currency),
        complete_usage(),
        amount_micro,
    )
    .expect("priced cost")
}

#[test]
fn attempt_outcome_is_immutable_non_secret_and_keeps_protocol_delivery_separate() {
    let failure = planning_failure(
        FailureClass::Lifecycle,
        FailureTarget::LocalAdapter {
            component: LocalAdapterComponent::Lifecycle,
        },
        RetryDisposition::StopRequest,
    );
    let cost = AttemptCostSnapshot::unavailable(
        attempt_id(0),
        unpriced_pricing("pricing-a"),
        missing_usage(),
        AttemptCostStatus::MissingUsage,
    )
    .expect("unavailable cost");

    let outcome = AttemptOutcome::new(
        attempt_id(0),
        AttemptRouteSnapshot {
            station_id: "station-a".to_string(),
            station_key_id: "key-a".to_string(),
            endpoint_revision: 7,
        },
        UpstreamProtocolOutcome::Failed {
            failure,
            output_committed: false,
        },
        DownstreamDeliveryOutcome {
            terminal: DeliveryTerminal::DownstreamDropped,
            body_bytes: Some(128),
        },
        cost,
    )
    .expect("outcome");

    assert!(matches!(
        outcome.protocol,
        UpstreamProtocolOutcome::Failed {
            output_committed: false,
            ..
        }
    ));
    assert_eq!(
        outcome.delivery.terminal,
        DeliveryTerminal::DownstreamDropped
    );
    assert_eq!(outcome.route.station_key_id, "key-a");
}

#[test]
fn effect_planner_consumes_typed_attempt_outcome_without_reclassifying_strings() {
    let mut failure = planning_failure(
        FailureClass::RateLimited,
        FailureTarget::StationEndpoint {
            station_id: "station-a".to_string(),
            endpoint_revision: 3,
        },
        RetryDisposition::WaitThenReplan,
    );
    failure.health = HealthEffect::Cooldown {
        retry_after_ms: Some(1000),
    };
    let outcome = AttemptOutcome::new(
        attempt_id(1),
        AttemptRouteSnapshot {
            station_id: "station-a".to_string(),
            station_key_id: "key-a".to_string(),
            endpoint_revision: 3,
        },
        UpstreamProtocolOutcome::Failed {
            failure,
            output_committed: false,
        },
        DownstreamDeliveryOutcome {
            terminal: DeliveryTerminal::NotStarted,
            body_bytes: None,
        },
        priced(1, "pricing-b", "USD", 42),
    )
    .expect("outcome");

    let plan = plan_attempt_outcome_effects(&outcome);
    let failure_plan = plan.failure.expect("failure effect");
    assert_eq!(failure_plan.class, FailureClass::RateLimited);
    assert_eq!(failure_plan.retry, RetryDisposition::WaitThenReplan);
    assert_eq!(
        failure_plan.health,
        HealthEffect::Cooldown {
            retry_after_ms: Some(1000)
        }
    );
}

#[test]
fn each_attempt_freezes_its_own_pricing_assessment_and_usage_gap_is_not_zero() {
    let first = AttemptCostSnapshot::unavailable(
        attempt_id(0),
        unpriced_pricing("pricing-first"),
        missing_usage(),
        AttemptCostStatus::MissingUsage,
    )
    .expect("missing usage");
    let second = priced(1, "pricing-second", "USD", 900);

    assert_eq!(first.pricing.pricing_context_id, "pricing-first");
    assert_eq!(second.pricing.pricing_context_id, "pricing-second");
    assert_eq!(first.total_cost_micro, None);
    assert_eq!(first.status, AttemptCostStatus::MissingUsage);
}

#[test]
fn request_aggregate_requires_all_started_attempt_costs_before_terminalizing() {
    let err = aggregate_request_costs(
        &[attempt_id(0), attempt_id(1)],
        &[priced(0, "pricing-a", "USD", 100)],
    )
    .expect_err("missing durable attempt cost");

    assert_eq!(
        err,
        OutcomeInvariantError::MissingDurableAttemptCost {
            attempt_id: attempt_id(1)
        }
    );
}

#[test]
fn request_aggregate_groups_durable_attempt_costs_by_currency_without_double_counting() {
    let aggregate = aggregate_request_costs(
        &[attempt_id(0), attempt_id(1)],
        &[
            priced(0, "pricing-usd", "USD", 100),
            priced(1, "pricing-eur", "EUR", 200),
        ],
    )
    .expect("aggregate");

    assert_eq!(
        aggregate.status,
        RequestCostAggregateStatus::CompleteMixedCurrency {
            currencies: vec!["EUR".to_string(), "USD".to_string()]
        }
    );
    assert_eq!(aggregate.compatibility_total, None);
    assert_eq!(aggregate.totals_by_currency_micro["USD"], 100);
    assert_eq!(aggregate.totals_by_currency_micro["EUR"], 200);
}

#[test]
fn single_currency_aggregate_exposes_compatibility_projection_only_when_unambiguous() {
    let aggregate = aggregate_request_costs(
        &[attempt_id(0), attempt_id(1)],
        &[
            priced(0, "pricing-a", "USD", 100),
            priced(1, "pricing-b", "USD", 200),
        ],
    )
    .expect("aggregate");

    assert_eq!(
        aggregate.status,
        RequestCostAggregateStatus::CompleteSingleCurrency {
            currency: "USD".to_string()
        }
    );
    assert_eq!(
        aggregate
            .compatibility_total
            .expect("single currency")
            .amount_micro,
        300
    );
}

#[test]
fn incomplete_attempt_cost_status_is_preserved_in_request_aggregate() {
    let gap = AttemptCostSnapshot::unavailable(
        attempt_id(1),
        unpriced_pricing("pricing-gap"),
        AttemptUsageSnapshot {
            status: AttemptUsageStatus::StreamUsageMissing,
            tokens: None,
        },
        AttemptCostStatus::StreamUsageMissing,
    )
    .expect("gap");

    let aggregate = aggregate_request_costs(
        &[attempt_id(0), attempt_id(1)],
        &[priced(0, "a", "USD", 10), gap],
    )
    .expect("aggregate");

    assert_eq!(aggregate.status, RequestCostAggregateStatus::Incomplete);
    assert_eq!(aggregate.incomplete_attempts[0].attempt_id, attempt_id(1));
    assert_eq!(
        aggregate.incomplete_attempts[0].status,
        AttemptCostStatus::StreamUsageMissing
    );
    assert_eq!(aggregate.compatibility_total, None);
}
