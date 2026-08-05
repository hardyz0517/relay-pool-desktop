use crate::models::routing_policy::RoutingPolicyConfigV1;

use super::{dispatch::{weighted_rendezvous, DispatchCandidate, DispatchDecision}, exploration::derive_seed, fixed_point::{BasisPoints, FactorContribution, UtilityScore}, planning_snapshot::{CandidateSnapshot, PlanningSnapshot}, tiers::{classify_tier, AvailabilityTier}};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedCandidate {
    pub(crate) station_key_id: String,
    pub(crate) tier: AvailabilityTier,
    pub(crate) utility: UtilityScore,
    pub(crate) contributions: [FactorContribution; 4],
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutePlan {
    pub(crate) snapshot_id: String,
    pub(crate) selected_station_key_id: String,
    pub(crate) candidates: Vec<PlannedCandidate>,
    pub(crate) dispatch: DispatchDecision,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlannerError { InvalidSnapshot(&'static str), NoEligibleCandidate, RuntimeAtCapacity }

pub(crate) fn plan_snapshot(snapshot: &PlanningSnapshot, root_seed: &[u8], round: u64) -> Result<RoutePlan, PlannerError> {
    snapshot.validate().map_err(PlannerError::InvalidSnapshot)?;
    if snapshot.runtime.in_flight >= snapshot.runtime.max_concurrency { return Err(PlannerError::RuntimeAtCapacity); }
    let mut planned = snapshot.candidates.iter().filter_map(|candidate| planned_candidate(candidate, &snapshot.policy)).collect::<Vec<_>>();
    if planned.is_empty() { return Err(PlannerError::NoEligibleCandidate); }
    planned.sort_by(|left, right| right.utility.value().cmp(&left.utility.value()).then_with(|| left.station_key_id.cmp(&right.station_key_id)));
    let best_tier = planned.iter().map(|candidate| candidate.tier).min().expect("not empty");
    let dispatch_candidates = planned.iter().filter(|candidate| candidate.tier == best_tier).map(|candidate| DispatchCandidate { id: candidate.station_key_id.clone(), utility: candidate.utility.value(), tier: candidate.tier, failure_domains: Vec::new() }).collect::<Vec<_>>();
    let seed = derive_seed(root_seed, snapshot.profile.seed_domain, round);
    let dispatch = weighted_rendezvous(&dispatch_candidates, &seed, snapshot.profile.exploit_band_basis_points).ok_or(PlannerError::NoEligibleCandidate)?;
    Ok(RoutePlan { snapshot_id: snapshot.snapshot_id.clone(), selected_station_key_id: dispatch.selected_id.clone(), candidates: planned, dispatch })
}

fn planned_candidate(candidate: &CandidateSnapshot, policy: &RoutingPolicyConfigV1) -> Option<PlannedCandidate> {
    let tier = classify_tier(candidate.credential_available, candidate.reliability_basis_points >= 1_000, candidate.cost_basis_points == Some(0), policy.allow_depleted_fallback)?;
    if candidate.capability_basis_points == 0 { return None; }
    let scores = [candidate.reliability_basis_points, candidate.responsiveness_basis_points, candidate.cost_basis_points.unwrap_or(0), candidate.preference_basis_points];
    let weights = [policy.reliability_weight, policy.responsiveness_weight, policy.cost_weight, policy.preference_weight];
    let mut total = BasisPoints::ZERO;
    let contributions = std::array::from_fn(|index| { let weight = BasisPoints::new(weights[index]).expect("validated policy"); let score = BasisPoints::new(scores[index]).expect("validated snapshot"); let contribution = weight.checked_mul(score).unwrap_or(BasisPoints::ZERO); total = total.checked_add(contribution).unwrap_or(BasisPoints::FULL); FactorContribution { weight, score, contribution } });
    Some(PlannedCandidate { station_key_id: candidate.station_key_id.clone(), tier, utility: UtilityScore::new(total), contributions })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::{algorithm_profile::DispatchAlgorithmProfile, planning_snapshot::RuntimeOverlaySnapshot};
    #[test]
    fn planner_accepts_only_a_snapshot_and_replays_deterministically() {
        let snapshot = PlanningSnapshot { snapshot_id: "snapshot-1".into(), durable_revision: 1, policy: RoutingPolicyConfigV1::default(), profile: DispatchAlgorithmProfile::default(), candidates: vec![CandidateSnapshot { station_key_id: "key-a".into(), station_id: "station-a".into(), endpoint_revision: 1, credential_available: true, capability_basis_points: 10_000, reliability_basis_points: 8_000, responsiveness_basis_points: 8_000, cost_basis_points: Some(8_000), preference_basis_points: 5_000, failure_domains: vec!["station-a".into()] }], runtime: RuntimeOverlaySnapshot { runtime_instance_id: "runtime-1".into(), runtime_revision: 1, candidate_set_revision: 1, in_flight: 0, max_concurrency: 1 } };
        assert_eq!(plan_snapshot(&snapshot, b"seed", 1), plan_snapshot(&snapshot, b"seed", 1));
    }
}
