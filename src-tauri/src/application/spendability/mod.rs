//! Canonical consumer-admission decisions shared by monitoring and routing.
//!
//! Balance values are deliberately not interpreted here.  A collector or a
//! trusted provider classifier must first emit one of the closed status values
//! below.  In particular, a key-level depleted observation is not sufficient
//! to pause a target when a current station/subscription observation says the
//! account is usable.

use crate::models::pricing::BalanceSnapshot;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpendabilityState {
    Usable,
    Low,
    Depleted,
    Unknown,
    NotSupported,
    NotApplicable,
}

impl SpendabilityState {
    pub(crate) fn is_pauseable(self) -> bool {
        matches!(self, Self::Depleted)
    }

    pub(crate) fn is_usable_override(self) -> bool {
        matches!(self, Self::Usable | Self::Low)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SpendabilityScope {
    Station,
    StationKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SampleExclusionReason {
    BalanceDepleted,
    SubscriptionUnavailable,
    QuotaExhausted,
    Cancelled,
    Interrupted,
    LocalConfiguration,
    LocalBudget,
    LocalInternalBeforeSend,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TechnicalHealthEffect {
    Positive,
    Negative,
    Neutral,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ProbeSampleDisposition {
    pub(crate) availability_eligible: bool,
    pub(crate) latency_eligible: bool,
    pub(crate) health_effect: TechnicalHealthEffect,
    pub(crate) exclusion_reason: Option<SampleExclusionReason>,
}

impl ProbeSampleDisposition {
    pub(crate) const fn eligible() -> Self {
        Self {
            availability_eligible: true,
            latency_eligible: true,
            health_effect: TechnicalHealthEffect::Positive,
            exclusion_reason: None,
        }
    }

    pub(crate) const fn excluded(reason: SampleExclusionReason) -> Self {
        Self {
            availability_eligible: false,
            latency_eligible: false,
            health_effect: TechnicalHealthEffect::Neutral,
            exclusion_reason: Some(reason),
        }
    }
}

/// Returns the current spendability state for a monitor target.
///
/// `station` represents the account/subscription pool.  It intentionally
/// takes precedence over a key observation when it is explicitly usable or
/// low, because the key balance is only one funding dimension of the target.
pub(crate) fn resolve_balance_spendability(
    target_scope: SpendabilityScope,
    key: Option<&BalanceSnapshot>,
    station: Option<&BalanceSnapshot>,
) -> SpendabilityState {
    let station_state = station.map(balance_status);
    let key_state = key.map(balance_status);

    match target_scope {
        SpendabilityScope::Station => station_state.unwrap_or(SpendabilityState::Unknown),
        SpendabilityScope::StationKey => match station_state {
            Some(state) if state.is_usable_override() => state,
            Some(SpendabilityState::Depleted) => SpendabilityState::Depleted,
            Some(SpendabilityState::NotSupported) => {
                key_state.unwrap_or(SpendabilityState::NotSupported)
            }
            Some(SpendabilityState::NotApplicable) => {
                key_state.unwrap_or(SpendabilityState::NotApplicable)
            }
            Some(SpendabilityState::Unknown) | None => {
                key_state.unwrap_or(SpendabilityState::Unknown)
            }
            Some(SpendabilityState::Low) | Some(SpendabilityState::Usable) => {
                unreachable!("usable station states are handled above")
            }
        },
    }
}

fn balance_status(snapshot: &BalanceSnapshot) -> SpendabilityState {
    match snapshot.status.trim().to_ascii_lowercase().as_str() {
        "normal" | "available" | "usable" => SpendabilityState::Usable,
        "low" | "warning" => SpendabilityState::Low,
        "depleted" | "exhausted" | "empty" => SpendabilityState::Depleted,
        "not_supported" | "unsupported" => SpendabilityState::NotSupported,
        "not_applicable" | "n_a" => SpendabilityState::NotApplicable,
        _ => SpendabilityState::Unknown,
    }
}

pub(crate) fn sample_disposition(failure_kind: Option<&str>) -> ProbeSampleDisposition {
    match failure_kind.map(|value| value.trim().to_ascii_lowercase()) {
        Some(kind) if matches!(kind.as_str(), "budget_exceeded" | "balance_depleted") => {
            ProbeSampleDisposition::excluded(SampleExclusionReason::BalanceDepleted)
        }
        Some(kind)
            if matches!(
                kind.as_str(),
                "subscription_unavailable" | "quota_exhausted"
            ) =>
        {
            ProbeSampleDisposition::excluded(SampleExclusionReason::QuotaExhausted)
        }
        Some(kind) if matches!(kind.as_str(), "cancelled") => {
            ProbeSampleDisposition::excluded(SampleExclusionReason::Cancelled)
        }
        Some(kind) if matches!(kind.as_str(), "interrupted") => {
            ProbeSampleDisposition::excluded(SampleExclusionReason::Interrupted)
        }
        Some(kind)
            if matches!(
                kind.as_str(),
                "needs_configuration" | "invalid_request" | "local_configuration"
            ) =>
        {
            ProbeSampleDisposition::excluded(SampleExclusionReason::LocalConfiguration)
        }
        Some(kind) if matches!(kind.as_str(), "budget_exceeded_local" | "local_budget") => {
            ProbeSampleDisposition::excluded(SampleExclusionReason::LocalBudget)
        }
        _ => ProbeSampleDisposition::eligible(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(status: &str) -> BalanceSnapshot {
        BalanceSnapshot {
            id: "balance".into(),
            station_id: "station".into(),
            station_key_id: None,
            scope: "station".into(),
            value: None,
            currency: "USD".into(),
            credit_unit: None,
            used_value: None,
            total_value: None,
            today_request_count: None,
            total_request_count: None,
            today_consumption: None,
            total_consumption: None,
            today_base_consumption: None,
            total_base_consumption: None,
            today_token_count: None,
            total_token_count: None,
            today_input_token_count: None,
            today_output_token_count: None,
            total_input_token_count: None,
            total_output_token_count: None,
            account_concurrency_limit: None,
            low_balance_threshold: None,
            status: status.into(),
            source: "fixture".into(),
            confidence: 1.0,
            collected_at: None,
            created_at: "2026-01-01".into(),
            updated_at: "2026-01-01".into(),
        }
    }

    #[test]
    fn usable_subscription_does_not_pause_negative_key() {
        let key = snapshot("depleted");
        let station = snapshot("normal");
        assert_eq!(
            resolve_balance_spendability(SpendabilityScope::StationKey, Some(&key), Some(&station)),
            SpendabilityState::Usable
        );
    }

    #[test]
    fn depleted_key_pauses_without_station_override() {
        let key = snapshot("depleted");
        assert_eq!(
            resolve_balance_spendability(SpendabilityScope::StationKey, Some(&key), None),
            SpendabilityState::Depleted
        );
    }

    #[test]
    fn unknown_and_unsupported_never_pause() {
        for status in ["unknown", "not_supported", "not_applicable", "legacy"] {
            let station = snapshot(status);
            assert!(!resolve_balance_spendability(
                SpendabilityScope::Station,
                None,
                Some(&station)
            )
            .is_pauseable());
        }
    }

    #[test]
    fn business_balance_failures_are_excluded_and_neutral() {
        let disposition = sample_disposition(Some("budget_exceeded"));
        assert!(!disposition.availability_eligible);
        assert!(!disposition.latency_eligible);
        assert_eq!(disposition.health_effect, TechnicalHealthEffect::Neutral);
    }
}
