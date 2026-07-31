use crate::models::operational::{
    BalanceScope, HealthState, Money, PriceConfidence, RecordRevision, UnixMillis,
};

use super::group_projector::ProjectionTrace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BalanceEvidenceStatus {
    Available,
    Unknown,
    NotSupported,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BalanceObservation {
    pub(crate) scope: BalanceScope,
    pub(crate) status: BalanceEvidenceStatus,
    pub(crate) balance: Option<Money>,
    pub(crate) low_balance_threshold: Option<Money>,
    pub(crate) authoritative: bool,
    pub(crate) fresh: bool,
    pub(crate) revision: RecordRevision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BalanceProjectionStatus {
    Healthy,
    DepletedEmergency,
    Unknown,
    NotSupported,
    NotApplicable,
    Stale,
    Untrusted,
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BalanceProjection {
    pub(crate) status: BalanceProjectionStatus,
    pub(crate) selected_scope: Option<BalanceScope>,
    pub(crate) health_hint: HealthState,
    pub(crate) trace: ProjectionTrace,
}

pub(crate) fn project_balance(
    key_scope: Option<BalanceObservation>,
    station_scope: Option<BalanceObservation>,
    resolved_at: UnixMillis,
) -> BalanceProjection {
    let selected = key_scope
        .filter(|observation| observation.scope == BalanceScope::StationKey)
        .or_else(|| {
            station_scope.filter(|observation| observation.scope == BalanceScope::StationAccount)
        });
    let Some(observation) = selected else {
        return balance_projection(
            resolved_at,
            BalanceProjectionStatus::Missing,
            None,
            "balance_missing",
            Vec::new(),
        );
    };

    let status = match observation.status {
        BalanceEvidenceStatus::Unknown => BalanceProjectionStatus::Unknown,
        BalanceEvidenceStatus::NotSupported => BalanceProjectionStatus::NotSupported,
        BalanceEvidenceStatus::NotApplicable => BalanceProjectionStatus::NotApplicable,
        BalanceEvidenceStatus::Available if !observation.authoritative => {
            BalanceProjectionStatus::Untrusted
        }
        BalanceEvidenceStatus::Available if !observation.fresh => BalanceProjectionStatus::Stale,
        BalanceEvidenceStatus::Available => {
            if is_depleted(&observation) {
                BalanceProjectionStatus::DepletedEmergency
            } else {
                BalanceProjectionStatus::Healthy
            }
        }
    };
    let scope = Some(observation.scope);
    let revision = observation.revision;
    balance_projection(
        resolved_at,
        status,
        scope,
        balance_reason(status),
        vec![revision],
    )
}

fn is_depleted(observation: &BalanceObservation) -> bool {
    let Some(balance) = &observation.balance else {
        return false;
    };
    let Some(threshold) = &observation.low_balance_threshold else {
        return false;
    };
    balance.currency() == threshold.currency()
        && balance.balance_amount_for_comparison() <= threshold.balance_amount_for_comparison()
}

trait MoneyCompare {
    fn balance_amount_for_comparison(&self) -> f64;
}

impl MoneyCompare for Money {
    fn balance_amount_for_comparison(&self) -> f64 {
        self.amount().get()
    }
}

fn balance_projection(
    resolved_at: UnixMillis,
    status: BalanceProjectionStatus,
    selected_scope: Option<BalanceScope>,
    reason: &'static str,
    revision_refs: Vec<RecordRevision>,
) -> BalanceProjection {
    BalanceProjection {
        status,
        selected_scope,
        health_hint: if status == BalanceProjectionStatus::DepletedEmergency {
            HealthState::Degraded
        } else {
            HealthState::Unknown
        },
        trace: ProjectionTrace::new(
            vec!["balance_projector"],
            PriceConfidence::new(if revision_refs.is_empty() { 0.0 } else { 1.0 })
                .expect("valid confidence"),
            resolved_at,
            reason,
            revision_refs,
        ),
    }
}

fn balance_reason(status: BalanceProjectionStatus) -> &'static str {
    match status {
        BalanceProjectionStatus::Healthy => "balance_healthy",
        BalanceProjectionStatus::DepletedEmergency => "balance_depleted",
        BalanceProjectionStatus::Unknown => "balance_unknown",
        BalanceProjectionStatus::NotSupported => "balance_not_supported",
        BalanceProjectionStatus::NotApplicable => "balance_not_applicable",
        BalanceProjectionStatus::Stale => "balance_stale",
        BalanceProjectionStatus::Untrusted => "balance_untrusted",
        BalanceProjectionStatus::Missing => "balance_missing",
    }
}
