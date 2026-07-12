pub mod affinity;
pub mod capacity;
pub mod eligibility;
pub mod explanation;
pub mod metrics;
pub mod multiplier;
pub mod scoring;
pub mod selection;
pub mod types;

use crate::{
    models::routing::{RoutingGroupFilter, SchedulerAdvancedSettings},
    services::proxy::scheduler::{
        affinity::{AffinityKind, AffinityStore},
        capacity::{effective_load_capacity, CapacityRegistry},
        eligibility::{evaluate_candidate, group_matches},
        metrics::RuntimeMetricsRegistry,
        scoring::{score_candidates, ScoreInput},
        selection::{
            move_sticky_candidate_to_front, top_k_candidates, weighted_order_without_replacement,
            ScoredCandidate, StickyKind,
        },
        types::{
            CandidateRejectionCode, ScheduleDecision, ScheduleError, ScheduleRequest,
            SchedulerCandidate, SchedulerCandidateDecision,
        },
    },
};

pub fn schedule_once(
    request: &ScheduleRequest,
    candidates: &[SchedulerCandidate],
    metrics: &RuntimeMetricsRegistry,
    capacity: &CapacityRegistry,
    affinity: &mut AffinityStore,
    advanced: &SchedulerAdvancedSettings,
) -> Result<ScheduleDecision, ScheduleError> {
    if candidates.is_empty()
        && !matches!(request.routing_group_filter, RoutingGroupFilter::AllGroups)
    {
        return Err(ScheduleError::new(
            "routing_no_candidate_in_group_scope",
            Vec::new(),
        ));
    }

    let routing_group_scope = explanation::routing_group_scope_key(&request.routing_group_filter);
    let affinity_hit = affinity.resolve(
        &routing_group_scope,
        request.previous_response_id.as_deref(),
        request.session_hash.as_deref(),
        request.now_ms,
    );

    let mut decisions = Vec::with_capacity(candidates.len());
    let mut score_inputs = Vec::new();
    for candidate in candidates {
        let routing_group_match = group_matches(&request.routing_group_filter, candidate);
        match evaluate_candidate(request, candidate) {
            Ok(()) => {
                let capacity_snapshot = capacity.snapshot(&candidate.station_key_id);
                let metrics_snapshot = metrics.snapshot(&candidate.station_key_id);
                score_inputs.push(ScoreInput {
                    station_key_id: candidate.station_key_id.clone(),
                    priority: candidate.priority,
                    effective_multiplier: candidate
                        .effective_multiplier
                        .as_ref()
                        .map(|fact| fact.value)
                        .unwrap_or(1.0),
                    in_flight: capacity_snapshot.in_flight,
                    effective_capacity: effective_load_capacity(0, 0),
                    waiting: capacity_snapshot.waiting,
                    error_rate_ewma: metrics_snapshot.error_rate_ewma,
                    ttft_ms: metrics_snapshot.ttft_ewma_ms,
                    quota_headroom: 1.0,
                });
                decisions.push(SchedulerCandidateDecision {
                    station_key_id: candidate.station_key_id.clone(),
                    station_id: candidate.station_id.clone(),
                    accepted: true,
                    rejection: None,
                    routing_group_match,
                    effective_multiplier: candidate.effective_multiplier.clone(),
                    score: None,
                    factors: Vec::new(),
                    top_k_rank: None,
                    slot_result: None,
                });
            }
            Err(rejection) => {
                decisions.push(SchedulerCandidateDecision {
                    station_key_id: candidate.station_key_id.clone(),
                    station_id: candidate.station_id.clone(),
                    accepted: false,
                    rejection: Some(rejection),
                    routing_group_match,
                    effective_multiplier: candidate.effective_multiplier.clone(),
                    score: None,
                    factors: Vec::new(),
                    top_k_rank: None,
                    slot_result: Some("rejected".to_string()),
                });
            }
        }
    }

    let score_breakdowns = score_candidates(&score_inputs, advanced);
    let mut scored = Vec::with_capacity(score_breakdowns.len());
    for breakdown in score_breakdowns {
        if let Some(decision) = decisions
            .iter_mut()
            .find(|decision| decision.station_key_id == breakdown.station_key_id)
        {
            decision.score = Some(breakdown.score);
            decision.factors =
                explanation::score_factor_labels(&breakdown.factors, advanced, breakdown.score);
        }
        scored.push(ScoredCandidate {
            station_key_id: breakdown.station_key_id.clone(),
            priority: breakdown.priority,
            score: breakdown.score,
            load_rate: breakdown.load_rate,
            waiting: breakdown.waiting,
            sticky_kind: affinity_hit
                .as_ref()
                .filter(|hit| hit.station_key_id == breakdown.station_key_id)
                .map(|hit| match hit.kind {
                    AffinityKind::PreviousResponse => StickyKind::PreviousResponse,
                    AffinityKind::Session => StickyKind::Session,
                }),
        });
    }

    if scored.is_empty() {
        return Err(ScheduleError::new(
            no_candidate_error_code(&decisions, &request.routing_group_filter),
            decisions,
        ));
    }

    let mut top_k = top_k_candidates(&scored, usize::from(advanced.top_k));
    if advanced.sticky_weighted {
        top_k = weighted_order_without_replacement(&top_k, request.now_ms as u64);
    } else {
        move_sticky_candidate_to_front(&mut top_k);
    }

    let mut ordered_station_key_ids = Vec::with_capacity(top_k.len());
    for (index, candidate) in top_k.iter().enumerate() {
        ordered_station_key_ids.push(candidate.station_key_id.clone());
        if let Some(decision) = decisions
            .iter_mut()
            .find(|decision| decision.station_key_id == candidate.station_key_id)
        {
            decision.top_k_rank = Some(index + 1);
            let mut guard = capacity.try_acquire(&candidate.station_key_id, 0);
            decision.slot_result = if guard.acquired() {
                guard.release();
                Some("acquired_simulated".to_string())
            } else {
                Some("slot_unavailable".to_string())
            };
        }
    }

    Ok(ScheduleDecision {
        selected_station_key_id: ordered_station_key_ids.first().cloned(),
        ordered_station_key_ids,
        candidate_decisions: decisions,
    })
}

