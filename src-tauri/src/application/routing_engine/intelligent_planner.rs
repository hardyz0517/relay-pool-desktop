use crate::application::model_mapping::CandidateModelVariant;
use crate::models::routing_policy::RoutingPolicyConfigV2;
use sha2::Digest;

use super::{
    dispatch::DispatchDecision,
    factors::cost_score,
    fixed_point::{BasisPoints, FactorContribution, UtilityScore},
    planning_snapshot::{CandidateSnapshot, PlanningSnapshot},
    tiers::{classify_tier, AvailabilityTier},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PlannedCandidate {
    pub(crate) station_key_id: String,
    pub(crate) lifecycle_revision: i64,
    pub(crate) routing_identity: String,
    pub(crate) target_rank: u16,
    pub(crate) variant: Option<CandidateModelVariant>,
    pub(crate) tier: AvailabilityTier,
    pub(crate) base_utility: UtilityScore,
    pub(crate) utility: UtilityScore,
    pub(crate) affinity_bonus: BasisPoints,
    pub(crate) affinity_applied: bool,
    pub(crate) contributions: [FactorContribution; 4],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CandidateScoreBreakdown {
    pub(crate) total: u16,
    pub(crate) factors: [FactorContribution; 4],
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

pub(crate) fn plan_snapshot(
    snapshot: &PlanningSnapshot,
    _root_seed: &[u8],
    _round: u64,
) -> Result<RoutePlan, PlannerError> {
    snapshot.validate().map_err(PlannerError::InvalidSnapshot)?;
    if snapshot.runtime.in_flight >= snapshot.runtime.max_concurrency {
        return Err(PlannerError::RuntimeAtCapacity);
    }
    let mut planned = snapshot
        .candidates
        .iter()
        .flat_map(|candidate| {
            let variants = if candidate.model_variants.is_empty() {
                vec![None]
            } else {
                candidate.model_variants.iter().cloned().map(Some).collect()
            };
            variants
                .into_iter()
                .filter_map(move |variant| planned_candidate(candidate, variant, &snapshot.policy))
        })
        .collect::<Vec<_>>();
    if planned.is_empty() {
        return Err(PlannerError::NoEligibleCandidate);
    }
    apply_affinity_correction(
        &mut planned,
        &snapshot.candidates,
        &snapshot.policy,
        &snapshot.profile,
        snapshot.runtime.affinity_station_key_id.as_deref(),
    );
    planned.sort_by(|left, right| {
        left.target_rank
            .cmp(&right.target_rank)
            .then_with(|| left.tier.cmp(&right.tier))
            .then_with(|| right.utility.value().cmp(&left.utility.value()))
            .then_with(|| left.station_key_id.cmp(&right.station_key_id))
            .then_with(|| left.routing_identity.cmp(&right.routing_identity))
    });
    let best_rank = planned
        .first()
        .map(|candidate| candidate.target_rank)
        .ok_or(PlannerError::NoEligibleCandidate)?;
    let best_tier = planned
        .iter()
        .filter(|candidate| candidate.target_rank == best_rank)
        .map(|candidate| candidate.tier)
        .min()
        .ok_or(PlannerError::NoEligibleCandidate)?;
    let selected = planned
        .iter()
        .find(|candidate| candidate.target_rank == best_rank && candidate.tier == best_tier)
        .ok_or(PlannerError::NoEligibleCandidate)?;
    // Production routing is score ordered.  Keep the dispatch shape for
    // tracing/compatibility, but make the selected identity the first item in
    // the already deterministic order.  No random exploration or rendezvous
    // draw is allowed to move a lower-scoring key ahead of it.
    let seed_commitment = sha2::Sha256::digest(snapshot.snapshot_id.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let dispatch = DispatchDecision {
        selected_id: selected.routing_identity.clone(),
        band_size: 1,
        explored: false,
        seed_commitment,
    };
    Ok(RoutePlan {
        snapshot_id: snapshot.snapshot_id.clone(),
        selected_station_key_id: dispatch.selected_id.clone(),
        candidates: planned,
        dispatch,
    })
}

fn planned_candidate(
    candidate: &CandidateSnapshot,
    variant: Option<CandidateModelVariant>,
    policy: &RoutingPolicyConfigV2,
) -> Option<PlannedCandidate> {
    if !candidate.hard_eligible {
        return None;
    }
    let tier = classify_tier(
        candidate.credential_available,
        true,
        candidate.depleted,
        policy.allow_depleted_fallback,
    )?;
    let tier = match (tier, candidate.backup_only) {
        (AvailabilityTier::DepletedEmergency, _) => AvailabilityTier::DepletedEmergency,
        (_, true) => AvailabilityTier::ConfiguredBackup,
        (tier, false) => tier,
    };
    if candidate.capability_basis_points == 0 {
        return None;
    }
    let (total, contributions) = weighted_score_components(candidate, policy, None)?;
    let target_rank = variant.as_ref().map(|value| value.target_rank).unwrap_or(0);
    let routing_identity = variant
        .as_ref()
        .map(CandidateModelVariant::identity_key)
        .unwrap_or_else(|| candidate.station_key_id.clone());
    Some(PlannedCandidate {
        station_key_id: candidate.station_key_id.clone(),
        lifecycle_revision: candidate.credential_revision,
        routing_identity,
        target_rank,
        variant,
        tier,
        base_utility: UtilityScore::new(total),
        utility: UtilityScore::new(total),
        affinity_bonus: BasisPoints::ZERO,
        affinity_applied: false,
        contributions,
    })
}

fn apply_affinity_correction(
    planned: &mut [PlannedCandidate],
    candidates: &[CandidateSnapshot],
    policy: &RoutingPolicyConfigV2,
    profile: &super::algorithm_profile::DispatchAlgorithmProfile,
    affinity_station_key_id: Option<&str>,
) {
    if !policy.affinity_enabled {
        return;
    }
    let Some(affinity_station_key_id) = affinity_station_key_id else {
        return;
    };
    let Some(best_rank) = planned.iter().map(|candidate| candidate.target_rank).min() else {
        return;
    };
    let Some(best_tier) = planned
        .iter()
        .filter(|candidate| candidate.target_rank == best_rank)
        .map(|candidate| candidate.tier)
        .min()
    else {
        return;
    };
    if !planned.iter().any(|candidate| {
        candidate.station_key_id == affinity_station_key_id
            && candidate.target_rank == best_rank
            && candidate.tier == best_tier
    }) {
        return;
    }
    let Some(affinity_candidate) = candidates
        .iter()
        .find(|candidate| candidate.station_key_id == affinity_station_key_id)
    else {
        return;
    };
    let layer_candidates = planned
        .iter()
        .filter(|candidate| candidate.target_rank == best_rank && candidate.tier == best_tier)
        .filter_map(|planned| {
            candidates
                .iter()
                .find(|candidate| candidate.station_key_id == planned.station_key_id)
        })
        .collect::<Vec<_>>();
    if affinity_candidate.quality_available {
        let best_reliability = layer_candidates
            .iter()
            .filter(|candidate| candidate.quality_available)
            .map(|candidate| candidate.reliability_basis_points)
            .max()
            .unwrap_or(affinity_candidate.reliability_basis_points);
        let best_responsiveness = layer_candidates
            .iter()
            .filter(|candidate| candidate.quality_available)
            .map(|candidate| candidate.responsiveness_basis_points)
            .max()
            .unwrap_or(affinity_candidate.responsiveness_basis_points);
        let margin = profile.affinity_hysteresis_margin_basis_points;
        if best_reliability.saturating_sub(affinity_candidate.reliability_basis_points) > margin
            || best_responsiveness.saturating_sub(affinity_candidate.responsiveness_basis_points)
                > margin
        {
            return;
        }
    }

    for candidate in planned.iter_mut().filter(|candidate| {
        candidate.station_key_id == affinity_station_key_id
            && candidate.target_rank == best_rank
            && candidate.tier == best_tier
    }) {
        let bonus_value = profile
            .affinity_bonus_cap_basis_points
            .min(10_000_u16.saturating_sub(candidate.base_utility.value().get()));
        let Some(bonus) = BasisPoints::new(bonus_value) else {
            continue;
        };
        let Some(effective) = candidate.base_utility.value().checked_add(bonus) else {
            continue;
        };
        candidate.affinity_bonus = bonus;
        candidate.affinity_applied = bonus_value > 0;
        candidate.utility = UtilityScore::new(effective);
    }
}

pub(crate) fn candidate_score_breakdown_with_cost_basis(
    candidate: &CandidateSnapshot,
    policy: &RoutingPolicyConfigV2,
    cost_basis_override: Option<u16>,
) -> Option<CandidateScoreBreakdown> {
    weighted_score_components(candidate, policy, cost_basis_override).map(|(score, factors)| {
        CandidateScoreBreakdown {
            total: score.get(),
            factors,
        }
    })
}

fn weighted_score_components(
    candidate: &CandidateSnapshot,
    policy: &RoutingPolicyConfigV2,
    cost_basis_override: Option<u16>,
) -> Option<(BasisPoints, [FactorContribution; 4])> {
    let cost = cost_score(cost_basis_override.or(candidate.cost_basis_points));
    let scores = [
        candidate.reliability_basis_points,
        candidate.responsiveness_basis_points,
        cost.map(BasisPoints::get).unwrap_or(0),
        candidate.preference_basis_points,
    ];
    let weights = [
        policy.reliability_weight,
        policy.responsiveness_weight,
        policy.cost_weight,
        policy.preference_weight,
    ];
    let available = [
        candidate.quality_available,
        candidate.quality_available,
        cost.is_some(),
        true,
    ];
    let effective_weights = normalized_available_weights(weights, available)?;
    let configured_weight_sum = weights
        .iter()
        .zip(available)
        .filter(|(_, available)| *available)
        .try_fold(0_u64, |sum, (weight, _)| {
            sum.checked_add(u64::from(*weight))
        })?;
    let total = if configured_weight_sum == 0 {
        BasisPoints::ZERO
    } else {
        let numerator = scores
            .iter()
            .zip(weights)
            .zip(available)
            .filter(|(_, available)| *available)
            .try_fold(0_u64, |sum, ((score, weight), _)| {
                sum.checked_add(u64::from(*score) * u64::from(weight))
            })?;
        let rounded = numerator
            .checked_add(configured_weight_sum / 2)?
            .checked_div(configured_weight_sum)?
            .min(10_000);
        BasisPoints::new(u16::try_from(rounded).ok()?)?
    };
    let contributions = std::array::from_fn(|index| {
        let weight = BasisPoints::new(effective_weights[index]).expect("normalized weight");
        let score = BasisPoints::new(scores[index]).expect("validated snapshot");
        let contribution_value =
            (u64::from(weight.get()) * u64::from(score.get()) + 5_000) / 10_000;
        let contribution =
            BasisPoints::new(contribution_value.min(10_000) as u16).unwrap_or(BasisPoints::ZERO);
        FactorContribution {
            weight,
            score,
            contribution,
        }
    });
    Some((total, contributions))
}

fn normalized_available_weights(configured: [u16; 4], available: [bool; 4]) -> Option<[u16; 4]> {
    let total = configured
        .iter()
        .zip(available)
        .filter(|(_, available)| *available)
        .try_fold(0_u64, |sum, (weight, _)| {
            sum.checked_add(u64::from(*weight))
        })?;
    if total == 0 {
        return Some([0; 4]);
    }
    let mut normalized = [0_u16; 4];
    let mut remainders = [0_u64; 4];
    let mut allocated = 0_u64;
    for index in 0..4 {
        if !available[index] || configured[index] == 0 {
            continue;
        }
        let scaled = u64::from(configured[index]).checked_mul(10_000)?;
        let quotient = scaled / total;
        normalized[index] = u16::try_from(quotient).ok()?;
        remainders[index] = scaled % total;
        allocated = allocated.checked_add(quotient)?;
    }
    let mut remainder_units = 10_000_u64.checked_sub(allocated)?;
    let mut awarded = [false; 4];
    while remainder_units > 0 {
        let index = (0..4)
            .filter(|index| available[*index] && configured[*index] > 0 && !awarded[*index])
            .max_by(|left, right| {
                remainders[*left]
                    .cmp(&remainders[*right])
                    .then_with(|| right.cmp(left))
            })?;
        normalized[index] = normalized[index].checked_add(1)?;
        awarded[index] = true;
        remainder_units -= 1;
    }
    Some(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::{
        algorithm_profile::DispatchAlgorithmProfile, candidate_plan::RoutePlanPricingSnapshot,
        planning_snapshot::RuntimeOverlaySnapshot,
    };

    fn scoring_candidate(station_key_id: &str) -> CandidateSnapshot {
        CandidateSnapshot {
            station_key_id: station_key_id.to_string(),
            station_id: format!("station-{station_key_id}"),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: Some("gpt-test".into()),
            model_alias_revision: 1,
            model_variants: Vec::new(),
            credential_available: true,
            hard_eligible: true,
            backup_only: false,
            depleted: false,
            capability_basis_points: 10_000,
            quality_available: true,
            reliability_basis_points: 8_000,
            responsiveness_basis_points: 8_000,
            cost_basis_points: Some(8_000),
            pricing: RoutePlanPricingSnapshot::unpriced("test"),
            preference_basis_points: 2_000,
            failure_domains: Vec::new(),
        }
    }

    #[test]
    fn unavailable_quality_is_omitted_and_remaining_weights_are_renormalized() {
        let mut candidate = scoring_candidate("quality-unavailable");
        candidate.quality_available = false;
        let policy = RoutingPolicyConfigV2::default();

        let (score, contributions) =
            weighted_score_components(&candidate, &policy, None).expect("fallback score");

        assert_eq!(score.get(), 5_429);
        assert_eq!(contributions[0].weight.get(), 0);
        assert_eq!(contributions[1].weight.get(), 0);
        assert_eq!(contributions[2].weight.get(), 5_714);
        assert_eq!(contributions[3].weight.get(), 4_286);
    }

    #[test]
    fn unavailable_cost_is_omitted_and_remaining_weights_are_renormalized() {
        let mut candidate = scoring_candidate("cost-unavailable");
        candidate.cost_basis_points = None;

        let (score, contributions) =
            weighted_score_components(&candidate, &RoutingPolicyConfigV2::default(), None)
                .expect("score from remaining factors");

        assert_ne!(score.get(), 5_000);
        assert_eq!(contributions[2].weight, BasisPoints::ZERO);
        assert_eq!(contributions[2].contribution, BasisPoints::ZERO);
        assert_eq!(
            contributions
                .iter()
                .map(|factor| u32::from(factor.weight.get()))
                .sum::<u32>(),
            10_000,
        );
    }

    #[test]
    fn no_available_positive_weight_keeps_a_stable_zero_score_candidate() {
        let mut candidate = scoring_candidate("fallback");
        candidate.quality_available = false;
        candidate.reliability_basis_points = 0;
        let mut policy = RoutingPolicyConfigV2::default();
        policy.reliability_weight = 5_000;
        policy.responsiveness_weight = 5_000;
        policy.cost_weight = 0;
        policy.preference_weight = 0;

        let planned = planned_candidate(&candidate, None, &policy)
            .expect("quality-unavailable candidate remains sortable");

        assert_eq!(planned.utility.value(), BasisPoints::ZERO);
        assert!(planned.contributions.iter().all(|factor| {
            factor.weight == BasisPoints::ZERO && factor.contribution == BasisPoints::ZERO
        }));
    }

    #[test]
    fn planner_accepts_only_a_snapshot_and_replays_deterministically() {
        let snapshot = PlanningSnapshot {
            snapshot_id: "snapshot-1".into(),
            durable_revision: 1,
            configured_key_count: 1,
            capability_match_count: 1,
            candidate_cap_count: 1,
            routing_runtime_generation_id: None,
            routing_generation_fence_revision: 0,
            routing_policy_revision: 1,
            routing_quality_revision: 0,
            routing_health_revision: 0,
            quality_projection_backlog: 0,
            quality_projection_lag_seconds: 0,
            quality_stale: false,
            policy: RoutingPolicyConfigV2::default(),
            attempt_budget:
                crate::application::routing_policy::AttemptBudgetProfileV1::from_policy(
                    1,
                    &crate::models::routing_policy::RetryFailoverPolicyV2::default(),
                )
                .expect("attempt budget"),
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
                model_variants: Vec::new(),
                credential_available: true,
                hard_eligible: true,
                backup_only: false,
                depleted: false,
                capability_basis_points: 10_000,
                quality_available: true,
                reliability_basis_points: 8_000,
                responsiveness_basis_points: 8_000,
                cost_basis_points: Some(8_000),
                pricing: RoutePlanPricingSnapshot::unpriced("test"),
                preference_basis_points: 5_000,
                failure_domains: vec!["station-a".into()],
            }],
            model_fallback_trigger: None,
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
        let mut policy = RoutingPolicyConfigV2::default();
        policy.reliability_weight = 0;
        policy.responsiveness_weight = 0;
        policy.cost_weight = 0;
        policy.preference_weight = 10_000;
        policy.affinity_enabled = true;
        let mut snapshot = PlanningSnapshot {
            snapshot_id: "affinity".into(),
            durable_revision: 1,
            configured_key_count: 2,
            capability_match_count: 2,
            candidate_cap_count: 2,
            routing_runtime_generation_id: None,
            routing_generation_fence_revision: 0,
            routing_policy_revision: 1,
            routing_quality_revision: 0,
            routing_health_revision: 0,
            quality_projection_backlog: 0,
            quality_projection_lag_seconds: 0,
            quality_stale: false,
            policy,
            attempt_budget:
                crate::application::routing_policy::AttemptBudgetProfileV1::from_policy(
                    1,
                    &crate::models::routing_policy::RetryFailoverPolicyV2::default(),
                )
                .expect("attempt budget"),
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
                    model_variants: Vec::new(),
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: false,
                    capability_basis_points: 10_000,
                    quality_available: true,
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
                    model_variants: Vec::new(),
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: false,
                    capability_basis_points: 10_000,
                    quality_available: true,
                    reliability_basis_points: 5_000,
                    responsiveness_basis_points: 5_000,
                    cost_basis_points: None,
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 8_900,
                    failure_domains: vec![],
                },
            ],
            model_fallback_trigger: None,
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
        let sticky = plan
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == "sticky")
            .expect("sticky candidate");
        assert_eq!(sticky.base_utility.value().get(), 8_900);
        assert_eq!(sticky.affinity_bonus.get(), 150);
        assert_eq!(sticky.utility.value().get(), 9_050);
        assert!(sticky.affinity_applied);

        snapshot.candidates[1].reliability_basis_points = 4_000;
        let at_hysteresis_boundary = plan_snapshot(&snapshot, b"seed", 1).unwrap();
        assert_eq!(
            at_hysteresis_boundary.candidates[0].station_key_id,
            "sticky"
        );

        snapshot.candidates[1].reliability_basis_points = 3_999;
        let escaped = plan_snapshot(&snapshot, b"seed", 1).unwrap();
        assert_eq!(escaped.candidates[0].station_key_id, "ordinary");
        let escaped_sticky = escaped
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == "sticky")
            .expect("escaped sticky candidate");
        assert_eq!(escaped_sticky.base_utility, escaped_sticky.utility);
        assert_eq!(escaped_sticky.affinity_bonus, BasisPoints::ZERO);
        assert!(!escaped_sticky.affinity_applied);

        snapshot.candidates[1].reliability_basis_points = 5_000;
        snapshot.runtime.affinity_station_key_id = None;
        let without_affinity = plan_snapshot(&snapshot, b"seed", 1).unwrap();
        assert_eq!(without_affinity.candidates[0].station_key_id, "ordinary");
    }

    #[test]
    fn lower_tier_unknown_cost_never_displaces_the_best_tier() {
        let profile = DispatchAlgorithmProfile::default();
        let mut policy = RoutingPolicyConfigV2::default();
        policy.allow_depleted_fallback = true;
        let snapshot = PlanningSnapshot {
            snapshot_id: "mixed-cost-tiers".into(),
            durable_revision: 1,
            configured_key_count: 3,
            capability_match_count: 3,
            candidate_cap_count: 3,
            routing_runtime_generation_id: None,
            routing_generation_fence_revision: 0,
            routing_policy_revision: 1,
            routing_quality_revision: 0,
            routing_health_revision: 0,
            quality_projection_backlog: 0,
            quality_projection_lag_seconds: 0,
            quality_stale: false,
            policy,
            attempt_budget:
                crate::application::routing_policy::AttemptBudgetProfileV1::from_policy(
                    1,
                    &crate::models::routing_policy::RetryFailoverPolicyV2::default(),
                )
                .expect("attempt budget"),
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
                    model_variants: Vec::new(),
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: false,
                    capability_basis_points: 10_000,
                    quality_available: true,
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
                    model_variants: Vec::new(),
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: true,
                    depleted: false,
                    capability_basis_points: 10_000,
                    quality_available: true,
                    reliability_basis_points: 9_000,
                    responsiveness_basis_points: 9_000,
                    cost_basis_points: None,
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 5_000,
                    failure_domains: vec![],
                },
                CandidateSnapshot {
                    station_key_id: "depleted-emergency".into(),
                    station_id: "station-emergency".into(),
                    endpoint_revision: 1,
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: Some("gpt-test".into()),
                    model_alias_revision: 1,
                    model_variants: Vec::new(),
                    credential_available: true,
                    hard_eligible: true,
                    backup_only: false,
                    depleted: true,
                    capability_basis_points: 10_000,
                    quality_available: true,
                    reliability_basis_points: 10_000,
                    responsiveness_basis_points: 10_000,
                    cost_basis_points: Some(10_000),
                    pricing: RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 10_000,
                    failure_domains: vec![],
                },
            ],
            model_fallback_trigger: None,
            runtime: RuntimeOverlaySnapshot {
                runtime_instance_id: "runtime".into(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 4,
                affinity_station_key_id: None,
            },
        };

        let plan = plan_snapshot(&snapshot, b"seed", 1)
            .expect("known primary candidate must remain dispatchable");
        assert_eq!(plan.selected_station_key_id, "primary-known");
        assert_eq!(plan.candidates[0].tier, AvailabilityTier::Primary);
        assert_eq!(plan.candidates[1].tier, AvailabilityTier::ConfiguredBackup);
        assert_eq!(plan.candidates[2].tier, AvailabilityTier::DepletedEmergency);
        assert!(!plan.dispatch.explored);
    }
}
