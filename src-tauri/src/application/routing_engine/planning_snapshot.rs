use crate::models::routing_policy::RoutingPolicyConfigV1;

use super::algorithm_profile::DispatchAlgorithmProfile;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateSnapshot {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) credential_revision: i64,
    pub(crate) credential_available: bool,
    pub(crate) hard_eligible: bool,
    pub(crate) backup_only: bool,
    pub(crate) depleted: bool,
    pub(crate) capability_basis_points: u16,
    pub(crate) reliability_basis_points: u16,
    pub(crate) responsiveness_basis_points: u16,
    pub(crate) cost_basis_points: Option<u16>,
    pub(crate) preference_basis_points: u16,
    pub(crate) failure_domains: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeOverlaySnapshot {
    pub(crate) runtime_instance_id: String,
    pub(crate) runtime_revision: u64,
    pub(crate) candidate_set_revision: u64,
    pub(crate) in_flight: u32,
    pub(crate) max_concurrency: u32,
    /// A request-scoped, validated affinity result captured by the runtime
    /// owner. Keeping only the opaque candidate id here lets the planner
    /// apply affinity as an explicit preference correction without exposing
    /// lookup keys or secrets to durable facts.
    pub(crate) affinity_station_key_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlanningSnapshot {
    pub(crate) snapshot_id: String,
    pub(crate) durable_revision: u64,
    pub(crate) policy: RoutingPolicyConfigV1,
    pub(crate) profile: DispatchAlgorithmProfile,
    pub(crate) candidates: Vec<CandidateSnapshot>,
    pub(crate) runtime: RuntimeOverlaySnapshot,
}

impl PlanningSnapshot {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.snapshot_id.is_empty()
            || self.durable_revision == 0
            || self.candidates.len() > usize::from(self.policy.max_candidates)
            || self.runtime.runtime_instance_id.is_empty()
            || self.runtime.runtime_revision == 0
            || self.runtime.candidate_set_revision == 0
            || self.runtime.in_flight > self.runtime.max_concurrency
        {
            return Err("invalid planning snapshot");
        }
        self.policy.validate()?;
        self.profile.validate()?;
        if self.candidates.iter().any(|candidate| {
            candidate.station_key_id.is_empty()
                || candidate.station_id.is_empty()
                || candidate.endpoint_revision <= 0
                || candidate.credential_revision <= 0
                || candidate.capability_basis_points > 10_000
                || candidate.reliability_basis_points > 10_000
                || candidate.responsiveness_basis_points > 10_000
                || candidate
                    .cost_basis_points
                    .is_some_and(|value| value > 10_000)
                || candidate.preference_basis_points > 10_000
        }) {
            return Err("planning snapshot contains invalid or unavailable candidate");
        }
        Ok(())
    }
}