fn no_candidate_error_code(
    decisions: &[SchedulerCandidateDecision],
    filter: &RoutingGroupFilter,
) -> &'static str {
    if !matches!(filter, RoutingGroupFilter::AllGroups)
        && decisions
            .iter()
            .all(|decision| !decision.routing_group_match)
    {
        return "routing_no_candidate_in_group_scope";
    }

    let matching_decisions = decisions
        .iter()
        .filter(|decision| decision.routing_group_match)
        .collect::<Vec<_>>();
    let all_matching_have_reason = |code| {
        !matching_decisions.is_empty()
            && matching_decisions.iter().all(|decision| {
                decision
                    .rejection
                    .as_ref()
                    .map(|rejection| rejection.reasons.contains(&code))
                    .unwrap_or(false)
            })
    };

    let all_matching_have_multiplier_evidence_rejection = !matching_decisions.is_empty()
        && matching_decisions.iter().all(|decision| {
            decision
                .rejection
                .as_ref()
                .map(|rejection| {
                    rejection
                        .reasons
                        .iter()
                        .any(|reason| multiplier_evidence_rejection_codes().contains(reason))
                })
                .unwrap_or(false)
        });
    let any_matching_has_expired_multiplier_evidence = matching_decisions.iter().any(|decision| {
        decision
            .rejection
            .as_ref()
            .map(|rejection| {
                rejection
                    .reasons
                    .contains(&CandidateRejectionCode::MultiplierEvidenceExpired)
            })
            .unwrap_or(false)
    });
    if all_matching_have_multiplier_evidence_rejection
        && any_matching_has_expired_multiplier_evidence
    {
        return "routing_multiplier_evidence_expired";
    }
    if all_matching_have_multiplier_evidence_rejection {
        return "routing_no_multiplier_evidence";
    }
    if all_matching_have_reason(CandidateRejectionCode::MultiplierOverCeiling) {
        return "routing_no_candidate_within_multiplier_limit";
    }

    "routing_no_eligible_candidate"
}

fn multiplier_evidence_rejection_codes() -> &'static [CandidateRejectionCode] {
    &[
        CandidateRejectionCode::NoMultiplierEvidence,
        CandidateRejectionCode::MultiplierEvidenceInvalid,
        CandidateRejectionCode::MultiplierEvidenceNegative,
        CandidateRejectionCode::MultiplierEvidenceExpired,
        CandidateRejectionCode::MultiplierEvidenceUnboundGroup,
        CandidateRejectionCode::MultiplierEvidenceLowConfidence,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::proxy::scheduler::types::CandidateRejection;

    #[test]
    fn mixed_multiplier_evidence_failures_prefer_expired_error_code() {
        let decisions = vec![
            rejected_decision(
                "expired",
                vec![CandidateRejectionCode::MultiplierEvidenceExpired],
            ),
            rejected_decision(
                "low-confidence",
                vec![CandidateRejectionCode::MultiplierEvidenceLowConfidence],
            ),
        ];

        assert_eq!(
            no_candidate_error_code(&decisions, &RoutingGroupFilter::AllGroups),
            "routing_multiplier_evidence_expired"
        );
    }

    fn rejected_decision(
        station_key_id: &str,
        reasons: Vec<CandidateRejectionCode>,
    ) -> SchedulerCandidateDecision {
        SchedulerCandidateDecision {
            station_key_id: station_key_id.to_string(),
            station_id: format!("station-{station_key_id}"),
            accepted: false,
            rejection: Some(CandidateRejection {
                primary_code: reasons[0],
                reasons,
            }),
            routing_group_match: true,
            effective_multiplier: None,
            score: None,
            factors: Vec::new(),
            top_k_rank: None,
            slot_result: Some("rejected".to_string()),
        }
    }
}
