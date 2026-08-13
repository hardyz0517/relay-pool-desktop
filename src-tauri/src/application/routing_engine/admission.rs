use std::collections::{BTreeMap, BTreeSet};

use crate::application::routing_engine::{
    candidate_plan::{AvailabilityTier as LegacyAvailabilityTier, RoutePlanCandidate},
    capacity::{
        CapacityAcquireFailure, CapacityConstraintKey, CapacityLease, CapacityMissObservation,
        CapacityWaitMiss, CapacityWaitPermit, CompositeCapacityRegistry, CompositeCapacityRequest,
        PlanningRoundCapacityState, ProviderAccountConstraint, RetryBudgetMiss,
        RetryBudgetRegistry, RetryPermit, RetryPermitDecision,
    },
    exploration::ExplorationBudgetRegistry,
    failure_domains::CapacityDomainCommitment,
    intelligent_planner::{plan_snapshot_with_budget, PlannedCandidate, PlannerError, RoutePlan},
    planning_snapshot::PlanningSnapshot,
    request::{RouteProgress, RouteRequestFacts},
};

#[cfg(test)]
use crate::application::routing_engine::candidate_plan::RoutePlanPricingSnapshot;

#[cfg(test)]
use crate::application::routing_engine::{
    candidate_plan::RoutePlannerError,
    hierarchical_preview::{ordered_plan_candidates, plan_route, PlanningInput},
    request::PlanningRoundContext,
};

const MAX_RUNTIME_ONLY_REPLANS: u32 = 8;
const DEFAULT_MAX_ATTEMPTS: u32 = 4;

fn ordered_planned_candidates(plan: &RoutePlan) -> Vec<&PlannedCandidate> {
    let Some(best_tier) = plan.candidates.iter().map(|candidate| candidate.tier).min() else {
        return Vec::new();
    };
    let mut ordered = Vec::with_capacity(plan.candidates.len());
    if let Some(selected) = plan
        .candidates
        .iter()
        .find(|candidate| candidate.station_key_id == plan.dispatch.selected_id)
    {
        ordered.push(selected);
    }
    ordered.extend(plan.candidates.iter().filter(|candidate| {
        candidate.tier == best_tier && candidate.station_key_id != plan.dispatch.selected_id
    }));
    ordered.extend(plan.candidates.iter().filter(|candidate| {
        candidate.tier != best_tier && candidate.station_key_id != plan.dispatch.selected_id
    }));
    ordered
}

#[cfg(test)]
fn route_plan_candidate_from_projection(
    projected: &crate::application::operational_facts::candidate_projector::RouteCandidateProjection,
    planned: &PlannedCandidate,
    snapshot_id: &str,
) -> RoutePlanCandidate {
    let tier = match planned.tier {
        crate::application::routing_engine::tiers::AvailabilityTier::Primary => {
            LegacyAvailabilityTier::Primary
        }
        crate::application::routing_engine::tiers::AvailabilityTier::Backup => {
            LegacyAvailabilityTier::ConfiguredBackup
        }
    };
    RoutePlanCandidate {
        station_key_id: projected.identity.station_key_id.clone(),
        station_id: projected.identity.station_id.clone(),
        endpoint_revision: projected.identity.endpoint_revision,
        credential_revision: 1,
        account_revision: 1,
        group_binding_id: None,
        group_revision: None,
        resolved_upstream_model: None,
        model_alias_revision: 1,
        capacity_domain: None,
        capacity_domain_revision: None,
        priority: projected.priority,
        tier,
        pricing: RoutePlanPricingSnapshot {
            basis: projected.pricing.basis,
            currency: projected.pricing.currency.clone(),
            unit: projected.pricing.unit.clone(),
            estimated_input_price: projected.pricing.estimated_input_price,
            estimated_output_price: projected.pricing.estimated_output_price,
            estimated_fixed_price: projected.pricing.estimated_fixed_price,
            estimated_cache_creation_price: projected.pricing.estimated_cache_creation_price,
            estimated_cache_read_price: projected.pricing.estimated_cache_read_price,
            status_label: projected.pricing.status_label.clone(),
        },
        evidence: vec![
            crate::application::routing_engine::candidate_plan::DecisionEvidence {
                code: "planner_snapshot",
                detail: snapshot_id.to_string(),
            },
            crate::application::routing_engine::candidate_plan::DecisionEvidence {
                code: "utility_score",
                detail: planned.utility.value().to_string(),
            },
        ],
    }
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
    retry_budget: RetryBudgetRegistry,
    fallback_policy: FallbackPolicy,
    fallback_blocked: Option<AdmissionFailureKind>,
    max_attempts: Option<u32>,
    candidate_failure_domains: BTreeMap<String, Vec<String>>,
    excluded_failure_domains: BTreeSet<String>,
    excluded_capacity_domains: BTreeSet<CapacityDomainCommitment>,
    capacity_cross_domain_consumed: bool,
    trace: Vec<AdmissionTraceEvent>,
}

