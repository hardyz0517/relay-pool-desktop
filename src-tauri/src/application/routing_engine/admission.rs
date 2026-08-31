use std::collections::{BTreeMap, BTreeSet};

use crate::application::routing_engine::{
    candidate_plan::{AvailabilityTier as LegacyAvailabilityTier, RoutePlanCandidate},
    capacity::{
        CapacityAcquireFailure, CapacityConstraintKey, CapacityLease, CapacityMissObservation,
        CapacityWaitMiss, CapacityWaitPermit, CompositeCapacityRegistry, CompositeCapacityRequest,
        PlanningRoundCapacityState, ProviderAccountConstraint,
    },
    intelligent_planner::{plan_snapshot, PlannedCandidate, PlannerError, RoutePlan},
    planning_snapshot::PlanningSnapshot,
    request::{RouteProgress, RouteRequestFacts},
};
use crate::application::routing_policy::AttemptBudgetProfileV1;
use crate::application::station_key_circuit::StationKeyCircuitState;
use crate::application::station_key_circuit::StationKeyCircuitStatus;
use crate::models::model_mapping::FallbackTrigger;
use crate::models::routing_generation::RoutingGenerationAdmissionGuard;

const MAX_RUNTIME_ONLY_REPLANS: u32 = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RoutingGenerationAdmissionDecision {
    Proceed,
    WaitForFence { fence_revision: u64 },
    RebuildSnapshot,
    Deadline,
}

pub(crate) fn assess_routing_generation_admission(
    snapshot: &PlanningSnapshot,
    current: &RoutingGenerationAdmissionGuard,
    now_ms: i64,
    deadline_ms: i64,
) -> RoutingGenerationAdmissionDecision {
    if now_ms >= deadline_ms {
        return RoutingGenerationAdmissionDecision::Deadline;
    }
    if current.fencing {
        return RoutingGenerationAdmissionDecision::WaitForFence {
            fence_revision: current.fence_revision,
        };
    }
    if snapshot.routing_runtime_generation_id != current.active_runtime_generation_id
        || snapshot.routing_generation_fence_revision != current.fence_revision
    {
        return RoutingGenerationAdmissionDecision::RebuildSnapshot;
    }
    RoutingGenerationAdmissionDecision::Proceed
}

