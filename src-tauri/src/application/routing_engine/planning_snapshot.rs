use crate::application::model_mapping::CandidateModelVariant;
use crate::application::routing_policy::AttemptBudgetProfileV1;
use crate::models::model_mapping::FallbackTrigger;
use crate::models::routing_policy::RoutingPolicyConfigV2;

use super::{
    algorithm_profile::DispatchAlgorithmProfile, candidate_plan::RoutePlanPricingSnapshot,
    failure_domains::CapacityDomainCommitment,
};

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateSnapshot {
    pub(crate) station_key_id: String,
    pub(crate) station_id: String,
    pub(crate) endpoint_revision: i64,
    pub(crate) credential_revision: i64,
    pub(crate) account_revision: i64,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_revision: Option<i64>,
    pub(crate) resolved_upstream_model: Option<String>,
    pub(crate) model_alias_revision: i64,
    /// All model variants resolved from the same immutable mapping snapshot.
    /// Empty is retained for compatibility with old test fixtures and means
    /// the candidate has one implicit variant from `resolved_upstream_model`.
    pub(crate) model_variants: Vec<CandidateModelVariant>,
    /// Opaque commitment from explicit station_capacity_domains facts. A
    /// missing value prohibits cross-domain capacity fallback.
    pub(crate) capacity_domain: Option<CapacityDomainCommitment>,
    pub(crate) capacity_domain_revision: Option<i64>,
    pub(crate) credential_available: bool,
    pub(crate) hard_eligible: bool,
    pub(crate) backup_only: bool,
    pub(crate) depleted: bool,
    pub(crate) capability_basis_points: u16,
    pub(crate) reliability_basis_points: u16,
    pub(crate) responsiveness_basis_points: u16,
    pub(crate) cost_basis_points: Option<u16>,
    pub(crate) pricing: RoutePlanPricingSnapshot,
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PlanningSnapshot {
    pub(crate) snapshot_id: String,
    pub(crate) durable_revision: u64,
    pub(crate) routing_policy_revision: u64,
    pub(crate) policy: RoutingPolicyConfigV2,
    /// Request-local reliability budget compiled with the policy revision.
    /// Replanning must not reconstruct or reset this value.
    pub(crate) attempt_budget: AttemptBudgetProfileV1,
    pub(crate) profile: DispatchAlgorithmProfile,
    pub(crate) candidates: Vec<CandidateSnapshot>,
    pub(crate) model_fallback_trigger: Option<FallbackTrigger>,
    pub(crate) runtime: RuntimeOverlaySnapshot,
}

impl PlanningSnapshot {
    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if self.snapshot_id.is_empty()
            || self.durable_revision == 0
            || self.routing_policy_revision == 0
            || self.attempt_budget.policy_revision != self.routing_policy_revision
            || self.candidates.len() > usize::from(self.policy.max_candidates)
            || self.runtime.runtime_instance_id.is_empty()
            || self.runtime.runtime_revision == 0
            || self.runtime.candidate_set_revision == 0
            || self.runtime.in_flight > self.runtime.max_concurrency
        {
            return Err("invalid planning snapshot");
        }
        self.policy
            .validate()
            .map_err(|_| "invalid planning policy")?;
        self.profile.validate()?;
        if self.candidates.iter().any(|candidate| {
            candidate.station_key_id.is_empty()
                || candidate.station_id.is_empty()
                || candidate.endpoint_revision <= 0
                || candidate.credential_revision <= 0
                || candidate.account_revision <= 0
                || candidate
                    .group_revision
                    .is_some_and(|revision| revision <= 0)
                || candidate.group_binding_id.is_some() != candidate.group_revision.is_some()
                || candidate.model_alias_revision <= 0
                || candidate
                    .capacity_domain_revision
                    .is_some_and(|revision| revision <= 0)
                || candidate.capacity_domain.is_some()
                    != candidate.capacity_domain_revision.is_some()
                || candidate.capability_basis_points > 10_000
                || candidate.reliability_basis_points > 10_000
                || candidate.responsiveness_basis_points > 10_000
                || candidate
                    .cost_basis_points
                    .is_some_and(|value| value > 10_000)
                || candidate.pricing.status_label.is_empty()
                || [
                    candidate.pricing.estimated_input_price,
                    candidate.pricing.estimated_output_price,
                ]
                .into_iter()
                .flatten()
                .any(|value| !value.is_finite() || value < 0.0)
                || candidate.preference_basis_points > 10_000
                || candidate.model_variants.iter().any(|variant| {
                    variant.station_key_id != candidate.station_key_id
                        || variant.station_id != candidate.station_id
                        || variant.upstream_model.is_empty()
                        || variant.credential_revision <= 0
                        || variant.endpoint_revision <= 0
                })
        }) {
            return Err("planning snapshot contains invalid or unavailable candidate");
        }
        Ok(())
    }
}