impl RouteAdmissionCoordinator {
    #[cfg(test)]
    pub fn new(
        request: RouteRequestFacts,
        settings: AdmissionSettings,
        initial_active_or_pending_capacity: u32,
    ) -> Self {
        Self::new_with_retry_budget(
            request,
            settings,
            RetryBudgetRegistry::new(initial_active_or_pending_capacity),
        )
    }

    pub fn new_with_retry_budget(
        request: RouteRequestFacts,
        settings: AdmissionSettings,
        retry_budget: RetryBudgetRegistry,
    ) -> Self {
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
            retry_budget,
            fallback_policy: settings.fallback_policy,
            fallback_blocked: None,
            max_attempts: None,
            candidate_failure_domains: BTreeMap::new(),
            excluded_failure_domains: BTreeSet::new(),
            excluded_capacity_domains: BTreeSet::new(),
            capacity_cross_domain_consumed: false,
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

        let planning_snapshot = input.planning_snapshot.ok_or_else(|| {
            self.failure(
                AdmissionFailureKind::ConfigUnstable,
                "planning_snapshot_required",
            )
        })?;
        self.candidate_failure_domains = planning_snapshot
            .candidates
            .iter()
            .map(|candidate| {
                (
                    candidate.station_key_id.clone(),
                    candidate.failure_domains.clone(),
                )
            })
            .collect();
        let mut working_snapshot = planning_snapshot.clone();
        working_snapshot.candidates.retain(|candidate| {
            !self
                .progress
                .view()
                .excludes_station_key(&candidate.station_key_id)
                && !candidate_uses_excluded_failure_domain(
                    candidate,
                    &self.excluded_failure_domains,
                )
                && !candidate_uses_excluded_capacity_domain(
                    candidate,
                    &self.excluded_capacity_domains,
                )
                && (self.excluded_capacity_domains.is_empty()
                    || candidate.capacity_domain.is_some())
        });
        let plan = plan_snapshot_with_budget(
            &working_snapshot,
            input.root_seed,
            self.progress.view().ordinal as u64 + 1,
            input.exploration_budget,
        )
        .map_err(|error| self.intelligent_planner_failure(error))?;

        let eligible_count = plan.candidates.len();
        if eligible_count == 0 {
            return Err(self.failure(AdmissionFailureKind::NoEligible, "no_eligible_candidate"));
        }
        let max_attempts = *self.max_attempts.get_or_insert(DEFAULT_MAX_ATTEMPTS);
        if self.progress.view().attempt_count >= max_attempts {
            return Err(self.failure(AdmissionFailureKind::AttemptLimit, "attempt_limit_reached"));
        }

        for planned in ordered_planned_candidates(&plan) {
            let candidate = if let Some(base) = input
                .execution_candidates
                .iter()
                .find(|candidate| candidate.station_key_id == planned.station_key_id)
            {
                let mut candidate = base.clone();
                candidate.tier = match planned.tier {
                    crate::application::routing_engine::tiers::AvailabilityTier::Primary => {
                        LegacyAvailabilityTier::Primary
                    }
                    crate::application::routing_engine::tiers::AvailabilityTier::Backup => {
                        LegacyAvailabilityTier::ConfiguredBackup
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
                ];
                candidate.capacity_domain = planning_snapshot
                    .candidates
                    .iter()
                    .find(|raw| raw.station_key_id == candidate.station_key_id)
                    .and_then(|raw| raw.capacity_domain.clone());
                candidate.capacity_domain_revision = planning_snapshot
                    .candidates
                    .iter()
                    .find(|raw| raw.station_key_id == candidate.station_key_id)
                    .and_then(|raw| raw.capacity_domain_revision);
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
                Ok(mut lease) => {
                    let is_capacity_cross_domain_fallback =
                        !self.excluded_capacity_domains.is_empty();
                    if is_capacity_cross_domain_fallback {
                        // All candidates sharing an exhausted trusted domain were
                        // removed above. The only remaining route is therefore the
                        // one allowed cross-domain terminal fallback.
                        self.capacity_cross_domain_consumed = true;
                    }
                    let retry_permit = self.acquire_retry_permit_after_capacity(&mut lease)?;
                    let selected_station_key_id = candidate.station_key_id.clone();
                    self.trace_event(
                        AdmissionTransition::CapacityAcquired,
                        "capacity_lease_acquired",
                    );
                    return Ok(AdmissionDecision::Selected(SelectedRoute {
                        candidate,
                        lease,
                        retry_permit,
                        evidence: vec![AdmissionEvidence::new("selected", selected_station_key_id)],
                        is_capacity_cross_domain_fallback,
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

    /// The old hierarchical planner is retained only for unit fixtures that
    /// intentionally exercise the transport shell without a policy snapshot.
    /// Production callers must use `next`, which requires a canonical
    /// `PlanningSnapshot` and invokes the intelligent planner directly.
    #[cfg(test)]
    pub fn next_legacy(
        &mut self,
        input: AdmissionPlanningInput<'_>,
    ) -> Result<AdmissionDecision, AdmissionFailure> {
        if input.planning_snapshot.is_some() {
            return self.next(input);
        }
        if let Some(kind) = self.fallback_blocked.clone() {
            return Err(self.failure(kind, "fallback_blocked"));
        }
        if input.now_ms >= self.progress.view().deadline_ms {
            return Err(self.failure(AdmissionFailureKind::Deadline, "deadline_elapsed"));
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
        if plan
            .strata
            .iter()
            .map(|stratum| stratum.candidates.len())
            .sum::<usize>()
            == 0
        {
            return Err(self.failure(AdmissionFailureKind::NoEligible, "no_eligible_candidate"));
        }
        for candidate in ordered_plan_candidates(&plan) {
            let Some(profile) = input.profiles.get(&candidate.station_key_id) else {
                return Err(self.failure(AdmissionFailureKind::ConfigUnstable, "missing_profile"));
            };
            if self.capacity_state_blocks_candidate(&candidate, profile) {
                continue;
            }
            if self.profile_invalidates_candidate(&candidate, profile) {
                return self.rebuild_or_fail_config(profile);
            }
            match input
                .capacity
                .try_acquire(profile.capacity_request(candidate))
            {
                Ok(mut lease) => {
                    let retry_permit = self.acquire_retry_permit_after_capacity(&mut lease)?;
                    return Ok(AdmissionDecision::Selected(SelectedRoute {
                        candidate: candidate.clone(),
                        lease,
                        retry_permit,
                        evidence: vec![AdmissionEvidence::new(
                            "selected",
                            candidate.station_key_id.clone(),
                        )],
                        is_capacity_cross_domain_fallback: false,
                    }));
                }
                Err(failure) => {
                    self.pass_capacity.record_miss(miss_observation(&failure));
                }
            }
        }
        Err(self.failure(
            AdmissionFailureKind::CapacityExhausted,
            "all_strata_capacity_exhausted",
        ))
    }

    #[cfg(test)]
    pub fn record_wait_wakeup(&mut self, runtime_overlay_revision: u64) {
        self.pass_capacity.clear();
        self.runtime_overlay_revision = runtime_overlay_revision;
        self.trace_event(AdmissionTransition::WaitWakeup, "wait_wakeup_replan");
    }

    #[cfg(test)]
    pub fn record_actual_terminal(
        &mut self,
        selected: SelectedRoute,
        outcome: ActualAttemptTerminal,
    ) -> Result<(), AdmissionFailure> {
        let station_key_id = selected.candidate.station_key_id.clone();
        drop(selected);
        self.record_actual_terminal_for_station_key(station_key_id, outcome)
    }

    pub fn record_actual_terminal_for_station_key(
        &mut self,
        station_key_id: String,
        outcome: ActualAttemptTerminal,
    ) -> Result<(), AdmissionFailure> {
        if outcome == ActualAttemptTerminal::FailedBeforeCommit {
            if let Some(domains) = self.candidate_failure_domains.get(&station_key_id) {
                self.excluded_failure_domains
                    .extend(domains.iter().cloned());
            }
        }
        self.progress.record_actual_attempt(station_key_id);
        self.pass_capacity.clear();
        self.trace_event(AdmissionTransition::AttemptTerminal, outcome.as_code());
        if self.capacity_cross_domain_consumed {
            self.fallback_blocked = Some(AdmissionFailureKind::AttemptLimit);
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

    /// Removes all snapshot candidates in one authoritative capacity domain.
    /// The next selection, if any, can only be a different domain and its
    /// terminal result closes this coordinator's retry chain.
    pub fn exclude_exhausted_capacity_domain(&mut self, domain: CapacityDomainCommitment) -> bool {
        if self.capacity_cross_domain_consumed {
            return false;
        }
        self.excluded_capacity_domains.insert(domain);
        true
    }

    #[cfg(test)]
    pub fn progress_view(&self) -> crate::application::routing_engine::request::RouteProgressView {
        self.progress.view()
    }

    #[cfg(test)]
    pub fn pass_capacity_state(&self) -> &PlanningRoundCapacityState {
        &self.pass_capacity
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

    fn acquire_retry_permit_after_capacity(
        &self,
        lease: &mut CapacityLease,
    ) -> Result<Option<RetryPermit>, AdmissionFailure> {
        match self
            .retry_budget
            .acquire_for_round(self.progress.view().attempt_count)
        {
            Ok(RetryPermitDecision::NotRequired) => Ok(None),
            Ok(RetryPermitDecision::Acquired(permit)) => Ok(Some(permit)),
            Err(RetryBudgetMiss::Exhausted { .. }) => {
                lease.release();
                Err(self.failure(
                    AdmissionFailureKind::TemporaryHealth,
                    "retry_budget_exhausted",
                ))
            }
        }
    }

    fn intelligent_planner_failure(&self, error: PlannerError) -> AdmissionFailure {
        match error {
            PlannerError::InvalidSnapshot(detail) => AdmissionFailure {
                kind: AdmissionFailureKind::ConfigUnstable,
                evidence: vec![AdmissionEvidence::new("invalid_planning_snapshot", detail)],
            },
            PlannerError::NoEligibleCandidate => {
                self.failure(AdmissionFailureKind::NoEligible, "no_eligible_candidate")
            }
            PlannerError::RuntimeAtCapacity => self.failure(
                AdmissionFailureKind::CapacityExhausted,
                "runtime_at_capacity",
            ),
        }
    }

    #[cfg(test)]
    fn planner_failure(&self, error: RoutePlannerError) -> AdmissionFailure {
        match error {
            RoutePlannerError::CandidateLimitExceeded { actual, limit } => AdmissionFailure {
                kind: AdmissionFailureKind::ConfigUnstable,
                evidence: vec![AdmissionEvidence::new(
                    "candidate_limit",
                    format!("{actual}>{limit}"),
                )],
            },
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

fn candidate_uses_excluded_failure_domain(
    candidate: &crate::application::routing_engine::planning_snapshot::CandidateSnapshot,
    excluded_failure_domains: &BTreeSet<String>,
) -> bool {
    candidate
        .failure_domains
        .iter()
        .any(|domain| excluded_failure_domains.contains(domain))
}

fn candidate_uses_excluded_capacity_domain(
    candidate: &crate::application::routing_engine::planning_snapshot::CandidateSnapshot,
    excluded_capacity_domains: &BTreeSet<CapacityDomainCommitment>,
) -> bool {
    candidate
        .capacity_domain
        .as_ref()
        .is_some_and(|domain| excluded_capacity_domains.contains(domain))
}

#[derive(Debug)]
pub struct AdmissionPlanningInput<'a> {
    pub execution_candidates: &'a [RoutePlanCandidate],
    pub planning_snapshot: Option<&'a PlanningSnapshot>,
    pub root_seed: &'a [u8],
    pub exploration_budget: Option<&'a ExplorationBudgetRegistry>,
    #[cfg(test)]
    pub affinity_station_key_id: Option<&'a str>,
    pub profiles: &'a BTreeMap<String, CandidateAdmissionProfile>,
    pub capacity: &'a CompositeCapacityRegistry,
    pub current_runtime_overlay_revision: u64,
    pub now_ms: i64,
    pub max_waiters_per_constraint: u32,
    #[cfg(test)]
    pub candidates: &'a [crate::application::operational_facts::candidate_projector::RouteCandidateProjection],
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
    pub retry_permit: Option<RetryPermit>,
    pub evidence: Vec<AdmissionEvidence>,
    /// This route was selected only after a trusted capacity-domain exclusion.
    /// It is not set when a domain is merely excluded but no alternative exists.
    pub is_capacity_cross_domain_fallback: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActualAttemptTerminal {
    FailedBeforeCommit,
    /// A provider-capacity rejection that is eligible for a bounded retry of
    /// the exact same resolved target. It consumes an outbound attempt but
    /// must not exclude the target or its credential failure domains.
    RetrySameTargetCapacity,
    PossiblyAccepted,
    Succeeded,
}

impl ActualAttemptTerminal {
    fn as_code(self) -> &'static str {
        match self {
            Self::FailedBeforeCommit => "failed_before_commit",
            Self::RetrySameTargetCapacity => "retry_same_target_capacity",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::routing_engine::planning_snapshot::CandidateSnapshot;

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
            pricing: crate::application::routing_engine::candidate_plan::RoutePlanPricingSnapshot::unpriced("test"),
            preference_basis_points: 5_000,
            failure_domains: domains.iter().map(|value| (*value).to_string()).collect(),
        }
    }

    #[test]
    fn retry_excludes_candidates_in_a_failed_candidate_failure_domain() {
        let excluded = BTreeSet::from(["station:shared".to_string()]);
        assert!(candidate_uses_excluded_failure_domain(
            &candidate(&["station:shared", "key:key-a"]),
            &excluded,
        ));
        assert!(!candidate_uses_excluded_failure_domain(
            &candidate(&["station:other", "key:key-b"]),
            &excluded,
        ));
    }
}
