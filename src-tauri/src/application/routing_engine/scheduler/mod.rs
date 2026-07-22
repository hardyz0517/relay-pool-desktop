pub(crate) mod affinity;
pub(crate) mod capacity;
pub(crate) mod eligibility;
pub(crate) mod explanation;
pub(crate) mod metrics;
#[cfg(test)]
pub(crate) mod multiplier;
pub(crate) mod scoring;
pub(crate) mod selection;
pub(crate) mod types;

use std::sync::Mutex;

use crate::models::routing::{RoutingGroupFilter, SchedulerAdvancedSettings};

use self::{
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
};

#[derive(Debug, Default)]
pub(crate) struct SchedulerRuntimeState {
    metrics: RuntimeMetricsRegistry,
    capacity: CapacityRegistry,
    affinity: Mutex<AffinityStore>,
}

impl SchedulerRuntimeState {
    #[cfg(test)]
    pub(crate) fn try_acquire(
        &self,
        station_key_id: impl Into<String>,
        max_concurrency: i64,
    ) -> capacity::CapacityGuard {
        self.capacity.try_acquire(station_key_id, max_concurrency)
    }

    pub(crate) fn schedule(
        &self,
        request: &ScheduleRequest,
        candidates: &[SchedulerCandidate],
        advanced: &SchedulerAdvancedSettings,
    ) -> Result<ScheduleDecision, ScheduleError> {
        let mut affinity = self
            .affinity
            .lock()
            .expect("scheduler affinity store poisoned");
        schedule_once(
            request,
            candidates,
            &self.metrics,
            &self.capacity,
            &mut affinity,
            advanced,
        )
    }

    #[cfg(test)]
    pub(crate) fn report_result(
        &self,
        station_key_id: impl Into<String>,
        success: bool,
        first_token_ms: Option<i64>,
    ) {
        self.metrics
            .report_result(station_key_id, success, first_token_ms);
    }

    #[cfg(test)]
    pub(crate) fn bind_session(
        &self,
        routing_group_scope: &str,
        session_hash: &str,
        station_key_id: &str,
        now_ms: i64,
        ttl_seconds: i64,
    ) {
        self.affinity
            .lock()
            .expect("scheduler affinity store poisoned")
            .bind_session(
                routing_group_scope,
                session_hash,
                station_key_id,
                now_ms,
                ttl_seconds,
            );
    }
}

