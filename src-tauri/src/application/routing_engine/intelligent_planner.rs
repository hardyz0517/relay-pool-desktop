use crate::models::routing_policy::RoutingPolicyConfigV1;

use super::{
    dispatch::{weighted_rendezvous, DispatchCandidate, DispatchDecision},
    exploration::{choose_lane, derive_seed, ExplorationBudgetRegistry, ExplorationLane},
    factors::cost_score,
    fixed_point::{BasisPoints, FactorContribution, UtilityScore},
    planning_snapshot::{CandidateSnapshot, PlanningSnapshot},
    tiers::{classify_tier, AvailabilityTier},
};

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
pub(crate) enum PlannerError {
    InvalidSnapshot(&'static str),
    NoEligibleCandidate,
    RuntimeAtCapacity,
}

#[cfg(test)]
pub(crate) fn plan_snapshot(
    snapshot: &PlanningSnapshot,
    root_seed: &[u8],
    round: u64,
) -> Result<RoutePlan, PlannerError> {
    plan_snapshot_with_budget(snapshot, root_seed, round, None)
}

pub(crate) fn plan_snapshot_with_budget(
    snapshot: &PlanningSnapshot,
    root_seed: &[u8],
    round: u64,
    exploration_budget: Option<&ExplorationBudgetRegistry>,
) -> Result<RoutePlan, PlannerError> {
    snapshot.validate().map_err(PlannerError::InvalidSnapshot)?;
    if snapshot.runtime.in_flight >= snapshot.runtime.max_concurrency {
        return Err(PlannerError::RuntimeAtCapacity);
    }
    let mut planned = snapshot
        .candidates
        .iter()
        .filter_map(|candidate| {
            planned_candidate(
                candidate,
                &snapshot.policy,
                snapshot.runtime.affinity_station_key_id.as_deref(),
            )
        })
        .collect::<Vec<_>>();
    if planned.is_empty() {
        return Err(PlannerError::NoEligibleCandidate);
    }
    planned.sort_by(|left, right| {
        right
            .utility
            .value()
            .cmp(&left.utility.value())
            .then_with(|| left.station_key_id.cmp(&right.station_key_id))
    });
    let best_tier = planned
        .iter()
        .map(|candidate| candidate.tier)
        .min()
        .expect("not empty");
    let seed = derive_seed(root_seed, snapshot.profile.seed_domain, round);
    // Exploration is constrained to the best eligible tier. Only advertise
    // the lane when that tier actually has an unknown-cost candidate; using a
    // lower-priority unknown here can reserve budget and then produce an empty
    // dispatch set even though the request has eligible candidates.
    let unknown_exists = planned.iter().any(|candidate| {
        candidate.tier == best_tier
            && snapshot
                .candidates
                .iter()
                .find(|raw| raw.station_key_id == candidate.station_key_id)
                .is_some_and(|raw| raw.cost_basis_points.is_none())
    });
    let lane = exploration_budget
        .map(|budget| {
            choose_lane(
                &seed,
                snapshot.profile.exploration_share_basis_points,
                unknown_exists,
                budget,
            )
        })
        .unwrap_or(ExplorationLane::Exploit);
    let dispatch_candidates = planned
        .iter()
        .filter(|candidate| candidate.tier == best_tier)
        .filter(|candidate| {
            lane != ExplorationLane::Explore
                || snapshot
                    .candidates
                    .iter()
                    .find(|raw| raw.station_key_id == candidate.station_key_id)
                    .is_some_and(|raw| raw.cost_basis_points.is_none())
        })
        .map(|candidate| DispatchCandidate {
            id: candidate.station_key_id.clone(),
            utility: candidate.utility.value(),
            tier: candidate.tier,
            failure_domains: snapshot
                .candidates
                .iter()
                .find(|raw| raw.station_key_id == candidate.station_key_id)
                .map(|raw| raw.failure_domains.clone())
                .unwrap_or_default(),
        })
        .collect::<Vec<_>>();
    let affinity_dispatch = snapshot
        .policy
        .affinity_enabled
        .then(|| snapshot.runtime.affinity_station_key_id.as_deref())
        .flatten()
        .and_then(|affinity_id| {
            dispatch_candidates
                .iter()
                .find(|candidate| candidate.id == affinity_id)
                .cloned()
        });
    let mut dispatch = if let Some(affinity_candidate) = affinity_dispatch {
        // A validated live affinity is a deterministic preference correction,
        // not a probabilistic hint. It still stays inside the hard-eligible
        // best tier and therefore cannot bypass safety gates.
        weighted_rendezvous(
            std::slice::from_ref(&affinity_candidate),
            &seed,
            snapshot.profile.exploit_band_basis_points,
        )
    } else {
        weighted_rendezvous(
            &dispatch_candidates,
            &seed,
            snapshot.profile.exploit_band_basis_points,
        )
    }
    .ok_or(PlannerError::NoEligibleCandidate)?;
    dispatch.explored = lane == ExplorationLane::Explore;
    Ok(RoutePlan {
        snapshot_id: snapshot.snapshot_id.clone(),
        selected_station_key_id: dispatch.selected_id.clone(),
        candidates: planned,
        dispatch,
    })
}

fn planned_candidate(
    candidate: &CandidateSnapshot,
    policy: &RoutingPolicyConfigV1,
    affinity_station_key_id: Option<&str>,
) -> Option<PlannedCandidate> {
    if !candidate.hard_eligible {
        return None;
    }
    let tier = classify_tier(
        candidate.credential_available,
        candidate.reliability_basis_points >= 1_000,
        candidate.depleted,
        policy.allow_depleted_fallback,
    )?;
    let tier = if candidate.backup_only {
        AvailabilityTier::Backup
    } else {
        tier
    };
    if candidate.capability_basis_points == 0 {
        return None;
    }
    let preference = if policy.affinity_enabled
        && affinity_station_key_id == Some(candidate.station_key_id.as_str())
    {
        10_000
    } else {
        candidate.preference_basis_points
    };
    let scores = [
        candidate.reliability_basis_points,
        candidate.responsiveness_basis_points,
        cost_score(candidate.cost_basis_points).get(),
        preference,
    ];
    let weights = [
        policy.reliability_weight,
        policy.responsiveness_weight,
        policy.cost_weight,
        policy.preference_weight,
    ];
    let mut total = BasisPoints::ZERO;
    let contributions = std::array::from_fn(|index| {
        let weight = BasisPoints::new(weights[index]).expect("validated policy");
        let score = BasisPoints::new(scores[index]).expect("validated snapshot");
        let contribution = weight.checked_mul(score).unwrap_or(BasisPoints::ZERO);
        total = total.checked_add(contribution).unwrap_or(BasisPoints::FULL);
        FactorContribution {
            weight,
            score,
            contribution,
        }
    });
    Some(PlannedCandidate {
        station_key_id: candidate.station_key_id.clone(),
        tier,
        utility: UtilityScore::new(total),
        contributions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile, candidate_plan::RoutePlanPricingSnapshot,
        planning_snapshot::RuntimeOverlaySnapshot,
    };
    #[test]
    fn planner_accepts_only_a_snapshot_and_replays_deterministically() {
        let snapshot = PlanningSnapshot {
            snapshot_id: "snapshot-1".into(),
            durable_revision: 1,
            routing_policy_revision: 1,
            policy: RoutingPolicyConfigV1::default(),
            profile: DispatchAlgorithmProfile::default(),
            candidates: vec![CandidateSnapshot {
                station_key_id: "key-a".into(),
                station_id: "station-a".into(),
                endpoint_revision: 1,
                credential_revision: 1,
                account_revision: 1,
                group_binding_id: None,
                group_revision: None,
                resolved_upstream_model: Some("gpt-test".into()),
                model_alias_revision: 1,
                capacity_domain: None,
                capacity_domain_revision: None,
                credential_available: true,
                hard_eligible: true,
                backup_only: false,
                depleted: false,
                capability_basis_points: 10_000,
                reliability_basis_points: 8_000,
                responsiveness_basis_points: 8_000,
                cost_basis_points: Some(8_000),
                pricing: RoutePlanPricingSnapshot::unpriced("test"),
                preference_basis_points: 5_000,
                failure_domains: vec!["station-a".into()],
            }],
            runtime: RuntimeOverlaySnapshot {
                runtime_instance_id: "runtime-1".into(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 1,
                affinity_station_key_id: None,
            },
        };
        assert_eq!(
            plan_snapshot(&snapshot, b"seed", 1),
            plan_snapshot(&snapshot, b"seed", 1)
        );
    }

    #[test]
    fn affinity_is_an_explicit_preference_correction() {
        let mut policy = RoutingPolicyConfigV1::default();
        policy.reliability_weight = 0;
        policy.responsiveness_weight = 0;
        policy.cost_weight = 0;
        policy.preference_weight = 10_000;
        policy.affinity_enabled = true;
        let mut snapshot = PlanningSnapshot {
            snapshot_id: "affinity".into(),
            durable_revision: 1,
            routing_policy_revision: 1,
            policy,
            profile: DispatchAlgorithmProfile::default(),
            candidates: vec![
                CandidateSnapshot {
                    station_key_id: "ordinary".into(),
                    station_id: "station-a".into(),
                    endpoint_revision: 1,
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: Some("gpt-test".into()),
                    model_alias_revision: 1,
                    capacity_domain: None,
                    capacity_domain_revision: None,
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: false,
                    capability_basis_points: 10_000,
                    reliability_basis_points: 5_000,
                    responsiveness_basis_points: 5_000,
                    cost_basis_points: None,
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 9_000,
                    failure_domains: vec![],
                },
                CandidateSnapshot {
                    station_key_id: "sticky".into(),
                    station_id: "station-b".into(),
                    endpoint_revision: 1,
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: Some("gpt-test".into()),
                    model_alias_revision: 1,
                    capacity_domain: None,
                    capacity_domain_revision: None,
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: false,
                    capability_basis_points: 10_000,
                    reliability_basis_points: 5_000,
                    responsiveness_basis_points: 5_000,
                    cost_basis_points: None,
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 1_000,
                    failure_domains: vec![],
                },
            ],
            runtime: RuntimeOverlaySnapshot {
                runtime_instance_id: "runtime".into(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 1,
                affinity_station_key_id: Some("sticky".into()),
            },
        };
        let plan = plan_snapshot(&snapshot, b"seed", 1).unwrap();
        assert_eq!(plan.candidates[0].station_key_id, "sticky");

        snapshot.runtime.affinity_station_key_id = None;
        let without_affinity = plan_snapshot(&snapshot, b"seed", 1).unwrap();
        assert_eq!(without_affinity.candidates[0].station_key_id, "ordinary");
    }

    #[test]
    fn exploration_does_not_empty_the_best_tier_for_lower_tier_unknown_cost() {
        let profile = DispatchAlgorithmProfile::default();
        let budget = ExplorationBudgetRegistry::new(1);
        let snapshot = PlanningSnapshot {
            snapshot_id: "mixed-cost-tiers".into(),
            durable_revision: 1,
            routing_policy_revision: 1,
            policy: RoutingPolicyConfigV1::default(),
            profile,
            candidates: vec![
                CandidateSnapshot {
                    station_key_id: "primary-known".into(),
                    station_id: "station-primary".into(),
                    endpoint_revision: 1,
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: Some("gpt-test".into()),
                    model_alias_revision: 1,
                    capacity_domain: None,
                    capacity_domain_revision: None,
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: false,
                    capability_basis_points: 10_000,
                    reliability_basis_points: 9_000,
                    responsiveness_basis_points: 9_000,
                    cost_basis_points: Some(8_000),
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 5_000,
                    failure_domains: vec![],
                },
                CandidateSnapshot {
                    station_key_id: "backup-unknown".into(),
                    station_id: "station-backup".into(),
                    endpoint_revision: 1,
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: Some("gpt-test".into()),
                    model_alias_revision: 1,
                    capacity_domain: None,
                    capacity_domain_revision: None,
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: true,
                    depleted: false,
                    capability_basis_points: 10_000,
                    reliability_basis_points: 9_000,
                    responsiveness_basis_points: 9_000,
                    cost_basis_points: None,
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 5_000,
                    failure_domains: vec![],
                },
            ],
            runtime: RuntimeOverlaySnapshot {
                runtime_instance_id: "runtime".into(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 4,
                affinity_station_key_id: None,
            },
        };

        let plan = plan_snapshot_with_budget(&snapshot, b"seed", 1, Some(&budget))
            .expect("known primary candidate must remain dispatchable");
        assert_eq!(plan.selected_station_key_id, "primary-known");
        assert!(!plan.dispatch.explored);
        assert_eq!(budget.remaining(), 1, "no exploration token was reserved");
    }
}
