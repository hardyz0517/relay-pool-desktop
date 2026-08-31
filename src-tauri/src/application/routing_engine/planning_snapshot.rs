use crate::application::model_mapping::CandidateModelVariant;
use crate::application::routing_policy::AttemptBudgetProfileV1;
use crate::models::model_mapping::FallbackTrigger;
use crate::models::operational::MAX_OPERATIONAL_CANDIDATES;
use crate::models::routing_policy::RoutingPolicyConfigV2;

use super::{
    algorithm_profile::DispatchAlgorithmProfile, candidate_plan::RoutePlanPricingSnapshot,
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
    pub(crate) credential_available: bool,
    pub(crate) hard_eligible: bool,
    pub(crate) backup_only: bool,
    pub(crate) depleted: bool,
    pub(crate) capability_basis_points: u16,
    /// False means reliability and responsiveness must be omitted from score
    /// normalization. The numeric fields remain populated for stable tracing
    /// but are not scoring inputs in that state.
    pub(crate) quality_available: bool,
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
    /// Number of configured station keys before request capability and static
    /// execution gates. These three counts are captured from the same durable
    /// read and drive terminal classification without re-querying mutable data.
    pub(crate) configured_key_count: usize,
    pub(crate) capability_match_count: usize,
    pub(crate) candidate_cap_count: usize,
    pub(crate) routing_runtime_generation_id: Option<String>,
    pub(crate) routing_generation_fence_revision: u64,
    pub(crate) routing_policy_revision: u64,
    pub(crate) routing_quality_revision: u64,
    pub(crate) routing_health_revision: u64,
    pub(crate) quality_projection_backlog: u64,
    pub(crate) quality_projection_lag_seconds: u64,
    pub(crate) quality_stale: bool,
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
            || self.capability_match_count > self.configured_key_count
            || self.candidate_cap_count > self.capability_match_count
            || self.candidates.len() > self.candidate_cap_count
            || self.candidate_cap_count > MAX_OPERATIONAL_CANDIDATES
            || self.candidates.len() > MAX_OPERATIONAL_CANDIDATES
            || self.runtime.runtime_instance_id.is_empty()
            || self.runtime.runtime_revision == 0
            || self.runtime.candidate_set_revision == 0
            || self.runtime.in_flight > self.runtime.max_concurrency
            || (self.routing_runtime_generation_id.is_some()
                && (self.routing_quality_revision == 0 || self.routing_health_revision == 0))
            || (self.quality_stale != (self.quality_projection_backlog > 0))
            || (!self.quality_stale && self.quality_projection_lag_seconds > 0)
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
                })
        }) {
            return Err("planning snapshot contains invalid or unavailable candidate");
        }
        Ok(())
    }
}