pub(crate) fn schedule_once(
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
    let sticky_escape = affinity_hit.as_ref().and_then(|hit| {
        candidates
            .iter()
            .find(|candidate| candidate.station_key_id == hit.station_key_id)
            .and_then(|candidate| {
                sticky_escape_reason(
                    &metrics.snapshot(&hit.station_key_id),
                    &capacity.snapshot(&hit.station_key_id),
                    candidate.max_concurrency,
                    advanced,
                )
            })
            .map(|reason| (hit.station_key_id.clone(), reason))
    });
    let affinity_hit = if sticky_escape.is_some() {
        None
    } else {
        affinity_hit
    };

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
                    effective_capacity: effective_load_capacity(
                        candidate.max_concurrency,
                        candidate.load_factor.unwrap_or(0),
                    ),
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
            if let Some((station_key_id, reason)) = sticky_escape.as_ref() {
                if station_key_id == &breakdown.station_key_id {
                    decision.factors.push(format!("sticky_escape:{reason}"));
                }
            }
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
            let max_concurrency = candidates
                .iter()
                .find(|source| source.station_key_id == candidate.station_key_id)
                .map(|source| source.max_concurrency)
                .unwrap_or(0);
            let mut guard = capacity.try_acquire(&candidate.station_key_id, max_concurrency);
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

fn sticky_escape_reason(
    metrics: &metrics::RuntimeMetricsSnapshot,
    capacity: &capacity::CapacitySnapshot,
    max_concurrency: i64,
    advanced: &SchedulerAdvancedSettings,
) -> Option<&'static str> {
    if !advanced.sticky_escape {
        return None;
    }
    if metrics.has_ttft
        && metrics
            .ttft_ewma_ms
            .is_some_and(|ttft| ttft > advanced.sticky_escape_ttft_ms as f64)
    {
        return Some("ttft");
    }
    if metrics.error_rate_ewma > advanced.sticky_escape_error_rate {
        return Some("error_rate");
    }
    if max_concurrency > 0 && capacity.in_flight >= max_concurrency as u64 {
        return Some("concurrency_full");
    }
    None
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
    let active_matching_decisions = matching_decisions
        .iter()
        .copied()
        .filter(|decision| {
            !decision
                .rejection
                .as_ref()
                .map(|rejection| {
                    rejection.reasons.len() == 1
                        && rejection
                            .reasons
                            .contains(&CandidateRejectionCode::AssetUnavailable)
                })
                .unwrap_or(false)
        })
        .collect::<Vec<_>>();
    let all_active_matching_have_reason = |code| {
        !active_matching_decisions.is_empty()
            && active_matching_decisions.iter().all(|decision| {
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
    if all_active_matching_have_reason(CandidateRejectionCode::MultiplierOverCeiling)
        || all_matching_have_reason(CandidateRejectionCode::MultiplierOverCeiling)
    {
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
    use crate::{
        application::routing_engine::scheduler::types::{
            CandidateRejection, EffectiveMultiplierFact, SchedulerCandidate,
        },
        models::routing::RouteEndpointKind,
    };

    #[test]
    fn ttft_degraded_session_affinity_soft_escapes_without_deleting_binding() {
        let request = sticky_request();
        let candidates = vec![
            eligible_candidate("sticky", 100),
            eligible_candidate("healthy", 0),
        ];
        let metrics = RuntimeMetricsRegistry::default();
        metrics.report_result("sticky", true, Some(20_000));
        let capacity = CapacityRegistry::default();
        let mut affinity = AffinityStore::default();
        affinity.bind_session("all_groups", "session", "sticky", 1_000, 3_600);

        let decision = schedule_once(
            &request,
            &candidates,
            &metrics,
            &capacity,
            &mut affinity,
            &SchedulerAdvancedSettings::default(),
        )
        .expect("eligible candidates");

        assert_eq!(decision.selected_station_key_id.as_deref(), Some("healthy"));
        assert_eq!(
            affinity.lookup_session("all_groups", "session", 1_001),
            Some("sticky".to_string())
        );
    }

    #[test]
    fn error_degraded_session_affinity_soft_escapes() {
        let request = sticky_request();
        let candidates = vec![
            eligible_candidate("sticky", 100),
            eligible_candidate("healthy", 0),
        ];
        let metrics = RuntimeMetricsRegistry::default();
        metrics.report_result("sticky", false, None);
        let capacity = CapacityRegistry::default();
        let mut affinity = AffinityStore::default();
        affinity.bind_session("all_groups", "session", "sticky", 1_000, 3_600);

        let decision = schedule_once(
            &request,
            &candidates,
            &metrics,
            &capacity,
            &mut affinity,
            &SchedulerAdvancedSettings::default(),
        )
        .expect("eligible candidates");

        assert_eq!(decision.selected_station_key_id.as_deref(), Some("healthy"));
    }

    #[test]
    fn concurrency_full_session_affinity_soft_escapes() {
        let request = sticky_request();
        let mut sticky = eligible_candidate("sticky", 100);
        sticky.max_concurrency = 1;
        let candidates = vec![sticky, eligible_candidate("healthy", 0)];
        let metrics = RuntimeMetricsRegistry::default();
        let capacity = CapacityRegistry::default();
        let _guard = capacity.try_acquire("sticky", 1);
        let mut affinity = AffinityStore::default();
        affinity.bind_session("all_groups", "session", "sticky", 1_000, 3_600);

        let decision = schedule_once(
            &request,
            &candidates,
            &metrics,
            &capacity,
            &mut affinity,
            &SchedulerAdvancedSettings::default(),
        )
        .expect("eligible candidates");

        assert_eq!(decision.selected_station_key_id.as_deref(), Some("healthy"));
    }

    #[test]
    fn runtime_state_capacity_guard_drives_concurrency_escape() {
        let request = sticky_request();
        let mut sticky = eligible_candidate("sticky", 100);
        sticky.max_concurrency = 1;
        let candidates = vec![sticky, eligible_candidate("healthy", 0)];
        let scheduler = SchedulerRuntimeState::default();
        scheduler.bind_session("all_groups", "session", "sticky", 1_000, 3_600);
        let guard = scheduler.try_acquire("sticky", 1);
        assert!(guard.acquired());

        let escaped = scheduler
            .schedule(&request, &candidates, &SchedulerAdvancedSettings::default())
            .expect("escaped selection");
        assert_eq!(escaped.selected_station_key_id.as_deref(), Some("healthy"));

        drop(guard);
        let sticky_again = scheduler
            .schedule(&request, &candidates, &SchedulerAdvancedSettings::default())
            .expect("sticky selection after release");
        assert_eq!(
            sticky_again.selected_station_key_id.as_deref(),
            Some("sticky")
        );
    }

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

    fn sticky_request() -> ScheduleRequest {
        ScheduleRequest {
            endpoint: RouteEndpointKind::ChatCompletions,
            requested_model: Some("gpt-4o".to_string()),
            mapped_model: Some("gpt-4o".to_string()),
            routing_group_filter: RoutingGroupFilter::AllGroups,
            stream: false,
            uses_tools: false,
            uses_vision: false,
            uses_reasoning: false,
            max_rate_multiplier: 2.0,
            session_hash: Some("session".to_string()),
            previous_response_id: None,
            excluded_key_ids: Vec::new(),
            now_ms: 1_001,
        }
    }

    fn eligible_candidate(station_key_id: &str, priority: i64) -> SchedulerCandidate {
        SchedulerCandidate {
            station_key_id: station_key_id.to_string(),
            station_id: format!("station-{station_key_id}"),
            priority,
            max_concurrency: 0,
            load_factor: None,
            group_binding_id: None,
            group_id_hash: None,
            group_type: None,
            station_enabled: true,
            key_enabled: true,
            schedulable: true,
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: true,
            supports_stream: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: true,
            model_allowlist: Vec::new(),
            model_blocklist: Vec::new(),
            health_blocked: false,
            balance_depleted: false,
            effective_multiplier: Some(EffectiveMultiplierFact {
                station_key_id: station_key_id.to_string(),
                value: 1.0,
                source: "test".to_string(),
                collected_at_ms: Some(1_000),
                valid_until_ms: Some(2_000),
                confidence: 1.0,
                group_binding_id: None,
            }),
            multiplier_reject_reason: None,
        }
    }
}
