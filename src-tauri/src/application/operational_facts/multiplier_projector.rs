use crate::models::operational::{RateMultiplier, RecordRevision, UnixMillis};

use super::group_projector::ProjectionTrace;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MultiplierEvidenceKind {
    BindingLatestUser,
    BindingLatestEffective,
    CurrentUser,
    CurrentEffective,
    CurrentDefault,
    ManualOverride,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MultiplierEvidence {
    pub(crate) kind: MultiplierEvidenceKind,
    pub(crate) multiplier: RateMultiplier,
    pub(crate) authoritative: bool,
    pub(crate) fresh: bool,
    pub(crate) revision: RecordRevision,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MultiplierProjectionInput {
    pub(crate) disabled: bool,
    pub(crate) ambiguous: bool,
    pub(crate) manual_override: Option<MultiplierEvidence>,
    pub(crate) binding_latest_user: Option<MultiplierEvidence>,
    pub(crate) binding_latest_effective: Option<MultiplierEvidence>,
    pub(crate) current_user: Option<MultiplierEvidence>,
    pub(crate) current_effective: Option<MultiplierEvidence>,
    pub(crate) current_default: Option<MultiplierEvidence>,
    pub(crate) resolved_at: UnixMillis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MultiplierResolutionStatus {
    Resolved,
    Disabled,
    Stale,
    Ambiguous,
    Missing,
    Untrusted,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct MultiplierProjection {
    pub(crate) multiplier: Option<RateMultiplier>,
    pub(crate) status: MultiplierResolutionStatus,
    pub(crate) selected_kind: Option<MultiplierEvidenceKind>,
    pub(crate) trace: ProjectionTrace,
}

pub(crate) fn project_multiplier(input: MultiplierProjectionInput) -> MultiplierProjection {
    if input.disabled {
        return unresolved(
            input.resolved_at,
            MultiplierResolutionStatus::Disabled,
            "multiplier_disabled",
        );
    }
    if input.ambiguous {
        return unresolved(
            input.resolved_at,
            MultiplierResolutionStatus::Ambiguous,
            "multiplier_ambiguous",
        );
    }

    let ordered = [
        input.manual_override,
        input.binding_latest_user,
        input.binding_latest_effective,
        input.current_user,
        input.current_effective,
        input.current_default,
    ];
    let Some(evidence) = ordered.into_iter().flatten().next() else {
        return unresolved(
            input.resolved_at,
            MultiplierResolutionStatus::Missing,
            "multiplier_missing",
        );
    };
    if !evidence.authoritative {
        return unresolved(
            input.resolved_at,
            MultiplierResolutionStatus::Untrusted,
            "multiplier_untrusted",
        );
    }
    if !evidence.fresh {
        return unresolved(
            input.resolved_at,
            MultiplierResolutionStatus::Stale,
            "multiplier_stale",
        );
    }

    MultiplierProjection {
        multiplier: Some(evidence.multiplier),
        status: MultiplierResolutionStatus::Resolved,
        selected_kind: Some(evidence.kind),
        trace: ProjectionTrace::new(
            vec!["multiplier_projector", "evidence"],
            crate::models::operational::PriceConfidence::new(1.0).expect("valid confidence"),
            input.resolved_at,
            "multiplier_resolved",
            vec![evidence.revision],
        ),
    }
}

fn unresolved(
    resolved_at: UnixMillis,
    status: MultiplierResolutionStatus,
    reason: &'static str,
) -> MultiplierProjection {
    MultiplierProjection {
        multiplier: None,
        status,
        selected_kind: None,
        trace: ProjectionTrace::new(
            vec!["multiplier_projector"],
            crate::models::operational::PriceConfidence::new(0.0).expect("valid confidence"),
            resolved_at,
            reason,
            Vec::new(),
        ),
    }
}
