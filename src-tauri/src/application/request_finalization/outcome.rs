use std::collections::{BTreeMap, BTreeSet};

use crate::application::request_lifecycle::request::AttemptId;
#[cfg(test)]
use crate::application::{
    request_finalization::failure::CanonicalFailure, request_lifecycle::delivery::DeliveryTerminal,
};

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct AttemptOutcome {
    pub(crate) attempt_id: AttemptId,
    pub(crate) route: AttemptRouteSnapshot,
    pub(crate) protocol: UpstreamProtocolOutcome,
    pub(crate) delivery: DownstreamDeliveryOutcome,
    pub(crate) cost: AttemptCostSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct AttemptRouteSnapshot {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
    pub(crate) endpoint_revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub(crate) enum UpstreamProtocolOutcome {
    Succeeded {
        output_committed: bool,
    },
    Failed {
        failure: CanonicalFailure,
        output_committed: bool,
    },
    NotStarted {
        reason: String,
    },
    Interrupted {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg(test)]
pub(crate) struct DownstreamDeliveryOutcome {
    pub(crate) terminal: DeliveryTerminal,
    pub(crate) body_bytes: Option<i64>,
}

#[cfg(test)]
impl AttemptOutcome {
    #[cfg(test)]
    pub(crate) fn new(
        attempt_id: AttemptId,
        route: AttemptRouteSnapshot,
        protocol: UpstreamProtocolOutcome,
        delivery: DownstreamDeliveryOutcome,
        cost: AttemptCostSnapshot,
    ) -> Result<Self, OutcomeInvariantError> {
        if attempt_id != cost.attempt_id {
            return Err(OutcomeInvariantError::CostAttemptMismatch {
                outcome_attempt: attempt_id,
                cost_attempt: cost.attempt_id,
            });
        }
        Ok(Self {
            attempt_id,
            route,
            protocol,
            delivery,
            cost,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FrozenPricingAssessment {
    pub(crate) pricing_context_id: String,
    pub(crate) basis: FrozenPricingBasis,
    pub(crate) currency: Option<String>,
    pub(crate) input_unit_price_micro: Option<i64>,
    pub(crate) output_unit_price_micro: Option<i64>,
    pub(crate) status_label: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrozenPricingBasis {
    ExactPrice,
    MultiplierProxy,
    Unpriced,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptUsageSnapshot {
    pub(crate) status: AttemptUsageStatus,
    pub(crate) tokens: Option<TokenUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.usage-status; owner=application/request_finalization; remove_when=outcome persistence drops stream-specific missing usage states"
    )
)]
pub(crate) enum AttemptUsageStatus {
    Complete,
    MissingUsage,
    StreamUsageMissing,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TokenUsage {
    pub(crate) input_tokens: i64,
    pub(crate) output_tokens: i64,
    pub(crate) total_tokens: i64,
    pub(crate) cache_creation_tokens: Option<i64>,
    pub(crate) cache_read_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptCostSnapshot {
    pub(crate) attempt_id: AttemptId,
    pub(crate) pricing: FrozenPricingAssessment,
    pub(crate) usage: AttemptUsageSnapshot,
    pub(crate) status: AttemptCostStatus,
    pub(crate) total_cost_micro: Option<i64>,
    pub(crate) currency: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.attempt-cost-status; owner=application/request_finalization; remove_when=durable cost aggregation drops stream/unpriced attempt states"
    )
)]
pub(crate) enum AttemptCostStatus {
    Priced,
    MissingUsage,
    StreamUsageMissing,
    Unpriced,
    PricingIncomplete,
    NotApplicable,
}

impl AttemptCostSnapshot {
    pub(crate) fn priced(
        attempt_id: AttemptId,
        pricing: FrozenPricingAssessment,
        usage: AttemptUsageSnapshot,
        total_cost_micro: i64,
    ) -> Result<Self, OutcomeInvariantError> {
        if usage.status != AttemptUsageStatus::Complete || usage.tokens.is_none() {
            return Err(OutcomeInvariantError::PricedCostWithoutCompleteUsage { attempt_id });
        }
        let Some(currency) = pricing.currency.clone().filter(|value| !value.is_empty()) else {
            return Err(OutcomeInvariantError::PricedCostWithoutCurrency { attempt_id });
        };
        Ok(Self {
            attempt_id,
            pricing,
            usage,
            status: AttemptCostStatus::Priced,
            total_cost_micro: Some(total_cost_micro),
            currency: Some(currency),
        })
    }

    pub(crate) fn unavailable(
        attempt_id: AttemptId,
        pricing: FrozenPricingAssessment,
        usage: AttemptUsageSnapshot,
        status: AttemptCostStatus,
    ) -> Result<Self, OutcomeInvariantError> {
        if status == AttemptCostStatus::Priced {
            return Err(OutcomeInvariantError::UnavailableCostMarkedPriced { attempt_id });
        }
        Ok(Self {
            attempt_id,
            pricing,
            usage,
            status,
            total_cost_micro: None,
            currency: None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.outcome-aggregate; owner=application/request_finalization; remove_when=persistence stops reserving canonical outcome aggregate"
    )
)]
pub(crate) struct RequestOutcome {
    pub(crate) request_id: String,
    pub(crate) terminal: RequestOutcomeTerminal,
    pub(crate) selected_attempt_id: Option<AttemptId>,
    pub(crate) started_attempts: Vec<AttemptId>,
    pub(crate) aggregate: RequestCostAggregate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.terminal-outcome; owner=application/request_finalization; remove_when=canonical finalization persistence drops terminal outcome contract"
    )
)]
pub(crate) enum RequestOutcomeTerminal {
    Completed,
    PartialSuccess,
    Failed { code: String },
    Interrupted { reason: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCostAggregate {
    pub(crate) status: RequestCostAggregateStatus,
    pub(crate) totals_by_currency_micro: BTreeMap<String, i64>,
    pub(crate) compatibility_total: Option<MoneyAmount>,
    pub(crate) incomplete_attempts: Vec<AttemptCostGap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestCostAggregateStatus {
    NoAttempts,
    CompleteSingleCurrency { currency: String },
    CompleteMixedCurrency { currencies: Vec<String> },
    Incomplete,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MoneyAmount {
    pub(crate) currency: String,
    pub(crate) amount_micro: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptCostGap {
    pub(crate) attempt_id: AttemptId,
    pub(crate) status: AttemptCostStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutcomeInvariantError {
    #[cfg(test)]
    CostAttemptMismatch {
        outcome_attempt: AttemptId,
        cost_attempt: AttemptId,
    },
    PricedCostWithoutCompleteUsage {
        attempt_id: AttemptId,
    },
    PricedCostWithoutCurrency {
        attempt_id: AttemptId,
    },
    UnavailableCostMarkedPriced {
        attempt_id: AttemptId,
    },
    MissingDurableAttemptCost {
        attempt_id: AttemptId,
    },
    ForeignAttemptCost {
        attempt_id: AttemptId,
    },
    DuplicateAttemptCost {
        attempt_id: AttemptId,
    },
}

pub(crate) fn aggregate_request_costs(
    started_attempts: &[AttemptId],
    durable_costs: &[AttemptCostSnapshot],
) -> Result<RequestCostAggregate, OutcomeInvariantError> {
    if started_attempts.is_empty() {
        return Ok(RequestCostAggregate {
            status: RequestCostAggregateStatus::NoAttempts,
            totals_by_currency_micro: BTreeMap::new(),
            compatibility_total: None,
            incomplete_attempts: Vec::new(),
        });
    }

    let expected = started_attempts.iter().cloned().collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    for snapshot in durable_costs {
        if !expected.contains(&snapshot.attempt_id) {
            return Err(OutcomeInvariantError::ForeignAttemptCost {
                attempt_id: snapshot.attempt_id.clone(),
            });
        }
        if !seen.insert(snapshot.attempt_id.clone()) {
            return Err(OutcomeInvariantError::DuplicateAttemptCost {
                attempt_id: snapshot.attempt_id.clone(),
            });
        }
    }

    for attempt_id in started_attempts {
        if !seen.contains(attempt_id) {
            return Err(OutcomeInvariantError::MissingDurableAttemptCost {
                attempt_id: attempt_id.clone(),
            });
        }
    }

    let mut totals_by_currency_micro = BTreeMap::<String, i64>::new();
    let mut incomplete_attempts = Vec::<AttemptCostGap>::new();
    let mut not_applicable_count = 0usize;

    for snapshot in durable_costs {
        match snapshot.status {
            AttemptCostStatus::Priced => {
                let Some(currency) = snapshot.currency.clone() else {
                    return Err(OutcomeInvariantError::PricedCostWithoutCurrency {
                        attempt_id: snapshot.attempt_id.clone(),
                    });
                };
                let total = snapshot.total_cost_micro.ok_or_else(|| {
                    OutcomeInvariantError::PricedCostWithoutCompleteUsage {
                        attempt_id: snapshot.attempt_id.clone(),
                    }
                })?;
                *totals_by_currency_micro.entry(currency).or_default() += total;
            }
            AttemptCostStatus::NotApplicable => {
                not_applicable_count += 1;
            }
            status => incomplete_attempts.push(AttemptCostGap {
                attempt_id: snapshot.attempt_id.clone(),
                status,
            }),
        }
    }

    let status = if !incomplete_attempts.is_empty() {
        RequestCostAggregateStatus::Incomplete
    } else if totals_by_currency_micro.is_empty() && not_applicable_count == durable_costs.len() {
        RequestCostAggregateStatus::NotApplicable
    } else if totals_by_currency_micro.len() == 1 {
        let currency = totals_by_currency_micro
            .keys()
            .next()
            .expect("one currency")
            .clone();
        RequestCostAggregateStatus::CompleteSingleCurrency { currency }
    } else {
        RequestCostAggregateStatus::CompleteMixedCurrency {
            currencies: totals_by_currency_micro.keys().cloned().collect(),
        }
    };

    let compatibility_total = match &status {
        RequestCostAggregateStatus::CompleteSingleCurrency { currency } => Some(MoneyAmount {
            currency: currency.clone(),
            amount_micro: *totals_by_currency_micro
                .get(currency)
                .expect("single currency total"),
        }),
        _ => None,
    };

    Ok(RequestCostAggregate {
        status,
        totals_by_currency_micro,
        compatibility_total,
        incomplete_attempts,
    })
}
