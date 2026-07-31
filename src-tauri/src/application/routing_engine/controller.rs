#![allow(dead_code)]

use std::collections::BTreeMap;

use crate::application::routing_engine::{
    capacity::{
        CapacityAcquireFailure, CapacityConstraintKey, CapacityLease, CapacityMissObservation,
        CapacityWaitMiss, CapacityWaitPermit, CompositeCapacityRegistry, CompositeCapacityRequest,
        PlanningRoundCapacityState, ProviderAccountConstraint, RetryBudgetMiss,
        RetryBudgetRegistry, RetryPermit, RetryPermitDecision,
    },
    planner::{ordered_plan_candidates, plan_candidate_count, plan_route, PlanningInput},
    request::{PlanningRoundContext, RouteProgress, RouteRequestFacts},
    selector::{RoutePlanCandidate, RoutePlannerError},
};

const MAX_RUNTIME_ONLY_REPLANS: u32 = 8;
const DEFAULT_MAX_ATTEMPTS: u32 = 3;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateAdmissionProfile {
    pub(crate) endpoint_revision: i64,
    pub(crate) expected_credential_revision: i64,
    pub(crate) credential_revision: i64,
    pub(crate) durable_generation: u64,
    pub(crate) global_max_concurrency: u32,
    pub(crate) station_account_max_concurrency: u32,
    pub(crate) station_key_max_concurrency: u32,
    pub(crate) provider_account_constraint: ProviderAccountConstraint,
    pub(crate) half_open_probe_id: Option<String>,
}

