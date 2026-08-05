//! UI-only asset status composition.
//!
//! This reducer deliberately consumes already projected facts. It never writes a
//! canonical row and is not an input to the routing planner.

use super::{
    balance_projector::BalanceProjectionStatus, capability_projector::CapabilityDecision,
    group_projector::GroupVerdict, multiplier_projector::MultiplierResolutionStatus,
    pricing_projector::PricingVerdict,
};
use crate::models::operational::UnixMillis;

pub(crate) const ASSET_STATUS_PROJECTOR_VERSION: &str = "asset_status_rollup_v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AssetStatus {
    Healthy,
    Degraded,
    Blocked,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AssetStatusRollup {
    pub(crate) status: AssetStatus,
    pub(crate) reason_code: &'static str,
    pub(crate) source_refs: Vec<&'static str>,
    pub(crate) observed_at: Option<UnixMillis>,
    pub(crate) projector_version: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AssetStatusInput {
    pub(crate) group: GroupVerdict,
    pub(crate) pricing: PricingVerdict,
    pub(crate) balance: BalanceProjectionStatus,
    pub(crate) capability: CapabilityDecision,
    pub(crate) multiplier: MultiplierResolutionStatus,
    pub(crate) observed_at: Option<UnixMillis>,
}

pub(crate) fn project_asset_status(input: AssetStatusInput) -> AssetStatusRollup {
    let mut source_refs = Vec::with_capacity(5);
    if matches!(input.group, GroupVerdict::Disabled | GroupVerdict::Invalid) {
        return rollup(
            input.observed_at,
            AssetStatus::Blocked,
            "group_blocked",
            vec!["group"],
        );
    }
    if matches!(input.capability, CapabilityDecision::Reject) {
        return rollup(
            input.observed_at,
            AssetStatus::Blocked,
            "capability_rejected",
            vec!["capability"],
        );
    }
    if matches!(input.balance, BalanceProjectionStatus::DepletedEmergency) {
        return rollup(
            input.observed_at,
            AssetStatus::Blocked,
            "balance_depleted",
            vec!["balance"],
        );
    }
    if matches!(input.multiplier, MultiplierResolutionStatus::Ambiguous) {
        return rollup(
            input.observed_at,
            AssetStatus::Blocked,
            "multiplier_ambiguous",
            vec!["multiplier"],
        );
    }
    if matches!(input.group, GroupVerdict::Missing)
        || matches!(
            input.pricing,
            PricingVerdict::Invalid | PricingVerdict::Ambiguous
        )
        || matches!(
            input.capability,
            CapabilityDecision::RequireStrictConfirmation
        )
    {
        source_refs.extend(["group", "pricing", "capability"]);
        return rollup(
            input.observed_at,
            AssetStatus::Unknown,
            "evidence_incomplete",
            source_refs,
        );
    }
    if matches!(
        input.balance,
        BalanceProjectionStatus::Unknown
            | BalanceProjectionStatus::Stale
            | BalanceProjectionStatus::Untrusted
            | BalanceProjectionStatus::Missing
    ) || matches!(
        input.multiplier,
        MultiplierResolutionStatus::Missing
            | MultiplierResolutionStatus::Stale
            | MultiplierResolutionStatus::Untrusted
    ) || matches!(
        input.pricing,
        PricingVerdict::Unpriced | PricingVerdict::Stale
    ) {
        source_refs.extend(["balance", "multiplier", "pricing"]);
        return rollup(
            input.observed_at,
            AssetStatus::Degraded,
            "evidence_degraded",
            source_refs,
        );
    }
    source_refs.extend(["group", "pricing", "balance", "capability", "multiplier"]);
    rollup(
        input.observed_at,
        AssetStatus::Healthy,
        "evidence_healthy",
        source_refs,
    )
}

fn rollup(
    observed_at: Option<UnixMillis>,
    status: AssetStatus,
    reason_code: &'static str,
    source_refs: Vec<&'static str>,
) -> AssetStatusRollup {
    AssetStatusRollup {
        status,
        reason_code,
        source_refs,
        observed_at,
        projector_version: ASSET_STATUS_PROJECTOR_VERSION,
    }
}
