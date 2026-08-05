use crate::application::{
    operational_facts::candidate_projector::RouteCandidateProjection,
    routing_engine::request::RouteProgressView,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum HardGate {
    AssetCredential,
    ProtocolModelFeatures,
    Group,
    Tag,
    Health,
    RuntimeGuard,
    Economics,
    ActualAttemptExclusion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteRejection {
    pub(crate) station_key_id: String,
    pub(crate) code: &'static str,
    pub(crate) gate: HardGate,
}

pub(crate) const REASON_CREDENTIAL_MISSING: &str = "credential_missing";
pub(crate) const REASON_CAPABILITY_REJECTED: &str = "capability_rejected";
pub(crate) const REASON_GROUP_MISMATCH: &str = "group_mismatch";
pub(crate) const REASON_HEALTH_HARD_REJECT: &str = "health_hard_reject";

pub(crate) fn evaluate_candidate(
    candidate: &RouteCandidateProjection,
    progress: &RouteProgressView,
) -> Result<(), RouteRejection> {
    if has_code(candidate, "credential_missing") {
        return Err(rejection(
            candidate,
            "credential_missing",
            HardGate::AssetCredential,
        ));
    }
    if has_code(candidate, "capability_rejected") {
        return Err(rejection(
            candidate,
            "capability_rejected",
            HardGate::ProtocolModelFeatures,
        ));
    }
    if has_code(candidate, "group_mismatch") {
        return Err(rejection(candidate, "group_mismatch", HardGate::Group));
    }
    if has_code(candidate, "tag_mismatch") {
        return Err(rejection(candidate, "tag_mismatch", HardGate::Tag));
    }
    if has_code(candidate, "health_hard_reject") {
        return Err(rejection(candidate, "health_hard_reject", HardGate::Health));
    }
    if has_code(candidate, "capacity_unavailable") {
        return Err(rejection(
            candidate,
            "capacity_unavailable",
            HardGate::RuntimeGuard,
        ));
    }
    if let Some(code) = first_economic_code(candidate) {
        return Err(rejection(candidate, code, HardGate::Economics));
    }
    if progress.excludes_station_key(&candidate.identity.station_key_id) {
        return Err(rejection(
            candidate,
            "actual_attempt_excluded",
            HardGate::ActualAttemptExclusion,
        ));
    }
    Ok(())
}

fn has_code(candidate: &RouteCandidateProjection, code: &'static str) -> bool {
    candidate.hard_rejection_codes.contains(&code)
}

fn first_economic_code(candidate: &RouteCandidateProjection) -> Option<&'static str> {
    [
        "multiplier_ceiling",
        "pricing_not_applicable_for_inference",
        "balance_depleted",
    ]
    .into_iter()
    .find(|code| has_code(candidate, code))
}

fn rejection(
    candidate: &RouteCandidateProjection,
    code: &'static str,
    gate: HardGate,
) -> RouteRejection {
    RouteRejection {
        station_key_id: candidate.identity.station_key_id.clone(),
        code,
        gate,
    }
}