fn ordered_planned_candidates(plan: &RoutePlan) -> Vec<&PlannedCandidate> {
    let Some(best_tier) = plan.candidates.iter().map(|candidate| candidate.tier).min() else {
        return Vec::new();
    };
    let mut ordered = Vec::with_capacity(plan.candidates.len());
    if let Some(selected) = plan
        .candidates
        .iter()
        .find(|candidate| candidate.routing_identity == plan.dispatch.selected_id)
    {
        ordered.push(selected);
    }
    ordered.extend(plan.candidates.iter().filter(|candidate| {
        candidate.tier == best_tier && candidate.routing_identity != plan.dispatch.selected_id
    }));
    ordered.extend(plan.candidates.iter().filter(|candidate| {
        candidate.tier != best_tier && candidate.routing_identity != plan.dispatch.selected_id
    }));
    ordered
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateAdmissionProfile {
    pub endpoint_revision: i64,
    pub expected_credential_revision: i64,
    pub credential_revision: i64,
    pub durable_generation: u64,
    pub global_max_concurrency: u32,
    pub station_account_max_concurrency: u32,
    pub station_key_max_concurrency: u32,
    pub provider_account_constraint: ProviderAccountConstraint,
    pub half_open_probe_id: Option<String>,
}

impl CandidateAdmissionProfile {
    pub fn capacity_request(&self, candidate: &RoutePlanCandidate) -> CompositeCapacityRequest {
        CompositeCapacityRequest {
            station_id: candidate.station_id.clone(),
            station_key_id: candidate.station_key_id.clone(),
            half_open_probe_id: self.half_open_probe_id.clone(),
            global_max_concurrency: self.global_max_concurrency,
            station_account_max_concurrency: self.station_account_max_concurrency,
            station_key_max_concurrency: self.station_key_max_concurrency,
            provider_account_constraint: self.provider_account_constraint.clone(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FallbackPolicy {
    pub has_stable_idempotency_key: bool,
    pub non_idempotent: bool,
}

impl FallbackPolicy {
    pub fn retry_safe(self, outcome: ActualAttemptTerminal) -> bool {
        !matches!(outcome, ActualAttemptTerminal::PossiblyAccepted)
            || !self.non_idempotent
            || self.has_stable_idempotency_key
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionSettings {
    pub deadline_ms: i64,
    pub initial_snapshot_id: String,
    pub initial_runtime_overlay_revision: u64,
    pub initial_durable_generation: u64,
    pub fallback_policy: FallbackPolicy,
    pub attempt_budget: AttemptBudgetProfileV1,
}

#[derive(Debug)]
pub struct RouteAdmissionCoordinator {
    #[cfg(test)]
    request: RouteRequestFacts,
    progress: RouteProgress,
    snapshot_id: String,
    runtime_overlay_revision: u64,
    durable_generation: u64,
    pass_capacity: PlanningRoundCapacityState,
    fallback_policy: FallbackPolicy,
    model_fallback_trigger: Option<FallbackTrigger>,
    model_fallback_rank_limit: Option<u16>,
    fallback_blocked: Option<AdmissionFailureKind>,
    max_attempts: Option<u32>,
    candidate_target_ranks: BTreeMap<String, u16>,
    attempted_model_target_ranks: BTreeSet<u16>,
    trace: Vec<AdmissionTraceEvent>,
}

impl RouteAdmissionCoordinator {
    pub fn new(request: RouteRequestFacts, settings: AdmissionSettings) -> Self {
        #[cfg(not(test))]
        let _ = request;
        Self {
            #[cfg(test)]
            request,
            progress: RouteProgress::new(settings.deadline_ms),
            snapshot_id: settings.initial_snapshot_id,
            runtime_overlay_revision: settings.initial_runtime_overlay_revision,
            durable_generation: settings.initial_durable_generation,
            pass_capacity: PlanningRoundCapacityState::default(),
            fallback_policy: settings.fallback_policy,
            model_fallback_trigger: None,
            model_fallback_rank_limit: None,
            fallback_blocked: None,
            max_attempts: Some(settings.attempt_budget.max_total_attempts),
            candidate_target_ranks: BTreeMap::new(),
            attempted_model_target_ranks: BTreeSet::new(),
            trace: Vec::new(),
        }
    }

    pub fn next(
        &mut self,
        input: AdmissionPlanningInput<'_>,
    ) -> Result<AdmissionDecision, AdmissionFailure> {
        if let Some(kind) = self.fallback_blocked.clone() {
            return Err(self.failure(kind, "fallback_blocked"));
        }
        if input.now_ms >= self.progress.view().deadline_ms {
            return Err(self.failure(AdmissionFailureKind::Deadline, "deadline_elapsed"));
        }
        if input.current_runtime_overlay_revision != self.runtime_overlay_revision {
            self.progress.record_runtime_rebuild();
            self.pass_capacity.clear();
            self.runtime_overlay_revision = input.current_runtime_overlay_revision;
            self.trace_event(
                AdmissionTransition::RuntimeReplan,
                "runtime_overlay_revision_changed",
            );
            if self.progress.view().runtime_rebuild_count > MAX_RUNTIME_ONLY_REPLANS {
                return Err(self.failure(
                    AdmissionFailureKind::TemporaryHealth,
                    "runtime_replan_limit_exceeded",
                ));
            }
            return Ok(AdmissionDecision::Replan {
                reason: AdmissionTransition::RuntimeReplan,
            });
        }

        let planning_snapshot = input.planning_snapshot;
        self.model_fallback_trigger = planning_snapshot.model_fallback_trigger;
        if let Some(detail) = candidate_population_failure(planning_snapshot) {
            return Err(self.failure(AdmissionFailureKind::NoEligible, detail));
        }
        self.candidate_target_ranks = planning_snapshot
            .candidates
            .iter()
            .flat_map(|candidate| {
                if candidate.model_variants.is_empty() {
                    vec![(candidate.station_key_id.clone(), 0_u16)]
                } else {
                    candidate
                        .model_variants
                        .iter()
                        .map(|variant| (variant.identity_key(), variant.target_rank))
                        .collect()
                }
            })
            .collect();
        // Keep the score-gate baseline from the complete snapshot. Request
        // exclusion is intentionally applied only to the candidate sequence;
        // it must not make a previously failed Closed Key disappear from the
        // `best_closed_score` comparison for a Half-Open candidate.
        let score_gate_plan = plan_snapshot(
            planning_snapshot,
            input.root_seed,
            self.progress.view().ordinal as u64 + 1,
        )
        .map_err(|error| self.intelligent_planner_failure(error))?;
        let mut working_snapshot = planning_snapshot.clone();
        working_snapshot.candidates.retain(|candidate| {
            !self
                .progress
                .view()
                .excludes_station_key(&candidate.station_key_id)
        });
        let plan = plan_snapshot(
            &working_snapshot,
            input.root_seed,
            self.progress.view().ordinal as u64 + 1,
        )
        .map_err(|error| self.intelligent_planner_failure(error))?;

        let eligible_count = plan.candidates.len();
        if eligible_count == 0 {
            return Err(self.failure(AdmissionFailureKind::NoEligible, "no_available_key"));
        }
        let max_attempts = self
            .max_attempts
            .expect("attempt budget is required by admission settings");
        if self.progress.view().attempt_count >= max_attempts {
            return Err(self.failure(AdmissionFailureKind::AttemptLimit, "attempt_limit_reached"));
        }

        let best_target_rank = plan
            .candidates
            .iter()
            .map(|candidate| candidate.target_rank)
            .min()
            .unwrap_or(0);
        if !model_fallback_rank_allowed(self.model_fallback_rank_limit, best_target_rank) {
            return Err(self.failure(
                AdmissionFailureKind::AttemptLimit,
                "model_fallback_rank_limit_reached",
            ));
        }
        if self.model_fallback_trigger == Some(FallbackTrigger::RetryExhaustedBeforeOutput)
            && best_target_rank > 0
            && !retry_exhausted_fallback_is_open(
                &planning_snapshot.candidates,
                &self.progress.view(),
                &self.attempted_model_target_ranks,
            )
        {
            // A retry-only chain cannot activate merely because rank zero was
            // removed by a durable health/capability gate. It needs an actual
            // pre-output terminal attempt on a rank-zero target; otherwise
            // this request has no eligible target under the configured policy.
            return Err(self.failure(
                AdmissionFailureKind::NoEligible,
                "fallback_requires_rank_zero_attempt",
            ));
        }
        // Model fallback is a rank-level policy. Capacity misses and scoring
        // must not silently jump to a lower-priority native model in the same
        // admission pass; the next rank becomes eligible only after the
        // current rank has produced a terminal exclusion.
        for planned in ordered_planned_candidates(&plan)
            .into_iter()
            .filter(|planned| planned.target_rank == best_target_rank)
        {
            let score_gate_passed =
                half_open_score_gate(planned, &score_gate_plan, input.circuit_statuses);
            let candidate = if let Some(base) = input
                .execution_candidates
                .iter()
                .find(|candidate| candidate.routing_identity() == planned.routing_identity)
            {
                let mut candidate = base.clone();
                candidate.tier = match planned.tier {
                    crate::application::routing_engine::tiers::AvailabilityTier::Primary => {
                        LegacyAvailabilityTier::Primary
                    }
                    crate::application::routing_engine::tiers::AvailabilityTier::ConfiguredBackup => {
                        LegacyAvailabilityTier::ConfiguredBackup
                    }
                    crate::application::routing_engine::tiers::AvailabilityTier::DepletedEmergency => {
                        LegacyAvailabilityTier::DepletedEmergency
                    }
                };
                candidate.evidence = vec![
                    crate::application::routing_engine::candidate_plan::DecisionEvidence {
                        code: "planner_snapshot",
                        detail: plan.snapshot_id.clone(),
                    },
                    crate::application::routing_engine::candidate_plan::DecisionEvidence {
                        code: "utility_score",
                        detail: planned.utility.value().to_string(),
                    },
                    crate::application::routing_engine::candidate_plan::DecisionEvidence {
                        code: "base_utility_score",
                        detail: planned.base_utility.value().to_string(),
                    },
                    crate::application::routing_engine::candidate_plan::DecisionEvidence {
                        code: "affinity_bonus",
                        detail: planned.affinity_bonus.to_string(),
                    },
                    crate::application::routing_engine::candidate_plan::DecisionEvidence {
                        code: "affinity_applied",
                        detail: planned.affinity_applied.to_string(),
                    },
                ];
                candidate.model_variant = planned.variant.clone();
                candidate.resolved_upstream_model = planned
                    .variant
                    .as_ref()
                    .map(|variant| variant.upstream_model.clone())
                    .or(candidate.resolved_upstream_model);
                candidate
            } else {
                return Err(self.failure(
                    AdmissionFailureKind::ConfigUnstable,
                    "candidate_snapshot_missing",
                ));
            };
            let Some(profile) = input.profiles.get(&candidate.station_key_id) else {
                return Err(self.failure(AdmissionFailureKind::ConfigUnstable, "missing_profile"));
            };
            if self.capacity_state_blocks_candidate(&candidate, profile) {
                self.trace_event(
                    AdmissionTransition::PlanSkip,
                    "candidate_unavailable_this_pass",
                );
                continue;
            }
            if self.profile_invalidates_candidate(&candidate, profile) {
                return self.rebuild_or_fail_config(profile);
            }

            match input
                .capacity
                .try_acquire(profile.capacity_request(&candidate))
            {
                Ok(lease) => {
                    let selected_station_key_id = candidate.station_key_id.clone();
                    self.trace_event(
                        AdmissionTransition::CapacityAcquired,
                        "capacity_lease_acquired",
                    );
                    return Ok(AdmissionDecision::Selected(SelectedRoute {
                        candidate,
                        lease,
                        evidence: vec![AdmissionEvidence::new("selected", selected_station_key_id)],
                        score_gate_passed,
                    }));
                }
                Err(failure) => {
                    let observation = miss_observation(&failure);
                    self.pass_capacity.record_miss(observation);
                    self.trace_event(AdmissionTransition::CapacityMiss, "capacity_miss");
                    continue;
                }
            }
        }

        match self.pass_capacity.build_wait_plan(
            input.now_ms,
            self.progress.view().deadline_ms,
            input.max_waiters_per_constraint,
        ) {
            Ok(plan) => match input.capacity.try_enter_wait(
                plan.constraint.clone(),
                plan.max_waiters,
                input.now_ms,
                self.progress.view().deadline_ms,
            ) {
                Ok(permit) => {
                    self.trace_event(AdmissionTransition::WaitEntered, "capacity_wait_entered");
                    Ok(AdmissionDecision::Wait {
                        constraint: plan.constraint,
                        permit,
                    })
                }
                Err(miss) => Err(self.wait_failure(miss)),
            },
            Err(_) => Err(self.failure(
                AdmissionFailureKind::CapacityExhausted,
                "all_strata_capacity_exhausted",
            )),
        }
    }

    pub fn record_actual_terminal_for_station_key(
        &mut self,
        station_key_id: String,
        routing_identity: String,
        outcome: ActualAttemptTerminal,
    ) -> Result<(), AdmissionFailure> {
        self.progress.record_actual_attempt(&station_key_id);
        if let Some(target_rank) = self.candidate_target_ranks.get(&routing_identity) {
            self.attempted_model_target_ranks.insert(*target_rank);
        }
        self.pass_capacity.clear();
        self.trace_event(AdmissionTransition::AttemptTerminal, outcome.as_code());
        if self.model_fallback_trigger == Some(FallbackTrigger::NoEligibleTarget)
            && outcome == ActualAttemptTerminal::FailedBeforeCommit
        {
            // `no_eligible_target` is a qualification fallback, not a
            // transport-error fallback. Once a target has been attempted and
            // failed before output, keep trying other candidates in that
            // rank but never descend to a lower model.
            let target_rank = self
                .candidate_target_ranks
                .get(&routing_identity)
                .copied()
                .unwrap_or(0);
            self.model_fallback_rank_limit = Some(tighten_model_fallback_rank_limit(
                self.model_fallback_rank_limit,
                target_rank,
            ));
        }
        if !self.fallback_policy.retry_safe(outcome) {
            self.fallback_blocked = Some(AdmissionFailureKind::CommitUncertain);
            return Err(self.failure(
                AdmissionFailureKind::CommitUncertain,
                "possibly_accepted_without_idempotency_key",
            ));
        }
        Ok(())
    }

    /// Records a retry attempt while keeping the current key eligible. The
    /// retry advances the outbound ordinal but does not consume another
    /// distinct-key slot from maxRetryCount.
    pub fn record_retry_attempt(&mut self) {
        self.progress.record_retry_attempt();
        self.pass_capacity.clear();
        self.trace_event(AdmissionTransition::AttemptTerminal, "retry_current_key");
    }

    /// Accounts for a key whose durable circuit opened after its last retry.
    /// No new outbound attempt is recorded here; the method consumes the one
    /// distinct-key slot and prevents a model variant from resurrecting it.
    pub fn exclude_attempted_key(&mut self, station_key_id: impl Into<String>) {
        self.progress.exclude_attempted_key(station_key_id);
        self.pass_capacity.clear();
        self.trace_event(AdmissionTransition::AttemptTerminal, "key_circuit_opened");
    }

    pub fn deadline_ms(&self) -> i64 {
        self.progress.view().deadline_ms
    }

    /// Remaining distinct keys after the currently active key. The initial
    /// key occupies one slot; maxRetryCount supplies the additional slots.
    pub fn remaining_additional_key_budget(&self) -> u32 {
        self.max_attempts
            .unwrap_or(1)
            .saturating_sub(self.progress.view().attempt_count)
            .saturating_sub(1)
    }

    /// Excludes one concrete station key for the remainder of this request.
    /// This is used when durable circuit admission rejects a candidate after
    /// the immutable planner snapshot has already selected it. The exclusion
    /// is identity-based, so a model variant cannot resurrect the same key.
    pub fn exclude_station_key(&mut self, station_key_id: impl Into<String>) -> bool {
        let station_key_id = station_key_id.into();
        if station_key_id.is_empty() {
            return false;
        }
        self.progress.exclude_without_attempt(station_key_id)
    }

    #[cfg(test)]
    pub fn trace(&self) -> &[AdmissionTraceEvent] {
        &self.trace
    }

    fn capacity_state_blocks_candidate(
        &self,
        candidate: &RoutePlanCandidate,
        profile: &CandidateAdmissionProfile,
    ) -> bool {
        self.pass_capacity
            .unavailable_this_pass
            .iter()
            .any(|miss| match &miss.constraint {
                CapacityConstraintKey::Global => true,
                CapacityConstraintKey::StationAccount(station_id) => {
                    station_id == &candidate.station_id
                }
                CapacityConstraintKey::StationKey(station_key_id) => {
                    station_key_id == &candidate.station_key_id
                }
                #[cfg(test)]
                CapacityConstraintKey::ProviderAccount(provider_account_id) => {
                    matches!(
                        &profile.provider_account_constraint,
                        ProviderAccountConstraint::Trusted {
                            provider_account_id: candidate_provider_account_id,
                            ..
                        } if candidate_provider_account_id == provider_account_id
                    )
                }
                CapacityConstraintKey::HalfOpen(half_open_probe_id) => {
                    profile.half_open_probe_id.as_deref() == Some(half_open_probe_id.as_str())
                }
            })
    }

    fn profile_invalidates_candidate(
        &self,
        candidate: &RoutePlanCandidate,
        profile: &CandidateAdmissionProfile,
    ) -> bool {
        profile.endpoint_revision != candidate.endpoint_revision
            || profile.credential_revision != profile.expected_credential_revision
            || profile.durable_generation != self.durable_generation
    }

    fn rebuild_or_fail_config(
        &mut self,
        profile: &CandidateAdmissionProfile,
    ) -> Result<AdmissionDecision, AdmissionFailure> {
        if self.progress.view().snapshot_rebuild_count > 0 {
            return Err(self.failure(
                AdmissionFailureKind::ConfigUnstable,
                "config_fence_changed_after_rebuild",
            ));
        }
        self.progress.record_snapshot_rebuild();
        self.durable_generation = profile.durable_generation;
        self.snapshot_id = format!("snapshot-generation-{}", profile.durable_generation);
        self.pass_capacity.clear();
        self.trace_event(
            AdmissionTransition::SnapshotRebuild,
            "candidate_fence_changed",
        );
        Ok(AdmissionDecision::Replan {
            reason: AdmissionTransition::SnapshotRebuild,
        })
    }

    fn intelligent_planner_failure(&self, error: PlannerError) -> AdmissionFailure {
        match error {
            PlannerError::InvalidSnapshot(detail) => AdmissionFailure {
                kind: AdmissionFailureKind::ConfigUnstable,
                evidence: vec![AdmissionEvidence::new("invalid_planning_snapshot", detail)],
            },
            PlannerError::NoEligibleCandidate => {
                self.failure(AdmissionFailureKind::NoEligible, "no_available_key")
            }
            PlannerError::RuntimeAtCapacity => self.failure(
                AdmissionFailureKind::CapacityExhausted,
                "runtime_at_capacity",
            ),
        }
    }

    fn wait_failure(&self, miss: CapacityWaitMiss) -> AdmissionFailure {
        let detail = match miss {
            CapacityWaitMiss::NotAdmitted => "wait_not_admitted",
            CapacityWaitMiss::QueueFull => "wait_queue_full",
        };
        self.failure(AdmissionFailureKind::CapacityExhausted, detail)
    }

    fn failure(&self, kind: AdmissionFailureKind, detail: impl Into<String>) -> AdmissionFailure {
        AdmissionFailure {
            kind,
            evidence: vec![AdmissionEvidence::new("failure", detail)],
        }
    }

    fn trace_event(&mut self, transition: AdmissionTransition, detail: impl Into<String>) {
        self.trace.push(AdmissionTraceEvent {
            transition,
            evidence: vec![AdmissionEvidence::new("reason", detail)],
        });
        if self.trace.len() > 64 {
            self.trace.remove(0);
        }
    }
}

fn candidate_population_failure(snapshot: &PlanningSnapshot) -> Option<&'static str> {
    if snapshot.configured_key_count == 0 {
        Some("no_configured_key")
    } else if snapshot.capability_match_count == 0 {
        Some("capability_mismatch")
    } else if snapshot.candidate_cap_count == 0 {
        Some("static_candidate_unavailable")
    } else {
        None
    }
}

/// Returns whether an Open/Half-Open candidate is allowed to enter the
/// deterministic route sequence. The comparison is intentionally limited to
/// the candidate's exact target rank and availability tier: scores from a
/// lower-priority model rank or backup layer must never suppress recovery of a
/// higher-priority layer. Missing state means the reducer will create Closed
/// state on admission, so it is treated as Closed here as well.
fn half_open_score_gate(
    planned: &PlannedCandidate,
    plan: &RoutePlan,
    statuses: &[StationKeyCircuitStatus],
) -> bool {
    let current_is_closed = statuses
        .iter()
        .find(|status| {
            status.station_key_id == planned.station_key_id
                && status.lifecycle_revision
                    == u64::try_from(planned.lifecycle_revision.max(1)).unwrap_or(1)
        })
        .map(|status| matches!(status.state, StationKeyCircuitState::Closed { .. }))
        .unwrap_or(true);
    if current_is_closed {
        return true;
    }

    let best_closed_score = plan
        .candidates
        .iter()
        .filter(|other| {
            other.station_key_id != planned.station_key_id
                && other.target_rank == planned.target_rank
                && other.tier == planned.tier
                && statuses
                    .iter()
                    .find(|status| {
                        status.station_key_id == other.station_key_id
                            && status.lifecycle_revision
                                == u64::try_from(other.lifecycle_revision.max(1)).unwrap_or(1)
                    })
                    .map(|status| matches!(status.state, StationKeyCircuitState::Closed { .. }))
                    .unwrap_or(true)
        })
        .map(|other| other.utility.value())
        .max();
    best_closed_score.is_none_or(|best| planned.utility.value() > best)
}

#[derive(Debug)]
pub struct AdmissionPlanningInput<'a> {
    pub execution_candidates: &'a [RoutePlanCandidate],
    pub planning_snapshot: &'a PlanningSnapshot,
    pub root_seed: &'a [u8],
    #[cfg(test)]
    pub affinity_station_key_id: Option<&'a str>,
    pub profiles: &'a BTreeMap<String, CandidateAdmissionProfile>,
    pub capacity: &'a CompositeCapacityRegistry,
    pub current_runtime_overlay_revision: u64,
    pub now_ms: i64,
    pub max_waiters_per_constraint: u32,
    pub circuit_statuses: &'a [StationKeyCircuitStatus],
}

#[derive(Debug)]
pub enum AdmissionDecision {
    Selected(SelectedRoute),
    Wait {
        constraint: CapacityConstraintKey,
        permit: CapacityWaitPermit,
    },
    Replan {
        reason: AdmissionTransition,
    },
}

#[derive(Debug)]
pub struct SelectedRoute {
    pub candidate: RoutePlanCandidate,
    pub lease: CapacityLease,
    pub evidence: Vec<AdmissionEvidence>,
    /// Whether this candidate passed the Half-Open score gate in the same
    /// immutable planning snapshot. Closed admission ignores this value.
    pub score_gate_passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActualAttemptTerminal {
    FailedBeforeCommit,
    PossiblyAccepted,
    Succeeded,
}

impl ActualAttemptTerminal {
    fn as_code(self) -> &'static str {
        match self {
            Self::FailedBeforeCommit => "failed_before_commit",
            Self::PossiblyAccepted => "possibly_accepted",
            Self::Succeeded => "succeeded",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionFailure {
    pub kind: AdmissionFailureKind,
    pub evidence: Vec<AdmissionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdmissionFailureKind {
    NoEligible,
    TemporaryHealth,
    CapacityExhausted,
    Deadline,
    ConfigUnstable,
    AttemptLimit,
    CommitUncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmissionTransition {
    RuntimeReplan,
    SnapshotRebuild,
    PlanSkip,
    CapacityMiss,
    CapacityAcquired,
    WaitEntered,
    #[cfg(test)]
    WaitWakeup,
    AttemptTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionTraceEvent {
    pub transition: AdmissionTransition,
    pub evidence: Vec<AdmissionEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdmissionEvidence {
    pub code: &'static str,
    pub detail: String,
}

impl AdmissionEvidence {
    fn new(code: &'static str, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: detail.into(),
        }
    }
}

fn miss_observation(failure: &CapacityAcquireFailure) -> CapacityMissObservation {
    match failure {
        CapacityAcquireFailure::ConstraintUnavailable {
            constraint,
            in_flight,
            max_concurrency,
            ..
        } => CapacityMissObservation {
            constraint: constraint.clone(),
            waitable: !matches!(constraint, CapacityConstraintKey::HalfOpen(_)),
            in_flight: *in_flight,
            max_concurrency: *max_concurrency,
        },
    }
}

fn retry_exhausted_fallback_is_open(
    candidates: &[crate::application::routing_engine::planning_snapshot::CandidateSnapshot],
    progress: &crate::application::routing_engine::request::RouteProgressView,
    attempted_target_ranks: &BTreeSet<u16>,
) -> bool {
    if attempted_target_ranks.contains(&0) {
        return true;
    }
    candidates.iter().any(|candidate| {
        progress.excludes_station_key(&candidate.station_key_id)
            && (candidate.model_variants.is_empty()
                || candidate
                    .model_variants
                    .iter()
                    .any(|variant| variant.target_rank == 0))
    })
}

fn model_fallback_rank_allowed(rank_limit: Option<u16>, target_rank: u16) -> bool {
    rank_limit.is_none_or(|rank_limit| target_rank <= rank_limit)
}

fn tighten_model_fallback_rank_limit(current: Option<u16>, failed_rank: u16) -> u16 {
    current.map_or(failed_rank, |current| current.min(failed_rank))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::planning_snapshot::CandidateSnapshot;
    use crate::application::routing_engine::{
        fixed_point::{BasisPoints, FactorContribution, UtilityScore},
        tiers::AvailabilityTier,
    };

    fn candidate(domains: &[&str]) -> CandidateSnapshot {
        CandidateSnapshot {
            station_key_id: "key".to_string(),
            station_id: "station".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: Some("test-model".to_string()),
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
            pricing: crate::application::routing_engine::candidate_plan::RoutePlanPricingSnapshot::unpriced("test"),
            preference_basis_points: 5_000,
            failure_domains: domains.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    fn snapshot_with_candidate() -> PlanningSnapshot {
        let policy = crate::models::routing_policy::RoutingPolicyConfigV2::default();
        PlanningSnapshot {
            snapshot_id: "generation-guard".into(),
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
            attempt_budget: AttemptBudgetProfileV1::from_policy(1, &policy.retry_failover)
                .expect("attempt budget"),
            policy,
            profile: crate::application::routing_engine::algorithm_profile::DispatchAlgorithmProfile::default(),
            candidates: vec![candidate(&[])],
            model_fallback_trigger: None,
            runtime: crate::application::routing_engine::planning_snapshot::RuntimeOverlaySnapshot {
                runtime_instance_id: "runtime".into(),
                runtime_revision: 1,
                candidate_set_revision: 1,
                in_flight: 0,
                max_concurrency: 1,
                affinity_station_key_id: None,
            },
        }
    }

    #[test]
    fn retry_only_fallback_requires_an_actual_rank_zero_attempt() {
        let candidates = vec![candidate(&[])];
        let mut progress = RouteProgress::new(1_000);
        let attempted = BTreeSet::new();
        assert!(!retry_exhausted_fallback_is_open(
            &candidates,
            &progress.view(),
            &attempted,
        ));

        progress.record_actual_attempt("key");
        assert!(retry_exhausted_fallback_is_open(
            &candidates,
            &progress.view(),
            &attempted,
        ));
    }

    #[test]
    fn retry_only_fallback_tracks_rank_zero_variant_by_station_key() {
        let mut rank_zero = candidate(&[]);
        rank_zero.model_variants = vec![crate::application::model_mapping::CandidateModelVariant {
            station_key_id: "key".to_string(),
            station_id: "station".to_string(),
            upstream_model: "native".to_string(),
            target_rank: 0,
            binding_revision: None,
            model_resolution_fence: "mapping-fence".to_string(),
            endpoint: crate::models::model_mapping::EndpointKind::Responses,
            credential_revision: 1,
            endpoint_revision: 1,
        }];
        let candidates = vec![rank_zero];
        let mut progress = RouteProgress::new(1_000);
        let attempted = BTreeSet::new();
        assert!(!retry_exhausted_fallback_is_open(
            &candidates,
            &progress.view(),
            &attempted,
        ));
        progress.record_actual_attempt(candidates[0].station_key_id.clone());
        assert!(retry_exhausted_fallback_is_open(
            &candidates,
            &progress.view(),
            &attempted,
        ));
    }

    #[test]
    fn station_key_exclusion_covers_every_model_variant() {
        let mut candidate = candidate(&[]);
        candidate.model_variants = [("native", 0_u16), ("mapped-fallback", 1_u16)]
            .into_iter()
            .map(|(upstream_model, target_rank)| {
                crate::application::model_mapping::CandidateModelVariant {
                    station_key_id: candidate.station_key_id.clone(),
                    station_id: "station".to_string(),
                    upstream_model: upstream_model.to_string(),
                    target_rank,
                    binding_revision: None,
                    model_resolution_fence: "mapping-fence".to_string(),
                    endpoint: crate::models::model_mapping::EndpointKind::Responses,
                    credential_revision: 1,
                    endpoint_revision: 1,
                }
            })
            .collect();

        let mut progress = RouteProgress::new(1_000);
        progress.record_actual_attempt(candidate.station_key_id.clone());

        assert!(progress
            .view()
            .excludes_station_key(&candidate.station_key_id));
        assert!(candidate.model_variants.iter().all(|variant| !progress
            .view()
            .excludes_station_key(&variant.identity_key())));
    }

    #[test]
    fn retry_only_fallback_survives_rank_zero_snapshot_rebuild() {
        let candidates = vec![candidate(&[])];
        let progress = RouteProgress::new(1_000).view();
        let attempted = BTreeSet::from([0_u16]);
        assert!(retry_exhausted_fallback_is_open(
            &candidates,
            &progress,
            &attempted,
        ));
    }

    #[test]
    fn no_eligible_fallback_keeps_same_rank_and_blocks_lower_ranks() {
        assert!(model_fallback_rank_allowed(Some(0), 0));
        assert!(!model_fallback_rank_allowed(Some(0), 1));
        assert_eq!(tighten_model_fallback_rank_limit(None, 0), 0);
        assert_eq!(tighten_model_fallback_rank_limit(Some(1), 0), 0);
        assert_eq!(tighten_model_fallback_rank_limit(Some(0), 1), 0);
    }

    fn planned(
        station_key_id: &str,
        score: u16,
        target_rank: u16,
        tier: AvailabilityTier,
    ) -> PlannedCandidate {
        let score = BasisPoints::new(score).expect("score");
        let zero = BasisPoints::ZERO;
        PlannedCandidate {
            station_key_id: station_key_id.to_string(),
            lifecycle_revision: 1,
            routing_identity: station_key_id.to_string(),
            target_rank,
            variant: None,
            tier,
            base_utility: UtilityScore::new(score),
            utility: UtilityScore::new(score),
            affinity_bonus: BasisPoints::ZERO,
            affinity_applied: false,
            contributions: [FactorContribution {
                weight: zero,
                score: zero,
                contribution: zero,
            }; 4],
        }
    }

    fn status(station_key_id: &str, state: StationKeyCircuitState) -> StationKeyCircuitStatus {
        StationKeyCircuitStatus {
            station_key_id: station_key_id.to_string(),
            lifecycle_revision: 1,
            policy_revision: 1,
            lease_policy: None,
            state,
        }
    }

    fn make_plan(candidates: Vec<PlannedCandidate>) -> RoutePlan {
        let selected = candidates
            .first()
            .map(|candidate| candidate.routing_identity.clone())
            .unwrap_or_default();
        RoutePlan {
            snapshot_id: "snapshot".to_string(),
            selected_station_key_id: selected.clone(),
            candidates,
            dispatch: crate::application::routing_engine::dispatch::DispatchDecision {
                selected_id: selected,
                band_size: 1,
                explored: false,
                seed_commitment: "seed".to_string(),
            },
        }
    }

    #[test]
    fn half_open_gate_rejects_when_same_layer_closed_score_is_higher() {
        let current = planned("key-a", 7_000, 0, AvailabilityTier::Primary);
        let other = planned("key-b", 8_000, 0, AvailabilityTier::Primary);
        let plan = make_plan(vec![current.clone(), other]);
        let statuses = vec![status(
            "key-a",
            StationKeyCircuitState::Open {
                state_revision: 1,
                opened_at_ms: 1,
                cooldown_until_ms: 1,
                consecutive_failures: 3,
                reopen_level: 1,
            },
        )];
        assert!(!half_open_score_gate(&current, &plan, &statuses));
    }

    #[test]
    fn half_open_gate_does_not_compare_across_rank_or_tier() {
        let current = planned("key-a", 7_000, 0, AvailabilityTier::Primary);
        let lower_rank = planned("key-b", 9_000, 1, AvailabilityTier::Primary);
        let backup = planned("key-c", 9_000, 0, AvailabilityTier::ConfiguredBackup);
        let plan = make_plan(vec![current.clone(), lower_rank, backup]);
        let statuses = vec![status(
            "key-a",
            StationKeyCircuitState::Open {
                state_revision: 1,
                opened_at_ms: 1,
                cooldown_until_ms: 1,
                consecutive_failures: 3,
                reopen_level: 1,
            },
        )];
        assert!(half_open_score_gate(&current, &plan, &statuses));
    }

    #[test]
    fn half_open_gate_treats_missing_state_as_closed_and_allows_without_closed_baseline() {
        let missing = planned("key-a", 1_000, 0, AvailabilityTier::Primary);
        let plan = make_plan(vec![missing.clone()]);
        assert!(half_open_score_gate(&missing, &plan, &[]));

        let current = planned("key-a", 7_000, 0, AvailabilityTier::Primary);
        let plan = make_plan(vec![current.clone()]);
        let statuses = vec![status(
            "key-a",
            StationKeyCircuitState::Open {
                state_revision: 1,
                opened_at_ms: 1,
                cooldown_until_ms: 1,
                consecutive_failures: 3,
                reopen_level: 1,
            },
        )];
        assert!(half_open_score_gate(&current, &plan, &statuses));
    }

    #[test]
    fn generation_fence_wait_is_distinct_from_no_available_key() {
        let mut snapshot = snapshot_with_candidate();
        snapshot.routing_runtime_generation_id = Some("rg1_active".into());
        snapshot.routing_generation_fence_revision = 4;
        let guard = RoutingGenerationAdmissionGuard {
            active_runtime_generation_id: Some("rg1_active".into()),
            fence_revision: 5,
            fencing: true,
        };
        assert_eq!(
            assess_routing_generation_admission(&snapshot, &guard, 10, 100),
            RoutingGenerationAdmissionDecision::WaitForFence { fence_revision: 5 }
        );
        assert_eq!(
            assess_routing_generation_admission(&snapshot, &guard, 100, 100),
            RoutingGenerationAdmissionDecision::Deadline
        );
    }

    #[test]
    fn generation_change_requires_snapshot_rebuild_before_admission() {
        let mut snapshot = snapshot_with_candidate();
        snapshot.routing_runtime_generation_id = Some("rg1_old".into());
        snapshot.routing_generation_fence_revision = 5;
        let guard = RoutingGenerationAdmissionGuard {
            active_runtime_generation_id: Some("rg1_new".into()),
            fence_revision: 5,
            fencing: false,
        };
        assert_eq!(
            assess_routing_generation_admission(&snapshot, &guard, 10, 100),
            RoutingGenerationAdmissionDecision::RebuildSnapshot
        );
    }

    #[test]
    fn terminal_population_classification_uses_frozen_snapshot_counts() {
        let mut snapshot = snapshot_with_candidate();
        snapshot.configured_key_count = 0;
        snapshot.capability_match_count = 0;
        snapshot.candidate_cap_count = 0;
        snapshot.candidates.clear();
        assert_eq!(
            candidate_population_failure(&snapshot),
            Some("no_configured_key")
        );

        snapshot.configured_key_count = 1;
        assert_eq!(
            candidate_population_failure(&snapshot),
            Some("capability_mismatch")
        );

        snapshot.capability_match_count = 1;
        assert_eq!(
            candidate_population_failure(&snapshot),
            Some("static_candidate_unavailable")
        );

        snapshot.candidate_cap_count = 1;
        assert_eq!(candidate_population_failure(&snapshot), None);
    }
}