impl CandidateAdmissionProfile {
    pub(crate) fn capacity_request(
        &self,
        candidate: &RoutePlanCandidate,
    ) -> CompositeCapacityRequest {
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
pub(crate) struct FallbackPolicy {
    pub(crate) has_stable_idempotency_key: bool,
    pub(crate) non_idempotent: bool,
}

impl FallbackPolicy {
    pub(crate) fn retry_safe(self, outcome: ActualAttemptTerminal) -> bool {
        !matches!(outcome, ActualAttemptTerminal::PossiblyAccepted)
            || !self.non_idempotent
            || self.has_stable_idempotency_key
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteControllerSettings {
    pub(crate) deadline_ms: i64,
    pub(crate) initial_snapshot_id: String,
    pub(crate) initial_runtime_overlay_revision: u64,
    pub(crate) initial_durable_generation: u64,
    pub(crate) fallback_policy: FallbackPolicy,
}

#[derive(Debug)]
pub(crate) struct RouteAdmissionController {
    request: RouteRequestFacts,
    progress: RouteProgress,
    snapshot_id: String,
    runtime_overlay_revision: u64,
    durable_generation: u64,
    pass_capacity: PlanningRoundCapacityState,
    retry_budget: RetryBudgetRegistry,
    fallback_policy: FallbackPolicy,
    fallback_blocked: Option<ControllerFailureKind>,
    max_attempts: Option<u32>,
    trace: Vec<ControllerTraceEvent>,
}

impl RouteAdmissionController {
    pub(crate) fn new(
        request: RouteRequestFacts,
        settings: RouteControllerSettings,
        initial_active_or_pending_capacity: u32,
    ) -> Self {
        Self {
            request,
            progress: RouteProgress::new(settings.deadline_ms),
            snapshot_id: settings.initial_snapshot_id,
            runtime_overlay_revision: settings.initial_runtime_overlay_revision,
            durable_generation: settings.initial_durable_generation,
            pass_capacity: PlanningRoundCapacityState::default(),
            retry_budget: RetryBudgetRegistry::new(initial_active_or_pending_capacity),
            fallback_policy: settings.fallback_policy,
            fallback_blocked: None,
            max_attempts: None,
            trace: Vec::new(),
        }
    }

    pub(crate) fn next(
        &mut self,
        input: ControllerPlanningInput<'_>,
    ) -> Result<ControllerDecision, ControllerFailure> {
        if let Some(kind) = self.fallback_blocked.clone() {
            return Err(self.failure(kind, "fallback_blocked"));
        }
        if input.now_ms >= self.progress.view().deadline_ms {
            return Err(self.failure(ControllerFailureKind::Deadline, "deadline_elapsed"));
        }
        if input.current_runtime_overlay_revision != self.runtime_overlay_revision {
            self.progress.record_runtime_rebuild();
            self.pass_capacity.clear();
            self.runtime_overlay_revision = input.current_runtime_overlay_revision;
            self.trace_event(
                ControllerTransition::RuntimeReplan,
                "runtime_overlay_revision_changed",
            );
            if self.progress.view().runtime_rebuild_count > MAX_RUNTIME_ONLY_REPLANS {
                return Err(self.failure(
                    ControllerFailureKind::TemporaryHealth,
                    "runtime_replan_limit_exceeded",
                ));
            }
            return Ok(ControllerDecision::Replan {
                reason: ControllerTransition::RuntimeReplan,
            });
        }

        let context = PlanningRoundContext {
            request: self.request.clone(),
            progress: self.progress.view(),
            snapshot_id: self.snapshot_id.clone(),
            runtime_overlay_revision: self.runtime_overlay_revision,
        };
        let plan = plan_route(PlanningInput {
            context: &context,
            candidates: input.candidates,
            affinity_station_key_id: input.affinity_station_key_id,
        })
        .map_err(|error| self.planner_failure(error))?;

        let eligible_count = plan_candidate_count(&plan);
        if eligible_count == 0 {
            return Err(self.failure(ControllerFailureKind::NoEligible, "no_eligible_candidate"));
        }
        let max_attempts = *self
            .max_attempts
            .get_or_insert_with(|| DEFAULT_MAX_ATTEMPTS.min(eligible_count as u32).max(1));
        if self.progress.view().attempt_count >= max_attempts {
            return Err(self.failure(ControllerFailureKind::AttemptLimit, "attempt_limit_reached"));
        }

        for candidate in ordered_plan_candidates(&plan) {
            let Some(profile) = input.profiles.get(&candidate.station_key_id) else {
                return Err(self.failure(ControllerFailureKind::ConfigUnstable, "missing_profile"));
            };
            if self.capacity_state_blocks_candidate(candidate, profile) {
                self.trace_event(
                    ControllerTransition::PlanSkip,
                    "candidate_unavailable_this_pass",
                );
                continue;
            }
            if self.profile_invalidates_candidate(candidate, profile) {
                return self.rebuild_or_fail_config(profile);
            }

            match input
                .capacity
                .try_acquire(profile.capacity_request(candidate))
            {
                Ok(mut lease) => {
                    let retry_permit = self.acquire_retry_permit_after_capacity(&mut lease)?;
                    self.trace_event(
                        ControllerTransition::CapacityAcquired,
                        "capacity_lease_acquired",
                    );
                    return Ok(ControllerDecision::Selected(SelectedRoute {
                        candidate: candidate.clone(),
                        lease,
                        retry_permit,
                        evidence: vec![ControllerEvidence::new(
                            "selected",
                            candidate.station_key_id.clone(),
                        )],
                    }));
                }
                Err(failure) => {
                    let observation = miss_observation(&failure);
                    self.pass_capacity.record_miss(observation);
                    self.trace_event(ControllerTransition::CapacityMiss, "capacity_miss");
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
                    self.trace_event(ControllerTransition::WaitEntered, "capacity_wait_entered");
                    Ok(ControllerDecision::Wait {
                        constraint: plan.constraint,
                        permit,
                    })
                }
                Err(miss) => Err(self.wait_failure(miss)),
            },
            Err(_) => Err(self.failure(
                ControllerFailureKind::CapacityExhausted,
                "all_strata_capacity_exhausted",
            )),
        }
    }

    pub(crate) fn record_wait_wakeup(&mut self, runtime_overlay_revision: u64) {
        self.pass_capacity.clear();
        self.runtime_overlay_revision = runtime_overlay_revision;
        self.trace_event(ControllerTransition::WaitWakeup, "wait_wakeup_replan");
    }

    pub(crate) fn record_actual_terminal(
        &mut self,
        selected: SelectedRoute,
        outcome: ActualAttemptTerminal,
    ) -> Result<(), ControllerFailure> {
        let station_key_id = selected.candidate.station_key_id.clone();
        drop(selected);
        self.progress.record_actual_attempt(station_key_id);
        self.pass_capacity.clear();
        self.trace_event(ControllerTransition::AttemptTerminal, outcome.as_code());
        if !self.fallback_policy.retry_safe(outcome) {
            self.fallback_blocked = Some(ControllerFailureKind::CommitUncertain);
            return Err(self.failure(
                ControllerFailureKind::CommitUncertain,
                "possibly_accepted_without_idempotency_key",
            ));
        }
        Ok(())
    }

    pub(crate) fn progress_view(
        &self,
    ) -> crate::application::routing_engine::request::RouteProgressView {
        self.progress.view()
    }

    pub(crate) fn pass_capacity_state(&self) -> &PlanningRoundCapacityState {
        &self.pass_capacity
    }

    pub(crate) fn trace(&self) -> &[ControllerTraceEvent] {
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
    ) -> Result<ControllerDecision, ControllerFailure> {
        if self.progress.view().snapshot_rebuild_count > 0 {
            return Err(self.failure(
                ControllerFailureKind::ConfigUnstable,
                "config_fence_changed_after_rebuild",
            ));
        }
        self.progress.record_snapshot_rebuild();
        self.durable_generation = profile.durable_generation;
        self.snapshot_id = format!("snapshot-generation-{}", profile.durable_generation);
        self.pass_capacity.clear();
        self.trace_event(
            ControllerTransition::SnapshotRebuild,
            "candidate_fence_changed",
        );
        Ok(ControllerDecision::Replan {
            reason: ControllerTransition::SnapshotRebuild,
        })
    }

    fn acquire_retry_permit_after_capacity(
        &self,
        lease: &mut CapacityLease,
    ) -> Result<Option<RetryPermit>, ControllerFailure> {
        match self
            .retry_budget
            .acquire_for_round(self.progress.view().attempt_count)
        {
            Ok(RetryPermitDecision::NotRequired) => Ok(None),
            Ok(RetryPermitDecision::Acquired(permit)) => Ok(Some(permit)),
            Err(RetryBudgetMiss::Exhausted { .. }) => {
                lease.release();
                Err(self.failure(
                    ControllerFailureKind::TemporaryHealth,
                    "retry_budget_exhausted",
                ))
            }
        }
    }

    fn planner_failure(&self, error: RoutePlannerError) -> ControllerFailure {
        match error {
            RoutePlannerError::CandidateLimitExceeded { actual, limit } => ControllerFailure {
                kind: ControllerFailureKind::CandidateLimit,
                evidence: vec![ControllerEvidence::new(
                    "candidate_limit",
                    format!("{actual}>{limit}"),
                )],
            },
        }
    }

    fn wait_failure(&self, miss: CapacityWaitMiss) -> ControllerFailure {
        let detail = match miss {
            CapacityWaitMiss::NotAdmitted => "wait_not_admitted",
            CapacityWaitMiss::QueueFull => "wait_queue_full",
        };
        self.failure(ControllerFailureKind::CapacityExhausted, detail)
    }

    fn failure(&self, kind: ControllerFailureKind, detail: impl Into<String>) -> ControllerFailure {
        ControllerFailure {
            kind,
            evidence: vec![ControllerEvidence::new("failure", detail)],
        }
    }

    fn trace_event(&mut self, transition: ControllerTransition, detail: impl Into<String>) {
        self.trace.push(ControllerTraceEvent {
            transition,
            evidence: vec![ControllerEvidence::new("reason", detail)],
        });
        if self.trace.len() > 64 {
            self.trace.remove(0);
        }
    }
}

#[derive(Debug)]
pub(crate) struct ControllerPlanningInput<'a> {
    pub(crate) candidates: &'a [crate::application::operational_facts::candidate_projector::RouteCandidateProjection],
    pub(crate) affinity_station_key_id: Option<&'a str>,
    pub(crate) profiles: &'a BTreeMap<String, CandidateAdmissionProfile>,
    pub(crate) capacity: &'a CompositeCapacityRegistry,
    pub(crate) current_runtime_overlay_revision: u64,
    pub(crate) now_ms: i64,
    pub(crate) max_waiters_per_constraint: u32,
}

#[derive(Debug)]
pub(crate) enum ControllerDecision {
    Selected(SelectedRoute),
    Wait {
        constraint: CapacityConstraintKey,
        permit: CapacityWaitPermit,
    },
    Replan {
        reason: ControllerTransition,
    },
}

#[derive(Debug)]
pub(crate) struct SelectedRoute {
    pub(crate) candidate: RoutePlanCandidate,
    pub(crate) lease: CapacityLease,
    pub(crate) retry_permit: Option<RetryPermit>,
    pub(crate) evidence: Vec<ControllerEvidence>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ActualAttemptTerminal {
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
pub(crate) struct ControllerFailure {
    pub(crate) kind: ControllerFailureKind,
    pub(crate) evidence: Vec<ControllerEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ControllerFailureKind {
    NoEligible,
    TemporaryHealth,
    CapacityExhausted,
    Deadline,
    ConfigUnstable,
    CandidateLimit,
    AttemptLimit,
    CommitUncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ControllerTransition {
    RuntimeReplan,
    SnapshotRebuild,
    PlanSkip,
    CapacityMiss,
    CapacityAcquired,
    WaitEntered,
    WaitWakeup,
    AttemptTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControllerTraceEvent {
    pub(crate) transition: ControllerTransition,
    pub(crate) evidence: Vec<ControllerEvidence>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ControllerEvidence {
    pub(crate) code: &'static str,
    pub(crate) detail: String,
}

impl ControllerEvidence {
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
