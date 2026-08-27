use crate::models::operational::{BalanceScope, HealthState};
use crate::models::operational::{PriceConfidence, RecordRevision, UnixMillis};
use crate::models::routing::RuntimeRoutingBalance;

use super::group_projector::ProjectionTrace;

pub(crate) const BALANCE_PROJECTOR_VERSION: &str = "balance_scope_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BalanceEvidenceStatus {
    Available,
    Depleted,
    Unknown,
    NotSupported,
    NotApplicable,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BalanceAmount {
    pub(crate) amount: f64,
    pub(crate) currency: String,
}

impl BalanceAmount {
    pub(crate) fn new(amount: f64, currency: impl Into<String>) -> Option<Self> {
        let currency = currency.into();
        (amount.is_finite() && !currency.trim().is_empty()).then_some(Self { amount, currency })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct BalanceObservation {
    pub(crate) scope: BalanceScope,
    pub(crate) status: BalanceEvidenceStatus,
    pub(crate) balance: Option<BalanceAmount>,
    pub(crate) low_balance_threshold: Option<BalanceAmount>,
    pub(crate) authoritative: bool,
    pub(crate) fresh: bool,
    pub(crate) revision: Option<RecordRevision>,
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
        BalanceEvidenceStatus::Depleted => BalanceProjectionStatus::DepletedEmergency,
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
        revision.into_iter().collect(),
    )
}

fn is_depleted(observation: &BalanceObservation) -> bool {
    let Some(balance) = &observation.balance else {
        return false;
    };
    // The configured low-balance threshold is advisory. It must never turn a
    // positive balance into an exhausted routing state.
    balance.amount <= 0.0
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
        trace: ProjectionTrace::for_projector(
            BALANCE_PROJECTOR_VERSION,
            vec!["balance_projector"],
            selected_scope
                .map(|scope| format!("balance_scope:{scope:?}"))
                .into_iter()
                .collect(),
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

/// Projects the runtime balance representation through the same policy used
/// by durable operational-fact observations. Numeric values take precedence
/// over textual provider status, so a positive value remains routeable even
/// when a stale provider status says `depleted` or `exhausted`.
pub(crate) fn project_runtime_balance(
    balance: Option<&RuntimeRoutingBalance>,
    resolved_at: UnixMillis,
) -> BalanceProjection {
    let Some(balance) = balance else {
        return project_balance(None, None, resolved_at);
    };

    let observation = BalanceObservation {
        scope: runtime_scope(&balance.scope),
        // A finite numeric value is the authoritative spendability signal;
        // textual provider states may be stale or contradictory.
        status: if balance.value.is_some_and(f64::is_finite) {
            BalanceEvidenceStatus::Available
        } else {
            runtime_evidence_status(&balance.status)
        },
        balance: balance
            .value
            .and_then(|value| BalanceAmount::new(value, balance.currency.clone())),
        low_balance_threshold: balance
            .low_balance_threshold
            .and_then(|value| BalanceAmount::new(value, balance.currency.clone())),
        authoritative: true,
        fresh: true,
        // Runtime candidates do not carry a durable record revision. The
        // projection trace therefore records no revision and remains clearly
        // distinct from the durable operational-facts path.
        revision: None,
    };

    match observation.scope {
        BalanceScope::StationKey => project_balance(Some(observation), None, resolved_at),
        BalanceScope::StationAccount => project_balance(None, Some(observation), resolved_at),
        BalanceScope::Unknown => balance_projection(
            resolved_at,
            BalanceProjectionStatus::Missing,
            None,
            "balance_scope_unknown",
            Vec::new(),
        ),
    }
}

fn runtime_scope(scope: &str) -> BalanceScope {
    match scope.trim().to_ascii_lowercase().as_str() {
        "station_key" | "key" => BalanceScope::StationKey,
        "station_account" | "station" | "account" => BalanceScope::StationAccount,
        _ => BalanceScope::Unknown,
    }
}

fn runtime_evidence_status(status: &str) -> BalanceEvidenceStatus {
    match status.trim().to_ascii_lowercase().as_str() {
        "normal" | "available" | "usable" | "low" | "warning" => BalanceEvidenceStatus::Available,
        "depleted" | "exhausted" | "empty" => BalanceEvidenceStatus::Depleted,
        "unsupported" | "not_supported" => BalanceEvidenceStatus::NotSupported,
        "not_applicable" | "n/a" => BalanceEvidenceStatus::NotApplicable,
        _ => BalanceEvidenceStatus::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observation(
        scope: BalanceScope,
        status: BalanceEvidenceStatus,
        value: Option<f64>,
        revision: i64,
    ) -> BalanceObservation {
        BalanceObservation {
            scope,
            status,
            balance: value.and_then(|value| BalanceAmount::new(value, "USD")),
            low_balance_threshold: None,
            authoritative: true,
            fresh: true,
            revision: Some(RecordRevision::new(revision).expect("revision")),
        }
    }

    #[test]
    fn key_scope_takes_precedence_over_station_scope() {
        let projection = project_balance(
            Some(observation(
                BalanceScope::StationKey,
                BalanceEvidenceStatus::Available,
                Some(3.0),
                2,
            )),
            Some(observation(
                BalanceScope::StationAccount,
                BalanceEvidenceStatus::Available,
                Some(0.0),
                1,
            )),
            UnixMillis::new(1).expect("timestamp"),
        );

        assert_eq!(projection.status, BalanceProjectionStatus::Healthy);
        assert_eq!(projection.selected_scope, Some(BalanceScope::StationKey));
    }

    #[test]
    fn positive_numeric_balance_wins_over_depleted_text_status() {
        let balance = RuntimeRoutingBalance {
            scope: "station_key".to_string(),
            value: Some(4.71),
            currency: "USD".to_string(),
            low_balance_threshold: Some(15.0),
            status: "depleted".to_string(),
            collected_at: None,
        };

        let projection =
            project_runtime_balance(Some(&balance), UnixMillis::new(1).expect("timestamp"));

        assert_eq!(projection.status, BalanceProjectionStatus::Healthy);
        assert_eq!(projection.selected_scope, Some(BalanceScope::StationKey));
    }

    #[test]
    fn zero_or_negative_numeric_balance_is_depleted() {
        for value in [Some(0.0), Some(-1.0)] {
            let balance = RuntimeRoutingBalance {
                scope: "station_key".to_string(),
                value,
                currency: "USD".to_string(),
                low_balance_threshold: None,
                status: "normal".to_string(),
                collected_at: None,
            };

            let projection =
                project_runtime_balance(Some(&balance), UnixMillis::new(1).expect("timestamp"));

            assert_eq!(
                projection.status,
                BalanceProjectionStatus::DepletedEmergency
            );
        }
    }
}
