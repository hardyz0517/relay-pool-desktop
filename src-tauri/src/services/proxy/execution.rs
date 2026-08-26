use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    future::Future,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use bytes::Bytes;
use futures_util::{future::BoxFuture, stream, StreamExt};
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use super::{
    adapters::{
        error_envelope::{BodyCapture, ErrorEnvelopeInput, FailureTransport},
        error_rules::collect_upstream_failure_evidence_for_profile,
        openai::openai_semantic_signal_from_evidence,
        responses::{render_responses_response, responses_error_semantic_signal},
    },
    endpoint_adapter::{response_headers_for_downstream, EndpointAdapter},
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    lifecycle::{
        attempt::{
            AttemptContext, AttemptFailureKind, AttemptTerminal, AttemptTerminalRecord,
            ClassifiedAttemptFailure, FailureBlame, HealthEffect, RetryDisposition,
        },
        ports::LifecycleWriteError,
        request::{AttemptId, RequestLogAnnotations},
        writer::{LifecycleWriter, WriterAdmissionError},
    },
    protocol::{
        failure_event_json, BootstrapDisposition, CompletionPolicy, DownstreamTransform,
        ProtocolTerminal, SseBootstrapMachine,
    },
    request::{ByteStream, CanonicalProxyRequest},
    request_send::RequestSendPhase,
    responses_chat_stream::chat_sse_to_responses_stream,
    routing_repository::{OperationalRouteSnapshot, RoutingExecutionSettings, RoutingRepository},
    routing_runtime::RoutingRuntimeState,
    transport_policy::TransportPolicySnapshot,
    upstream::{UpstreamAttempt, UpstreamClientPool},
};

use crate::{
    application::{
        credentials::ExecutionCredentialResolver,
        health_protection::HealthProbeAdmissionMode,
        model_mapping,
        operational_facts::target_resolver::{
            ExecutionTargetHandle, ExecutionTargetRef, ExecutionTargetResolver,
            LeasedSelectedTarget, RequestBodyIdentity, TargetProtocolProfile,
        },
        request_finalization::effect_planner::classified_attempt_failure_from_canonical,
        request_finalization::failure::{
            failure_from_provider_signal, planning_failure, CapabilityApplicabilitySet,
            FailureClass, FailureTarget, ProviderErrorSemanticSignal,
            RetryDisposition as CanonicalRetryDisposition,
        },
        request_lifecycle::attempt::project_retry_disposition,
        routing_engine::{
            admission::{
                ActualAttemptTerminal, AdmissionDecision, AdmissionEvidence, AdmissionFailure,
                AdmissionFailureKind, AdmissionPlanningInput, AdmissionSettings, FallbackPolicy,
                RouteAdmissionCoordinator, SelectedRoute,
            },
            affinity::{AffinityKind, AffinityLookup, AffinityRegistry},
            candidate_plan::{RoutePlanCandidate, RoutePlanPricingSnapshot},
            capacity::CompositeCapacityRegistry,
            failure_domains::CapacityDomainCommitment,
            request::{
                CanonicalRouteRequest, GroupFilterMode, OrderingProfile, PlanningRequestContext,
                RouteKind, RouteRequestClassifier, RouteRequestFacts, ValidatedLocalRouteSettings,
            },
            routing_failure::RoutePlanningFailure,
        },
        routing_execution_reader::RoutingExecutionReadError,
        routing_policy::AttemptBudgetProfileV1,
    },
    models::{
        pricing::BalanceSnapshot,
        proxy::UpstreamApiFormat,
        routing::{RouteEndpointKind, RoutingGroupFilter, RoutingPolicy},
    },
    observability::decision_trace::{
        DecisionTraceBuilder, DecisionTraceEvent, DecisionTraceEventKind,
    },
    services::time::now_millis_for_services,
};

#[derive(Clone)]
pub(crate) struct ExecutionEngine {
    repository: Arc<dyn RoutingRepository>,
    credentials: Arc<dyn ExecutionCredentialResolver>,
    attempts: Arc<dyn AttemptExecutor>,
    transport_policy: TransportPolicySnapshot,
    capacity: Arc<CompositeCapacityRegistry>,
    affinity: Arc<Mutex<AffinityRegistry>>,
    lifecycle_writer: Option<LifecycleWriter>,
    routing_runtime: Arc<RoutingRuntimeState>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum HealthProbeAcquireOutcome {
    None,
    Lease(crate::application::health_protection::HealthProtectionProbe),
    LeaseRace,
}

pub(crate) trait AttemptExecutor: Send + Sync {
    fn attempt<'a>(
        &'a self,
        request: &'a CanonicalProxyRequest,
        target: &'a ExecutionTargetHandle,
        mapped_model: Option<&'a str>,
    ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>>;
}

pub(crate) enum PreparedAttempt {
    Buffered {
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
    },
    Stream {
        status: StatusCode,
        headers: HeaderMap,
        chunks: ByteStream,
        completion_policy: CompletionPolicy,
        diagnostic_memory:
            Option<crate::services::proxy::diagnostic_memory::DiagnosticMemoryPermit>,
    },
}

pub(crate) struct ProxyExecutionResponse {
    pub status: StatusCode,
    pub headers: HeaderMap,
    pub body: ProxyExecutionBody,
    selected_station_key_id: Option<String>,
    selected_station_id: Option<String>,
    fallback_count: i64,
    pub lifecycle: ExecutionLifecycleEvidence,
}

#[derive(Debug, Clone)]
pub(crate) struct ExecutionLifecycleEvidence {
    pub annotations: RequestLogAnnotations,
    pub selected_attempt: Option<AttemptContext>,
    pub selected_attempt_cost: Option<super::attempt::SelectedAttemptCostSnapshot>,
    pub attempt_count: u16,
    pub(crate) fallback_count: u16,
}

#[derive(Debug, Clone, Copy)]
struct AttemptTimings {
    request_started_at_ms: i64,
    upstream_headers_ms: i64,
    first_token_ms: i64,
}

pub(crate) enum ProxyExecutionBody {
    Buffered(Bytes),
    Stream {
        chunks: ByteStream,
        diagnostic_memory:
            Option<crate::services::proxy::diagnostic_memory::DiagnosticMemoryPermit>,
    },
}

const MAX_EXECUTION_REPLANS: usize = 3;

/// `/models` is a read-only aggregation protocol, not an inference replay
/// consumer. The adapter is deliberately typed so a future caller cannot
/// silently treat its aggregate disposition as an ordinary route action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModelsAggregationDisposition {
    ContinueCandidate,
    StopAggregation,
}

struct ModelsRetryAdapter;

impl ModelsRetryAdapter {
    fn disposition(action: &RetryAction) -> ModelsAggregationDisposition {
        match action.kind {
            RetryActionKind::StopRequest => ModelsAggregationDisposition::StopAggregation,
            RetryActionKind::RetrySameTarget
            | RetryActionKind::WaitThenReplan
            | RetryActionKind::TryDifferentFailureDomain => {
                ModelsAggregationDisposition::ContinueCandidate
            }
        }
    }

    fn allows_next_candidate(action: &RetryAction) -> bool {
        matches!(
            Self::disposition(action),
            ModelsAggregationDisposition::ContinueCandidate
        )
    }
}

/// The execution-level intent selected after canonical classification and the
/// replay gate have run.  This enum deliberately contains no request state;
/// that state lives in `RetryAction` below so every consumer gets the same
/// evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryActionKind {
    RetrySameTarget,
    WaitThenReplan,
    TryDifferentFailureDomain,
    StopRequest,
}

impl RetryActionKind {
    fn as_trace_label(self) -> &'static str {
        match self {
            Self::RetrySameTarget => "retry_same_target",
            Self::WaitThenReplan => "wait_then_replan",
            Self::TryDifferentFailureDomain => "try_different_failure_domain",
            Self::StopRequest => "stop_request",
        }
    }

    fn allows_replan(self) -> bool {
        !matches!(self, Self::StopRequest)
    }

    fn retries_same_target(self) -> bool {
        matches!(self, Self::RetrySameTarget)
    }

    fn canonical_retry(self) -> CanonicalRetryDisposition {
        match self {
            Self::RetrySameTarget => CanonicalRetryDisposition::RetrySameTarget,
            Self::WaitThenReplan => CanonicalRetryDisposition::WaitThenReplan,
            Self::TryDifferentFailureDomain => CanonicalRetryDisposition::TryDifferentFailureDomain,
            Self::StopRequest => CanonicalRetryDisposition::StopRequest,
        }
    }
}

/// The result of the single replay-safety gate.  A rejected gate is retained
/// on a stop action so the UI/terminal diagnostics can explain why a user
/// retry setting did not authorize another outbound request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplayGateResult {
    Allowed,
    Rejected { reason_key: &'static str },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetryActionContext {
    pub(crate) attempt_ordinal: u16,
    pub(crate) policy_revision: u64,
    pub(crate) remaining_attempt_budget: u32,
    pub(crate) remaining_precommit_budget_ms: Option<u64>,
    pub(crate) excluded_failure_domain_count: u16,
}

#[cfg(test)]
impl Default for RetryActionContext {
    fn default() -> Self {
        Self {
            attempt_ordinal: 0,
            policy_revision: 1,
            remaining_attempt_budget: 4,
            remaining_precommit_budget_ms: None,
            excluded_failure_domain_count: 0,
        }
    }
}

/// A fully explained action for one failed attempt.  It is intentionally an
/// owned value rather than a bare enum: execution, lifecycle diagnostics and
/// tests must observe the same failure code, replay result, budget and delay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RetryAction {
    pub(crate) kind: RetryActionKind,
    pub(crate) reason_key: &'static str,
    pub(crate) explanation_key: &'static str,
    pub(crate) failure_code: &'static str,
    pub(crate) replay: ReplayGateResult,
    pub(crate) attempt_ordinal: u16,
    pub(crate) policy_revision: u64,
    pub(crate) remaining_attempt_budget: u32,
    pub(crate) remaining_precommit_budget_ms: Option<u64>,
    pub(crate) excluded_failure_domain_count: u16,
    pub(crate) wait_delay_ms: Option<u64>,
}

impl RetryAction {
    fn allows_replan(&self) -> bool {
        self.kind.allows_replan() && self.remaining_attempt_budget > 0
    }

    fn retries_same_target(&self) -> bool {
        self.kind.retries_same_target() && self.remaining_attempt_budget > 0
    }

    fn lifecycle_retry(&self) -> RetryDisposition {
        project_retry_disposition(self.kind.canonical_retry())
    }

    fn stop(
        failure: &ProxyFailure,
        context: RetryActionContext,
        replay: ReplayGateResult,
        reason_key: &'static str,
        explanation_key: &'static str,
    ) -> Self {
        Self {
            kind: RetryActionKind::StopRequest,
            reason_key,
            explanation_key,
            failure_code: failure
                .canonical()
                .map(|failure| failure.public.code.as_str())
                .unwrap_or_else(|| failure.code.as_str()),
            replay,
            attempt_ordinal: context.attempt_ordinal,
            policy_revision: context.policy_revision,
            remaining_attempt_budget: context.remaining_attempt_budget,
            remaining_precommit_budget_ms: context.remaining_precommit_budget_ms,
            excluded_failure_domain_count: context.excluded_failure_domain_count,
            wait_delay_ms: None,
        }
    }
}

/// Single owner for translating canonical retry intent into an execution
/// action. Replay safety is deliberately kept in this planner, next to the
/// existing classifier boundary, so callers cannot bypass the gate by looking
/// at an HTTP status or `RetryClass` projection.
pub(crate) struct RetryActionPlanner;

impl RetryActionPlanner {
    /// Compatibility helper for focused planner tests. Production callers use
    /// `plan_with_context` so ordinal, revision, deadline and remaining budget
    /// cannot silently disappear at the execution boundary.
    #[cfg(test)]
    fn plan(failure: &ProxyFailure, idempotent: bool, committed: bool) -> RetryAction {
        Self::plan_with_context(
            failure,
            idempotent,
            committed,
            RetryActionContext::default(),
        )
    }

    fn plan_with_context(
        failure: &ProxyFailure,
        idempotent: bool,
        committed: bool,
        context: RetryActionContext,
    ) -> RetryAction {
        if committed || failure.retry_class == RetryClass::AfterCommitStop {
            return RetryAction::stop(
                failure,
                context,
                ReplayGateResult::Rejected {
                    reason_key: "committed_request",
                },
                "committed_request",
                "routing.retry.committedStop",
            );
        }
        let Some(canonical) = failure.canonical() else {
            // Fail closed for local/legacy failures until they have an explicit
            // canonical producer. The execution layer must never infer
            // upstream semantics from a projected HTTP status.
            return RetryAction::stop(
                failure,
                context,
                ReplayGateResult::Rejected {
                    reason_key: "missing_canonical_failure",
                },
                "missing_canonical_failure",
                "routing.retry.missingCanonicalFailure",
            );
        };
        let not_replayable = matches!(
            canonical.replay_safety,
            crate::application::request_finalization::failure::ReplaySafety::NotReplayable
        );
        let provider_proves_pre_acceptance = matches!(
            canonical.replay_safety,
            crate::application::request_finalization::failure::ReplaySafety::ReplaySafe
        ) && matches!(
            canonical.request_acceptance,
            crate::application::request_finalization::failure::RequestAcceptance::RejectedBeforeAcceptance
        );
        // A definitely-unsent transport boundary overrides an uncertain
        // provider acceptance classification. Unknown or any sent phase
        // remains closed for non-idempotent operations until a versioned
        // provider-idempotency capability is available.
        let replay_allowed = !not_replayable
            && (idempotent
                || failure
                    .request_send_phase
                    .definitely_no_request_bytes_sent()
                || provider_proves_pre_acceptance);
        if !replay_allowed {
            return RetryAction::stop(
                failure,
                context,
                ReplayGateResult::Rejected {
                    reason_key: "replay_gate_rejected",
                },
                "replay_gate_rejected",
                "routing.retry.replaySuppressed",
            );
        }

        let kind = match canonical.retry {
            CanonicalRetryDisposition::RetrySameTarget => RetryActionKind::RetrySameTarget,
            CanonicalRetryDisposition::TryDifferentFailureDomain => {
                RetryActionKind::TryDifferentFailureDomain
            }
            CanonicalRetryDisposition::WaitThenReplan => RetryActionKind::WaitThenReplan,
            CanonicalRetryDisposition::StopRequest => RetryActionKind::StopRequest,
        };
        if matches!(kind, RetryActionKind::StopRequest) {
            return RetryAction::stop(
                failure,
                context,
                ReplayGateResult::Allowed,
                "classifier_stop",
                "routing.retry.classifierStop",
            );
        }
        if context.remaining_attempt_budget == 0 {
            return RetryAction::stop(
                failure,
                context,
                ReplayGateResult::Allowed,
                "attempt_budget_exhausted",
                "routing.retry.attemptBudgetExhausted",
            );
        }

        let requested_delay_ms = if matches!(
            kind,
            RetryActionKind::RetrySameTarget | RetryActionKind::WaitThenReplan
        ) {
            failure
                .retry_after_ms
                .and_then(|value| u64::try_from(value).ok())
        } else {
            None
        };
        let wait_delay_ms = requested_delay_ms.and_then(|delay| {
            context
                .remaining_precommit_budget_ms
                .map_or(Some(delay), |remaining| {
                    (delay < remaining).then_some(delay)
                })
        });
        if matches!(
            kind,
            RetryActionKind::RetrySameTarget | RetryActionKind::WaitThenReplan
        ) && requested_delay_ms.is_some()
            && wait_delay_ms.is_none()
        {
            return RetryAction::stop(
                failure,
                context,
                ReplayGateResult::Allowed,
                "deadline_budget_exhausted",
                "routing.retry.deadlineBudgetExhausted",
            );
        }

        let (reason_key, explanation_key) = match kind {
            RetryActionKind::RetrySameTarget => {
                ("capacity_transient", "routing.retry.sameTargetCapacity")
            }
            RetryActionKind::WaitThenReplan => ("rate_limit_wait", "routing.retry.waitThenReplan"),
            RetryActionKind::TryDifferentFailureDomain => (
                "failure_domain_unavailable",
                "routing.retry.tryDifferentFailureDomain",
            ),
            RetryActionKind::StopRequest => unreachable!("stop handled above"),
        };
        RetryAction {
            kind,
            reason_key,
            explanation_key,
            failure_code: canonical.public.code.as_str(),
            replay: ReplayGateResult::Allowed,
            attempt_ordinal: context.attempt_ordinal,
            policy_revision: context.policy_revision,
            remaining_attempt_budget: context.remaining_attempt_budget,
            remaining_precommit_budget_ms: context.remaining_precommit_budget_ms,
            excluded_failure_domain_count: context.excluded_failure_domain_count,
            wait_delay_ms,
        }
    }
}

impl ExecutionEngine {
    // Unit tests intentionally omit the production lifecycle writer.
    #[cfg(test)]
    pub(crate) fn new(
        repository: Arc<dyn RoutingRepository>,
        credentials: Arc<dyn ExecutionCredentialResolver>,
        attempts: Arc<dyn AttemptExecutor>,
    ) -> Self {
        Self {
            repository,
            credentials,
            attempts,
            transport_policy: TransportPolicySnapshot::default(),
            capacity: Arc::new(CompositeCapacityRegistry::default()),
            affinity: Arc::new(Mutex::new(AffinityRegistry::default())),
            lifecycle_writer: None,
            routing_runtime: Arc::new(RoutingRuntimeState::new(64, 1)),
        }
    }

    pub(crate) fn new_with_transport_policy_and_lifecycle(
        repository: Arc<dyn RoutingRepository>,
        credentials: Arc<dyn ExecutionCredentialResolver>,
        attempts: Arc<dyn AttemptExecutor>,
        transport_policy: TransportPolicySnapshot,
        lifecycle_writer: LifecycleWriter,
        routing_runtime: Arc<RoutingRuntimeState>,
    ) -> Self {
        Self {
            repository,
            credentials,
            attempts,
            transport_policy,
            capacity: routing_runtime.capacity_registry(),
            affinity: Arc::new(Mutex::new(AffinityRegistry::default())),
            lifecycle_writer: Some(lifecycle_writer),
            routing_runtime,
        }
    }

    #[cfg(test)]
    fn with_transport_policy(mut self, transport_policy: TransportPolicySnapshot) -> Self {
        self.transport_policy = transport_policy;
        self
    }

    pub(crate) async fn execute(
        &self,
        request: CanonicalProxyRequest,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let transport_policy = Arc::clone(request.transport_policy());
        let request_timing = request.request_timing();
        let request_started_at_ms = request_timing.started_at_ms;
        let precommit_started = request_timing.started_at;
        // Carry the ingress monotonic anchor through every planning/replan
        // call. The repository must never derive a new deadline per replan.
        let planning_context = PlanningRequestContext::from_started_at(
            precommit_started,
            transport_policy.request_deadline,
        );
        if request.local_path == "/usage" || request.local_path == "/v1/usage" {
            return self.execute_usage(request).await;
        }

        // Model catalogs are a control-plane aggregation endpoint. They must
        // never be filtered, rejected, or rewritten by inference mapping rules.
        if matches!(request.endpoint, RouteEndpointKind::Models) {
            let execution_settings = await_request_deadline(
                transport_policy.request_deadline,
                precommit_started,
                self.repository.load_execution_settings(),
                |error| internal_failure(format!("load routing settings failed: {error}")),
            )
            .await?;
            let route_facts =
                route_request_facts(&request, &execution_settings, request_started_at_ms, None);
            let (planning_snapshot, snapshot) = await_request_deadline(
                transport_policy.request_deadline,
                precommit_started,
                self.load_route_snapshots(
                    &request,
                    &execution_settings,
                    route_facts.clone(),
                    None,
                    planning_context,
                    None,
                    HealthProbeAdmissionMode::Normal,
                ),
                |failure| failure,
            )
            .await?;
            return self
                .execute_models(
                    request,
                    route_facts,
                    snapshot,
                    planning_snapshot,
                    &execution_settings,
                    None,
                    request_started_at_ms,
                    precommit_started,
                    planning_context,
                )
                .await;
        }

        let execution_settings = await_request_deadline(
            transport_policy.request_deadline,
            precommit_started,
            self.repository.load_execution_settings(),
            |error| internal_failure(format!("load routing settings failed: {error}")),
        )
        .await?;
        let resolved_model_plan = model_mapping::resolve_request(
            request.model.clone(),
            mapping_endpoint_kind(&request.endpoint),
            request.stream,
            request.requirements.uses_tools,
            request.requirements.uses_vision,
            request.requirements.uses_reasoning,
        )
        .map_err(|error| match error {
            model_mapping::ModelMappingResolutionError::InvalidModelName => ProxyFailure::new(
                ProxyFailureCode::RequestBodyInvalid,
                FailureSource::Local,
                RetryClass::Never,
                StatusCode::BAD_REQUEST,
                "requested model is invalid",
            ),
            model_mapping::ModelMappingResolutionError::TargetRequiresCandidateContext
            | model_mapping::ModelMappingResolutionError::ProfileNotFound
            | model_mapping::ModelMappingResolutionError::ProfileHasNoOffering
            | model_mapping::ModelMappingResolutionError::NoResolvedTargets => ProxyFailure::new(
                ProxyFailureCode::RequestBodyInvalid,
                FailureSource::Local,
                RetryClass::Never,
                StatusCode::BAD_REQUEST,
                "model mapping target cannot be resolved for this request",
            ),
        })?;
        model_mapping::record_request_trace(&request.request_id, &resolved_model_plan);
        if matches!(
            resolved_model_plan.disposition,
            model_mapping::Disposition::Reject
        ) {
            return Err(model_mapping_rejection_failure(&resolved_model_plan));
        }
        // Candidate projection owns the rank-aware target expansion. The
        // request entry point only seeds hard gates with rank zero; it must
        // not collapse a fallback chain or reject it as a multi-target plan.
        let mapped_model = resolved_model_plan
            .target_models
            .first()
            .map(|target| target.route_model.clone())
            .or_else(|| {
                request
                    .model
                    .as_deref()
                    .map(str::trim)
                    .filter(|model| !model.is_empty())
                    .map(ToOwned::to_owned)
            });
        let route_facts = route_request_facts(
            &request,
            &execution_settings,
            request_started_at_ms,
            mapped_model.as_deref(),
        )
        .with_model_mapping(
            resolved_model_plan.mapping_revision,
            resolved_model_plan.model_resolution_fence.clone(),
        )
        .with_mapping_requested_model(request.model.clone())
        .with_mapping_endpoint(mapping_endpoint_kind(&request.endpoint));
        let (mut planning_snapshot, mut snapshot) = await_request_deadline(
            transport_policy.request_deadline,
            precommit_started,
            self.load_route_snapshots(
                &request,
                &execution_settings,
                route_facts.clone(),
                mapped_model.as_deref(),
                planning_context,
                None,
                HealthProbeAdmissionMode::Normal,
            ),
            |failure| failure,
        )
        .await?;
        // A durable Half-Open lease is acquired only after the immutable
        // candidate snapshot is known. The lease is then fed back into one
        // planning pass so admission can admit exactly that candidate.
        let health_probe_acquisition = await_request_deadline(
            transport_policy.request_deadline,
            precommit_started,
            self.acquire_health_probe(&planning_snapshot),
            |failure| failure,
        )
        .await?;
        let mut health_probe = match health_probe_acquisition {
            HealthProbeAcquireOutcome::Lease(probe) => Some(probe),
            HealthProbeAcquireOutcome::None => None,
            HealthProbeAcquireOutcome::LeaseRace => {
                // The discovery snapshot may contain an expired Open scope
                // only to give this request a chance to reserve a lease. If
                // another request won that race, rebuild strictly so that
                // the stale candidate cannot be sent as an ordinary request.
                let refreshed = await_request_deadline(
                    transport_policy.request_deadline,
                    precommit_started,
                    self.load_route_snapshots(
                        &request,
                        &execution_settings,
                        route_facts.clone(),
                        mapped_model.as_deref(),
                        planning_context,
                        None,
                        HealthProbeAdmissionMode::StrictAfterLeaseRace,
                    ),
                    |failure| failure,
                )
                .await?;
                (planning_snapshot, snapshot) = refreshed;
                None
            }
        };
        if let Some(probe) = health_probe.as_ref() {
            let refreshed = await_request_deadline(
                transport_policy.request_deadline,
                precommit_started,
                self.load_route_snapshots(
                    &request,
                    &execution_settings,
                    route_facts.clone(),
                    mapped_model.as_deref(),
                    planning_context,
                    Some(probe.clone()),
                    HealthProbeAdmissionMode::Normal,
                ),
                |failure| failure,
            )
            .await;
            match refreshed {
                Ok(value) => (planning_snapshot, snapshot) = value,
                Err(failure) => {
                    if let Some(probe) = health_probe.take() {
                        self.cancel_health_protection_probe(probe).await?;
                    }
                    return Err(failure);
                }
            }
        }
        // Freeze the compiled request budget before any replan. A refreshed
        // planning snapshot may change candidates, but never the in-flight
        // request's attempt or capacity retry allowance.
        let attempt_budget = planning_snapshot.attempt_budget;
        // Per-request DecisionTraceProfileV1 record; in-memory ring only.
        let mut decision_trace = DecisionTraceBuilder::new(&request.request_id).ok();
        // Keep the effective immutable profile visible in the bounded trace.
        // This is deliberately coarse and contains no request/provider data.
        let profile_trace_detail = attempt_budget_trace_detail(attempt_budget);

        let idempotent = request.idempotency_key.is_some();
        let mut last_failure = None;
        let mut attempted_count = 0_i64;
        let mut controller = RouteAdmissionCoordinator::new_with_retry_budget(
            route_facts.clone(),
            AdmissionSettings {
                deadline_ms: precommit_deadline_ms(
                    request_started_at_ms,
                    transport_policy.request_deadline,
                ),
                initial_snapshot_id: planning_snapshot.snapshot_id.clone(),
                initial_runtime_overlay_revision: planning_snapshot.runtime.runtime_revision,
                initial_durable_generation: planning_snapshot.durable_revision,
                fallback_policy: FallbackPolicy {
                    has_stable_idempotency_key: idempotent,
                    non_idempotent: !idempotent,
                },
                attempt_budget,
            },
            self.routing_runtime.retry_budget(),
        );
        let root_seed = self.routing_runtime.root_seed();
        let exploration_budget = self.routing_runtime.exploration_budget();

        let mut attempt_index = 0_usize;
        let mut capacity_sleep_ms = 0_u64;
        let mut replan_count = 0_usize;
        'attempts: while attempt_index < attempt_budget.max_total_attempts as usize {
            if self
                .transport_policy
                .remaining_request_deadline(precommit_started)
                .is_none()
            {
                if let Some(probe) = health_probe.take() {
                    self.cancel_health_protection_probe(probe).await?;
                }
                return Err(precommit_timeout_failure());
            }
            let admission_input = AdmissionPlanningInput {
                execution_candidates: &snapshot.candidates,
                planning_snapshot: Some(&planning_snapshot),
                root_seed: &root_seed,
                exploration_budget: Some(&exploration_budget),
                #[cfg(test)]
                affinity_station_key_id: planning_snapshot
                    .runtime
                    .affinity_station_key_id
                    .as_deref(),
                profiles: &snapshot.profiles,
                capacity: &self.capacity,
                current_runtime_overlay_revision: self.routing_runtime.snapshot().runtime_revision,
                now_ms: controller_now_ms(request_started_at_ms, precommit_started),
                max_waiters_per_constraint: 0,
                #[cfg(test)]
                candidates: &snapshot.legacy_candidates,
            };
            let decision = match controller.next(admission_input) {
                Ok(decision) => decision,
                Err(failure) if last_failure.is_some() && catalog_planning_exhausted(&failure) => {
                    if last_failure
                        .as_ref()
                        .and_then(ProxyFailure::canonical)
                        .and_then(capacity_domain_from_failure)
                        .is_some()
                    {
                        record_trace_event(
                            &mut decision_trace,
                            DecisionTraceEventKind::SameDomainFallbackSuppressed,
                            "capacity_same_domain_fallback_suppressed",
                            attempt_index as u32,
                            None,
                        );
                    }
                    break;
                }
                Err(failure) => {
                    if let Some(probe) = health_probe.take() {
                        self.cancel_health_protection_probe(probe).await?;
                    }
                    return Err(controller_failure(failure, &execution_settings.policy));
                }
            };
            let selected = match decision {
                AdmissionDecision::Selected(selected) => selected,
                AdmissionDecision::Replan { .. } => {
                    if replan_count >= MAX_EXECUTION_REPLANS {
                        if let Some(probe) = health_probe.take() {
                            self.cancel_health_protection_probe(probe).await?;
                        }
                        return Err(controller_failure(
                            AdmissionFailure {
                                kind: AdmissionFailureKind::ConfigUnstable,
                                evidence: vec![AdmissionEvidence {
                                    code: "execution_replan_limit_exceeded",
                                    detail: "routing state changed repeatedly before admission"
                                        .to_string(),
                                }],
                            },
                            &execution_settings.policy,
                        ));
                    }
                    replan_count += 1;
                    let refreshed = await_request_deadline(
                        transport_policy.request_deadline,
                        precommit_started,
                        self.load_route_snapshots(
                            &request,
                            &execution_settings,
                            route_facts.clone(),
                            mapped_model.as_deref(),
                            planning_context,
                            // The first admission replan happens before an
                            // outbound attempt. Preserve the durable probe
                            // fence so the next immutable snapshot still
                            // admits exactly the reserved Half-Open scope.
                            health_probe.clone(),
                            HealthProbeAdmissionMode::Normal,
                        ),
                        |failure| failure,
                    )
                    .await;
                    match refreshed {
                        Ok(value) => (planning_snapshot, snapshot) = value,
                        Err(failure) => {
                            if let Some(probe) = health_probe.take() {
                                self.cancel_health_protection_probe(probe).await?;
                            }
                            return Err(failure);
                        }
                    }
                    continue;
                }
                other => match selected_route_or_failure(other) {
                    Ok(selected) => selected,
                    Err(failure) => {
                        if let Some(probe) = health_probe.take() {
                            self.cancel_health_protection_probe(probe).await?;
                        }
                        return Err(controller_failure(failure, &execution_settings.policy));
                    }
                },
            };
            attempted_count = attempted_count.max(attempt_index as i64 + 1);
            record_trace_event(
                &mut decision_trace,
                DecisionTraceEventKind::AttemptStart,
                "attempt_start",
                attempt_index as u32,
                Some(&profile_trace_detail),
            );
            let candidate = selected.candidate.clone();
            // A probe lease is scoped to one of the candidate's exact
            // commitments. The second planning pass may choose a healthy
            // candidate ahead of the probed one; never attach that lease to
            // a candidate that does not own the scope.
            let candidate_scopes =
                crate::application::error_rate_protection::candidate_health_scopes(
                    &candidate.station_id,
                    &candidate.station_key_id,
                    candidate.endpoint_revision,
                );
            if health_probe.as_ref().is_some_and(|probe| {
                candidate_scopes
                    .as_ref()
                    .is_none_or(|scopes| !scopes.contains(&probe.scope))
            }) {
                if let Some(probe) = health_probe.take() {
                    self.cancel_health_protection_probe(probe).await?;
                }
            }
            let candidate_model = candidate
                .resolved_upstream_model
                .as_deref()
                .or(mapped_model.as_deref());
            let is_capacity_cross_domain_fallback = selected.is_capacity_cross_domain_fallback;
            let attempt_started_at_ms = now_millis_for_services() as i64;
            let attempt_started = Instant::now();
            let Some(remaining) = self
                .transport_policy
                .remaining_request_deadline(precommit_started)
            else {
                if let Some(probe) = health_probe.take() {
                    self.cancel_health_protection_probe(probe).await?;
                }
                return Err(precommit_timeout_failure());
            };
            let target = tokio::time::timeout(
                remaining,
                self.resolve_selected_target(
                    selected,
                    &snapshot.targets,
                    &planning_snapshot,
                    &request,
                    candidate_model,
                ),
            )
            .await
            .unwrap_or_else(|_| Err(precommit_timeout_failure()));
            let original_commitment = target.as_ref().ok().map(|target| target.commitment.clone());
            // Selection alone is not a fallback: target validation must also
            // succeed before this route can enter the outbound attempt branch.
            if is_capacity_cross_domain_fallback && target.is_ok() {
                record_trace_event(
                    &mut decision_trace,
                    DecisionTraceEventKind::CrossDomainFallback,
                    "capacity_cross_domain_fallback",
                    attempt_index as u32,
                    None,
                );
            }
            let attempt_remaining = self
                .transport_policy
                .remaining_request_deadline(precommit_started);
            let attempt_probe = if target.is_ok() && attempt_remaining.is_some() {
                health_probe.take()
            } else {
                if let Some(probe) = health_probe.take() {
                    self.cancel_health_protection_probe(probe).await?;
                }
                None
            };
            let attempt_probe_scope = attempt_probe.as_ref().map(|probe| probe.scope.clone());
            let attempt_probe_state_revision =
                attempt_probe.as_ref().map(|probe| probe.state_revision);
            let mut attempt_result = match attempt_remaining {
                Some(remaining) => tokio::time::timeout(remaining, async {
                    let target = target?;
                    let prepared = self
                        .attempts
                        .attempt(&request, &target, candidate_model)
                        .await?;
                    let upstream_headers_ms = attempt_started.elapsed().as_millis() as i64;
                    let prepared = self
                        .bootstrap_stream(prepared, &request, &target, candidate_model)
                        .await?;
                    Ok((prepared, upstream_headers_ms))
                })
                .await
                .unwrap_or_else(|_| Err(precommit_timeout_failure())),
                None => Err(precommit_timeout_failure()),
            };
            let mut same_target_retry_ordinal = 0_u8;
            loop {
                match attempt_result {
                    Ok((prepared, upstream_headers_ms)) => {
                        controller
                            .record_actual_terminal_for_station_key(
                                candidate.routing_identity(),
                                ActualAttemptTerminal::Succeeded,
                            )
                            .map_err(|failure| {
                                controller_failure(failure, &execution_settings.policy)
                            })?;
                        let first_token_ms = precommit_started.elapsed().as_millis() as i64;
                        self.bind_success_affinity(
                            &request,
                            &execution_settings,
                            candidate_model,
                            &candidate,
                            &planning_snapshot.policy,
                            now_millis_for_services() as i64,
                        );
                        record_trace_event(
                            &mut decision_trace,
                            DecisionTraceEventKind::RequestTerminal,
                            "request_completed",
                            attempt_index as u32,
                            None,
                        );
                        finish_decision_trace(decision_trace, &self.routing_runtime);
                        return Ok(ProxyExecutionResponse::from_prepared(
                            prepared,
                            &candidate,
                            attempt_index as i64,
                            &request,
                            &execution_settings.policy,
                            AttemptTimings {
                                request_started_at_ms,
                                upstream_headers_ms,
                                first_token_ms,
                            },
                            attempt_probe_scope.clone(),
                            attempt_probe_state_revision,
                        ));
                    }
                    Err(mut failure) => {
                        attach_failure_candidate(&mut failure, &candidate);
                        let action = RetryActionPlanner::plan_with_context(
                            &failure,
                            idempotent,
                            false,
                            RetryActionContext {
                                attempt_ordinal: attempt_index as u16,
                                policy_revision: attempt_budget.policy_revision,
                                remaining_attempt_budget: attempt_budget
                                    .max_total_attempts
                                    .saturating_sub(attempt_index as u32 + 1),
                                remaining_precommit_budget_ms: self
                                    .transport_policy
                                    .remaining_request_deadline(precommit_started)
                                    .map(duration_millis_u64),
                                excluded_failure_domain_count: controller
                                    .excluded_failure_domain_count(),
                            },
                        );
                        let action_detail = retry_action_trace_detail(&action);
                        if failure.canonical().is_some() {
                            record_trace_event(
                                &mut decision_trace,
                                DecisionTraceEventKind::CanonicalFailure,
                                action.failure_code,
                                attempt_index as u32,
                                Some(&action_detail),
                            );
                        } else {
                            record_trace_event(
                                &mut decision_trace,
                                DecisionTraceEventKind::FailClosed,
                                "fail_closed_no_canonical",
                                attempt_index as u32,
                                Some(&action_detail),
                            );
                        }
                        self.finish_attempt(failed_attempt_record_with_probe(
                            &request.request_id,
                            attempt_index as u16,
                            &candidate,
                            &failure,
                            action,
                            attempt_started_at_ms,
                            false,
                            attempt_probe_scope.clone(),
                            attempt_probe_state_revision,
                        ))
                        .await?;
                        // Keep the canonical distinction visible at the
                        // execution boundary. Waiting is bounded by the same
                        // precommit budget as admission; a replan never
                        // recreates or resets the request-local budget.
                        if matches!(action.kind, RetryActionKind::WaitThenReplan) {
                            if let Some(delay_ms) = action.wait_delay_ms {
                                let Some(remaining) = self
                                    .transport_policy
                                    .remaining_request_deadline(precommit_started)
                                else {
                                    last_failure = Some(failure);
                                    break 'attempts;
                                };
                                if Duration::from_millis(delay_ms) >= remaining {
                                    last_failure = Some(failure);
                                    break 'attempts;
                                }
                                if delay_ms > 0 {
                                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                }
                            }
                        }
                        if matches!(
                            action.kind,
                            RetryActionKind::WaitThenReplan
                                | RetryActionKind::TryDifferentFailureDomain
                        ) {
                            // Preserve the failed candidate's full domain set
                            // before the next planning snapshot is loaded. A
                            // replan may add/remove candidates, but it cannot
                            // resurrect a domain ruled out by this request.
                            if let Some(domains) = planning_snapshot
                                .candidates
                                .iter()
                                .find(|raw| raw.station_key_id == candidate.station_key_id)
                                .map(|raw| raw.failure_domains.clone())
                            {
                                for domain in domains {
                                    controller.exclude_failure_domain(domain);
                                }
                            }
                        }
                        let capacity_domain =
                            failure.canonical().and_then(capacity_domain_from_failure);
                        let capacity_retry_registry =
                            self.routing_runtime.capacity_retry_registry();
                        if action.retries_same_target()
                            && u32::from(same_target_retry_ordinal)
                                < attempt_budget.max_same_target_capacity_retries
                            && attempt_index + 1 < attempt_budget.max_total_attempts as usize
                        {
                            let Some(domain) = capacity_domain.clone() else {
                                last_failure = Some(failure);
                                break 'attempts;
                            };
                            controller
                                .record_actual_terminal_for_station_key(
                                    candidate.routing_identity(),
                                    ActualAttemptTerminal::RetrySameTargetCapacity,
                                )
                                .map_err(|failure| {
                                    controller_failure(failure, &execution_settings.policy)
                                })?;
                            same_target_retry_ordinal += 1;
                            record_trace_event(
                                &mut decision_trace,
                                DecisionTraceEventKind::SameTargetRetry,
                                "capacity_same_target_retry",
                                attempt_index as u32,
                                Some(&format!("retry_{}", same_target_retry_ordinal)),
                            );
                            let delay_ms = action.wait_delay_ms.unwrap_or_else(|| {
                                capacity_retry_registry
                                    .deterministic_equal_jitter_ms(
                                        request.request_id.as_bytes(),
                                        same_target_retry_ordinal,
                                    )
                                    .unwrap_or_default()
                            });
                            let Some(remaining) = self
                                .transport_policy
                                .remaining_request_deadline(precommit_started)
                            else {
                                last_failure = Some(failure);
                                break;
                            };
                            let wait_budget_exhausted = capacity_sleep_ms.saturating_add(delay_ms)
                                > attempt_budget.capacity_retry_wait_budget_ms;
                            let deadline_suppressed = Duration::from_millis(delay_ms) >= remaining;
                            if wait_budget_exhausted || deadline_suppressed {
                                let (code, detail) = if wait_budget_exhausted {
                                    (
                                        "capacity_retry_wait_budget_exhausted",
                                        format!(
                                            "budget_{}_used_{}_delay_{}",
                                            attempt_budget.capacity_retry_wait_budget_ms,
                                            capacity_sleep_ms,
                                            delay_ms
                                        ),
                                    )
                                } else {
                                    (
                                        "capacity_retry_deadline_suppressed",
                                        format!(
                                            "remaining_{}_delay_{}",
                                            duration_millis_u64(remaining),
                                            delay_ms
                                        ),
                                    )
                                };
                                record_trace_event(
                                    &mut decision_trace,
                                    DecisionTraceEventKind::CanonicalFailure,
                                    code,
                                    attempt_index as u32,
                                    Some(&detail),
                                );
                                // Exhausting the wait budget suppresses only
                                // another wait on this capacity domain. Keep
                                // the failure flowing to the capacity-domain
                                // fallback below so a distinct domain can be
                                // tried in the same request. A fully consumed
                                // request deadline is handled by the `None`
                                // branch above and remains terminal.
                            } else {
                                // Queue admission happens before sleeping so waiters are
                                // bounded and cancellation releases their ticket by RAII.
                                // Sleeping must not consume an active retry permit.
                                let mut retry_waiter =
                                    match capacity_retry_registry.register_waiter(domain) {
                                        Ok(waiter) => waiter,
                                        Err(_) => {
                                            last_failure = Some(failure);
                                            break 'attempts;
                                        }
                                    };
                                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                                capacity_sleep_ms = capacity_sleep_ms.saturating_add(delay_ms);
                                let Some(remaining_after_wait) = self
                                    .transport_policy
                                    .remaining_request_deadline(precommit_started)
                                else {
                                    last_failure = Some(failure);
                                    break 'attempts;
                                };
                                let retry_admission = match retry_waiter.try_promote(
                                    controller_now_ms(request_started_at_ms, precommit_started),
                                ) {
                                    Ok(permit) => permit,
                                    Err(_) => {
                                        last_failure = Some(failure);
                                        break 'attempts;
                                    }
                                };
                                let Some(profile) =
                                    snapshot.profiles.get(&candidate.station_key_id)
                                else {
                                    drop(retry_admission);
                                    last_failure = Some(failure);
                                    break 'attempts;
                                };
                                let lease = match self
                                    .capacity
                                    .try_acquire(profile.capacity_request(&candidate))
                                {
                                    Ok(lease) => lease,
                                    Err(_) => {
                                        drop(retry_admission);
                                        last_failure = Some(failure);
                                        break 'attempts;
                                    }
                                };
                                let selected_retry = SelectedRoute {
                                    candidate: candidate.clone(),
                                    lease,
                                    retry_permit: None,
                                    evidence: vec![AdmissionEvidence {
                                        code: "capacity_same_target_retry",
                                        detail: same_target_retry_ordinal.to_string(),
                                    }],
                                    is_capacity_cross_domain_fallback: false,
                                };
                                let retry_started = Instant::now();
                                let retry_target = tokio::time::timeout(
                                    remaining_after_wait,
                                    async {
                                        let current_target = self
                                            .repository
                                            .load_current_execution_target(
                                                candidate.station_key_id.clone(),
                                            )
                                            .await
                                            .map_err(|error| {
                                                internal_failure(format!(
                                                    "reload retry execution target failed: {error}"
                                                ))
                                            })?;
                                        let Some(current_target) = current_target else {
                                            return Err(execution_target_failure(
                                                crate::application::operational_facts::target_resolver::ExecutionTargetError::TargetUnavailable {
                                                    station_key_id: candidate.station_key_id.clone(),
                                                    reason: "target_removed_before_retry",
                                                },
                                            ));
                                        };
                                        let current_targets = BTreeMap::from([(
                                            candidate.station_key_id.clone(),
                                            current_target,
                                        )]);
                                        self.resolve_selected_target(
                                            selected_retry,
                                            &current_targets,
                                            &planning_snapshot,
                                            &request,
                                            candidate_model,
                                        )
                                        .await
                                    },
                                )
                                .await
                                .unwrap_or_else(|_| Err(precommit_timeout_failure()));
                                attempt_result = match self
                                    .transport_policy
                                    .remaining_request_deadline(precommit_started)
                                {
                                    Some(remaining) => tokio::time::timeout(remaining, async {
                                        let target = retry_target?;
                                        let Some(original_commitment) = original_commitment.as_ref()
                                        else {
                                            return Err(internal_failure(
                                                "original execution commitment unavailable for retry",
                                            ));
                                        };
                                        ExecutionTargetResolver::revalidate_commitment(
                                            original_commitment,
                                            &target.commitment,
                                        )
                                        .map_err(execution_target_failure)?;
                                        let prepared = self
                                            .attempts
                                            .attempt(&request, &target, candidate_model)
                                            .await?;
                                        let headers_ms = retry_started.elapsed().as_millis() as i64;
                                        let prepared = self
                                            .bootstrap_stream(
                                                prepared,
                                                &request,
                                                &target,
                                                candidate_model,
                                            )
                                            .await?;
                                        Ok((prepared, headers_ms))
                                    })
                                    .await
                                    .unwrap_or_else(|_| Err(precommit_timeout_failure())),
                                    None => Err(precommit_timeout_failure()),
                                };
                                attempt_index += 1;
                                attempted_count = attempted_count.max(attempt_index as i64 + 1);
                                match &attempt_result {
                                    Ok(_) => retry_admission.complete_success(),
                                    Err(next)
                                        if matches!(
                                            next.canonical().map(|v| v.retry),
                                            Some(CanonicalRetryDisposition::RetrySameTarget)
                                        ) =>
                                    {
                                        retry_admission.complete_capacity_failure(
                                            controller_now_ms(
                                                request_started_at_ms,
                                                precommit_started,
                                            ),
                                        )
                                    }
                                    Err(_) => drop(retry_admission),
                                }
                                continue;
                            }
                        }
                        if let Some(domain) = capacity_domain {
                            capacity_retry_registry.record_capacity_exhausted(
                                domain.clone(),
                                controller_now_ms(request_started_at_ms, precommit_started),
                            );
                            controller
                                .record_actual_terminal_for_station_key(
                                    candidate.routing_identity(),
                                    ActualAttemptTerminal::RetrySameTargetCapacity,
                                )
                                .map_err(|failure| {
                                    controller_failure(failure, &execution_settings.policy)
                                })?;
                            if attempt_budget.allow_cross_capacity_domain_fallback
                                && controller.exclude_exhausted_capacity_domain(domain)
                            {
                                last_failure = Some(failure);
                                self.routing_runtime.mark_runtime_changed();
                                attempt_index += 1;
                                continue 'attempts;
                            }
                            record_trace_event(
                                &mut decision_trace,
                                DecisionTraceEventKind::SameDomainFallbackSuppressed,
                                "capacity_same_domain_fallback_suppressed",
                                attempt_index as u32,
                                None,
                            );
                            last_failure = Some(failure);
                            break 'attempts;
                        }
                        let terminal_result = controller.record_actual_terminal_for_station_key(
                            candidate.routing_identity(),
                            actual_terminal_for_action(&failure, action),
                        );
                        if action.allows_replan() {
                            terminal_result.map_err(|failure| {
                                controller_failure(failure, &execution_settings.policy)
                            })?;
                        }
                        last_failure = Some(failure);
                        if !action.allows_replan() {
                            break 'attempts;
                        }
                        self.routing_runtime.mark_runtime_changed();
                        attempt_index += 1;
                    }
                }
                break;
            }
        }

        if let Some(probe) = health_probe.take() {
            self.cancel_health_protection_probe(probe).await?;
        }
        let mut failure = last_failure.unwrap_or_else(|| {
            ProxyFailure::new(
                ProxyFailureCode::RouteNoCandidate,
                FailureSource::Routing,
                RetryClass::Never,
                StatusCode::BAD_GATEWAY,
                "all route candidates failed",
            )
        });
        failure.context_mut().attempt_count = Some(attempted_count);
        failure.context_mut().route_policy =
            Some(routing_policy_label(&execution_settings.policy).to_string());
        record_trace_event(
            &mut decision_trace,
            DecisionTraceEventKind::RequestTerminal,
            "request_failed",
            attempted_count.max(0) as u32,
            None,
        );
        finish_decision_trace(decision_trace, &self.routing_runtime);
        Err(failure)
    }

    async fn resolve_selected_target(
        &self,
        selected: SelectedRoute,
        targets: &BTreeMap<String, ExecutionTargetRef>,
        planning_snapshot: &crate::application::routing_engine::planning_snapshot::PlanningSnapshot,
        request: &CanonicalProxyRequest,
        mapped_model: Option<&str>,
    ) -> Result<ExecutionTargetHandle, ProxyFailure> {
        let station_key_id = selected.candidate.station_key_id.clone();
        let Some(current) = targets.get(&station_key_id).cloned() else {
            return Err(ProxyFailure::new(
                ProxyFailureCode::RouteFactsUnavailable,
                FailureSource::Routing,
                RetryClass::BeforeOutput,
                StatusCode::SERVICE_UNAVAILABLE,
                "selected route target unavailable",
            ));
        };
        let expected_secret_ref_id = current
            .api_key_secret_ref
            .as_ref()
            .map(|secret_ref| secret_ref.id.clone())
            .unwrap_or_default();
        let candidate_commitment = planning_snapshot
            .candidates
            .iter()
            .find(|candidate| candidate.station_key_id == station_key_id)
            .ok_or_else(|| {
                ProxyFailure::new(
                    ProxyFailureCode::RouteFactsUnavailable,
                    FailureSource::Routing,
                    RetryClass::BeforeOutput,
                    StatusCode::SERVICE_UNAVAILABLE,
                    "selected route commitment unavailable",
                )
            })?;
        ExecutionTargetResolver::resolve(
            LeasedSelectedTarget {
                station_key_id,
                expected_endpoint_revision: selected.candidate.endpoint_revision,
                expected_secret_ref_id,
                expected_credential_revision: candidate_commitment.credential_revision,
                expected_account_revision: candidate_commitment.account_revision,
                expected_group_binding_id: candidate_commitment.group_binding_id.clone(),
                expected_group_revision: candidate_commitment.group_revision,
                resolved_upstream_model: mapped_model
                    .map(ToString::to_string)
                    .or_else(|| candidate_commitment.resolved_upstream_model.clone()),
                model_alias_revision: candidate_commitment.model_alias_revision,
                expected_capacity_domain: candidate_commitment.capacity_domain.clone(),
                expected_capacity_domain_revision: candidate_commitment.capacity_domain_revision,
                policy_revision: planning_snapshot.routing_policy_revision,
                request_body_identity: RequestBodyIdentity::from_bytes(&request.body),
                protocol_profile: TargetProtocolProfile {
                    upstream_api_format: current.upstream_api_format.clone(),
                    stream: request.stream,
                    uses_tools: request.requirements.uses_tools,
                    uses_vision: request.requirements.uses_vision,
                    uses_reasoning: request.requirements.uses_reasoning,
                },
                lease: selected.lease,
                retry_permit: selected.retry_permit,
            },
            current,
            self.credentials.as_ref(),
        )
        .await
        .map_err(execution_target_failure)
    }

    async fn bootstrap_stream(
        &self,
        prepared: PreparedAttempt,
        request: &CanonicalProxyRequest,
        target: &ExecutionTargetHandle,
        mapped_model: Option<&str>,
    ) -> Result<PreparedAttempt, ProxyFailure> {
        let PreparedAttempt::Stream {
            status,
            headers,
            mut chunks,
            completion_policy,
            diagnostic_memory: _,
        } = prepared
        else {
            return Ok(prepared);
        };

        let Some(mut machine) = SseBootstrapMachine::for_completion_policy_with_diagnostic_memory(
            completion_policy,
            self.routing_runtime.diagnostic_memory_budget(),
        )
        .map_err(protocol_bootstrap_failure)?
        else {
            return Ok(PreparedAttempt::Stream {
                status,
                headers,
                chunks,
                completion_policy,
                diagnostic_memory: None,
            });
        };
        loop {
            match tokio::time::timeout(request.transport_policy().first_byte_timeout, chunks.next())
                .await
            {
                Ok(Some(Ok(bytes))) if bytes.is_empty() => continue,
                Ok(Some(Ok(bytes))) => {
                    match machine
                        .observe_chunk(&bytes)
                        .map_err(protocol_bootstrap_failure)?
                    {
                        BootstrapDisposition::Pending => continue,
                        BootstrapDisposition::PrecommitTerminal { terminal, event } => {
                            return Err(precommit_protocol_terminal_failure(
                                terminal,
                                &event,
                                completion_policy,
                                &headers,
                                request,
                                target,
                                mapped_model,
                            ));
                        }
                        BootstrapDisposition::Emit { events, .. } => {
                            let diagnostic_memory = machine.take_diagnostic_memory_permit();
                            let prefix = stream::iter(events.into_iter().map(Ok));
                            return Ok(PreparedAttempt::Stream {
                                status,
                                headers,
                                chunks: prefix.chain(chunks).boxed(),
                                completion_policy,
                                diagnostic_memory,
                            });
                        }
                    }
                }
                Ok(Some(Err(failure))) => return Err(precommit_stream_failure(failure, target)),
                Ok(None) => match machine.finish_eof().map_err(protocol_bootstrap_failure)? {
                    BootstrapDisposition::Emit { events, .. } => {
                        let diagnostic_memory = machine.take_diagnostic_memory_permit();
                        return Ok(PreparedAttempt::Stream {
                            status,
                            headers,
                            chunks: stream::iter(events.into_iter().map(Ok)).boxed(),
                            completion_policy,
                            diagnostic_memory,
                        });
                    }
                    BootstrapDisposition::PrecommitTerminal { terminal, event } => {
                        return Err(precommit_protocol_terminal_failure(
                            terminal,
                            &event,
                            completion_policy,
                            &headers,
                            request,
                            target,
                            mapped_model,
                        ));
                    }
                    BootstrapDisposition::Pending => {
                        return Err(precommit_stream_ended_failure(target))
                    }
                },
                Err(_) => return Err(upstream_first_byte_timeout_failure(target)),
            }
        }
    }

    async fn execute_usage(
        &self,
        request: CanonicalProxyRequest,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let snapshots = self
            .repository
            .load_balance_snapshots()
            .await
            .map_err(|error| internal_failure(format!("load balance snapshots failed: {error}")))?;
        Ok(ProxyExecutionResponse::local_buffered(
            StatusCode::OK,
            json_headers(),
            local_usage_body(snapshots)?,
            &request,
            "local_usage_success",
        ))
    }

    async fn execute_models(
        &self,
        request: CanonicalProxyRequest,
        route_facts: RouteRequestFacts,
        mut snapshot: OperationalRouteSnapshot,
        mut planning_snapshot: crate::application::routing_engine::planning_snapshot::PlanningSnapshot,
        execution_settings: &RoutingExecutionSettings,
        mapped_model: Option<String>,
        request_started_at_ms: i64,
        precommit_started: Instant,
        planning_context: PlanningRequestContext,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let transport_policy = Arc::clone(request.transport_policy());
        let mut seen_ids = HashSet::new();
        let mut models = Vec::new();
        let mut attempted_count = 0_i64;
        let mut failed_count = 0_i64;
        let mut last_failure = None;
        let mut headers = HeaderMap::new();
        let attempt_budget = planning_snapshot.attempt_budget;
        let mut controller = RouteAdmissionCoordinator::new_with_retry_budget(
            route_facts.clone(),
            AdmissionSettings {
                deadline_ms: precommit_deadline_ms(
                    request_started_at_ms,
                    transport_policy.request_deadline,
                ),
                initial_snapshot_id: planning_snapshot.snapshot_id.clone(),
                initial_runtime_overlay_revision: planning_snapshot.runtime.runtime_revision,
                initial_durable_generation: planning_snapshot.durable_revision,
                fallback_policy: FallbackPolicy {
                    has_stable_idempotency_key: true,
                    non_idempotent: false,
                },
                attempt_budget,
            },
            self.routing_runtime.retry_budget(),
        );
        let root_seed = self.routing_runtime.root_seed();
        let exploration_budget = self.routing_runtime.exploration_budget();

        let mut attempt_index = 0_usize;
        let mut replan_count = 0_usize;
        while attempt_index < attempt_budget.max_total_attempts as usize {
            if self
                .transport_policy
                .remaining_request_deadline(precommit_started)
                .is_none()
            {
                return Err(precommit_timeout_failure());
            }
            let admission_input = AdmissionPlanningInput {
                execution_candidates: &snapshot.candidates,
                planning_snapshot: Some(&planning_snapshot),
                root_seed: &root_seed,
                exploration_budget: Some(&exploration_budget),
                #[cfg(test)]
                affinity_station_key_id: None,
                profiles: &snapshot.profiles,
                capacity: &self.capacity,
                current_runtime_overlay_revision: self.routing_runtime.snapshot().runtime_revision,
                now_ms: controller_now_ms(request_started_at_ms, precommit_started),
                max_waiters_per_constraint: 0,
                #[cfg(test)]
                candidates: &snapshot.legacy_candidates,
            };
            let decision = match controller.next(admission_input) {
                Ok(decision) => decision,
                Err(failure) if attempted_count > 0 && catalog_planning_exhausted(&failure) => {
                    break;
                }
                Err(failure) => {
                    return Err(controller_failure(
                        failure,
                        &RoutingPolicy::PriorityFallback,
                    ));
                }
            };
            let selected = match decision {
                AdmissionDecision::Selected(selected) => selected,
                AdmissionDecision::Replan { .. } => {
                    if replan_count >= MAX_EXECUTION_REPLANS {
                        return Err(controller_failure(
                            AdmissionFailure {
                                kind: AdmissionFailureKind::ConfigUnstable,
                                evidence: vec![AdmissionEvidence {
                                    code: "execution_replan_limit_exceeded",
                                    detail: "routing state changed repeatedly before admission"
                                        .to_string(),
                                }],
                            },
                            &RoutingPolicy::PriorityFallback,
                        ));
                    }
                    replan_count += 1;
                    (planning_snapshot, snapshot) = await_request_deadline(
                        transport_policy.request_deadline,
                        precommit_started,
                        self.load_route_snapshots(
                            &request,
                            execution_settings,
                            route_facts.clone(),
                            mapped_model.as_deref(),
                            planning_context,
                            None,
                            HealthProbeAdmissionMode::Normal,
                        ),
                        |failure| failure,
                    )
                    .await?;
                    continue;
                }
                other => match selected_route_or_failure(other) {
                    Ok(selected) => selected,
                    Err(failure) if attempted_count > 0 && catalog_planning_exhausted(&failure) => {
                        break;
                    }
                    Err(failure) => {
                        return Err(controller_failure(
                            failure,
                            &RoutingPolicy::PriorityFallback,
                        ));
                    }
                },
            };
            let candidate = selected.candidate.clone();
            attempted_count = attempted_count.max(attempt_index as i64 + 1);
            let attempt_started_at_ms = now_millis_for_services() as i64;
            let Some(remaining) = self
                .transport_policy
                .remaining_request_deadline(precommit_started)
            else {
                return Err(precommit_timeout_failure());
            };
            let target = match tokio::time::timeout(
                remaining,
                self.resolve_selected_target(
                    selected,
                    &snapshot.targets,
                    &planning_snapshot,
                    &request,
                    mapped_model.as_deref(),
                ),
            )
            .await
            {
                Err(_) => return Err(precommit_timeout_failure()),
                Ok(Ok(target)) => target,
                Ok(Err(mut failure)) => {
                    attach_failure_candidate(&mut failure, &candidate);
                    failed_count += 1;
                    last_failure = Some(failure);
                    controller
                        .record_actual_terminal_for_station_key(
                            candidate.routing_identity(),
                            ActualAttemptTerminal::FailedBeforeCommit,
                        )
                        .map_err(|failure| {
                            controller_failure(failure, &RoutingPolicy::PriorityFallback)
                        })?;
                    self.routing_runtime.mark_runtime_changed();
                    attempt_index += 1;
                    continue;
                }
            };
            let Some(remaining) = self
                .transport_policy
                .remaining_request_deadline(precommit_started)
            else {
                return Err(precommit_timeout_failure());
            };
            match tokio::time::timeout(
                remaining,
                self.attempts
                    .attempt(&request, &target, mapped_model.as_deref()),
            )
            .await
            {
                Err(_) => return Err(precommit_timeout_failure()),
                Ok(Ok(prepared)) => {
                    let (_, attempt_headers, body) = prepared.into_parts();
                    headers = attempt_headers;
                    match body {
                        ProxyExecutionBody::Buffered(body) => match extract_models(&body) {
                            Ok(items) => {
                                for item in items {
                                    let Some(id) = item.get("id").and_then(Value::as_str) else {
                                        continue;
                                    };
                                    if seen_ids.insert(id.to_string()) {
                                        models.push(item);
                                    }
                                }
                                self.finish_attempt(success_attempt_record(
                                    &request.request_id,
                                    attempt_index as u16,
                                    &candidate,
                                    attempt_started_at_ms,
                                    true,
                                    None,
                                ))
                                .await?;
                                controller
                                    .record_actual_terminal_for_station_key(
                                        candidate.routing_identity(),
                                        ActualAttemptTerminal::Succeeded,
                                    )
                                    .map_err(|failure| {
                                        controller_failure(
                                            failure,
                                            &RoutingPolicy::PriorityFallback,
                                        )
                                    })?;
                            }
                            Err(error) => {
                                let failure = internal_failure(error.clone());
                                let action = RetryAction::stop(
                                    &failure,
                                    RetryActionContext {
                                        attempt_ordinal: attempt_index as u16,
                                        policy_revision: attempt_budget.policy_revision,
                                        remaining_attempt_budget: attempt_budget
                                            .max_total_attempts
                                            .saturating_sub(attempt_index as u32 + 1),
                                        remaining_precommit_budget_ms: self
                                            .transport_policy
                                            .remaining_request_deadline(precommit_started)
                                            .map(duration_millis_u64),
                                        excluded_failure_domain_count: 0,
                                    },
                                    ReplayGateResult::Rejected {
                                        reason_key: "models_response_parse_failed",
                                    },
                                    "models_response_parse_failed",
                                    "routing.retry.modelsResponseParseFailed",
                                );
                                self.finish_attempt(failed_attempt_record(
                                    &request.request_id,
                                    attempt_index as u16,
                                    &candidate,
                                    &failure,
                                    action,
                                    attempt_started_at_ms,
                                    true,
                                    None,
                                ))
                                .await?;
                                failed_count += 1;
                                last_failure = Some(failure);
                                controller
                                    .record_actual_terminal_for_station_key(
                                        candidate.routing_identity(),
                                        ActualAttemptTerminal::FailedBeforeCommit,
                                    )
                                    .map_err(|failure| {
                                        controller_failure(
                                            failure,
                                            &RoutingPolicy::PriorityFallback,
                                        )
                                    })?;
                                if !ModelsRetryAdapter::allows_next_candidate(&action) {
                                    break;
                                }
                            }
                        },
                        ProxyExecutionBody::Stream { .. } => {
                            let failure =
                                internal_failure("model list upstream returned a stream response");
                            let action = RetryAction::stop(
                                &failure,
                                RetryActionContext {
                                    attempt_ordinal: attempt_index as u16,
                                    policy_revision: attempt_budget.policy_revision,
                                    remaining_attempt_budget: attempt_budget
                                        .max_total_attempts
                                        .saturating_sub(attempt_index as u32 + 1),
                                    remaining_precommit_budget_ms: self
                                        .transport_policy
                                        .remaining_request_deadline(precommit_started)
                                        .map(duration_millis_u64),
                                    excluded_failure_domain_count: 0,
                                },
                                ReplayGateResult::Rejected {
                                    reason_key: "models_stream_response",
                                },
                                "models_stream_response",
                                "routing.retry.modelsStreamResponse",
                            );
                            self.finish_attempt(failed_attempt_record(
                                &request.request_id,
                                attempt_index as u16,
                                &candidate,
                                &failure,
                                action,
                                attempt_started_at_ms,
                                false,
                                None,
                            ))
                            .await?;
                            failed_count += 1;
                            last_failure = Some(failure);
                            controller
                                .record_actual_terminal_for_station_key(
                                    candidate.routing_identity(),
                                    ActualAttemptTerminal::FailedBeforeCommit,
                                )
                                .map_err(|failure| {
                                    controller_failure(failure, &RoutingPolicy::PriorityFallback)
                                })?;
                            if !ModelsRetryAdapter::allows_next_candidate(&action) {
                                break;
                            }
                        }
                    }
                }
                Ok(Err(mut failure)) => {
                    attach_failure_candidate(&mut failure, &candidate);
                    let action = RetryActionPlanner::plan_with_context(
                        &failure,
                        true,
                        false,
                        RetryActionContext {
                            attempt_ordinal: attempt_index as u16,
                            policy_revision: attempt_budget.policy_revision,
                            remaining_attempt_budget: attempt_budget
                                .max_total_attempts
                                .saturating_sub(attempt_index as u32 + 1),
                            remaining_precommit_budget_ms: self
                                .transport_policy
                                .remaining_request_deadline(precommit_started)
                                .map(duration_millis_u64),
                            excluded_failure_domain_count: controller
                                .excluded_failure_domain_count(),
                        },
                    );
                    self.finish_attempt(failed_attempt_record(
                        &request.request_id,
                        attempt_index as u16,
                        &candidate,
                        &failure,
                        action,
                        attempt_started_at_ms,
                        false,
                        None,
                    ))
                    .await?;
                    failed_count += 1;
                    last_failure = Some(failure);
                    controller
                        .record_actual_terminal_for_station_key(
                            candidate.routing_identity(),
                            ActualAttemptTerminal::FailedBeforeCommit,
                        )
                        .map_err(|failure| {
                            controller_failure(failure, &RoutingPolicy::PriorityFallback)
                        })?;
                    if !ModelsRetryAdapter::allows_next_candidate(&action) {
                        break;
                    }
                }
            }
            self.routing_runtime.mark_runtime_changed();
            attempt_index += 1;
        }

        if models.is_empty() {
            return Err(
                last_failure.unwrap_or_else(|| internal_failure("all model upstreams failed"))
            );
        }

        let body = serde_json::to_vec(&serde_json::json!({
            "object": "list",
            "data": models,
        }))
        .map(Bytes::from)
        .map_err(|error| internal_failure(format!("serialize models response failed: {error}")))?;
        if headers.is_empty() {
            headers = json_headers();
        }

        Ok(ProxyExecutionResponse::local_buffered_with_fallback(
            StatusCode::OK,
            headers,
            body,
            &request,
            "models_aggregated_success",
            failed_count,
            attempted_count,
        ))
    }

    async fn load_route_snapshots(
        &self,
        request: &CanonicalProxyRequest,
        settings: &RoutingExecutionSettings,
        route_facts: RouteRequestFacts,
        model: Option<&str>,
        planning_context: PlanningRequestContext,
        health_probe: Option<crate::application::health_protection::HealthProtectionProbe>,
        health_probe_mode: HealthProbeAdmissionMode,
    ) -> Result<
        (
            crate::application::routing_engine::planning_snapshot::PlanningSnapshot,
            OperationalRouteSnapshot,
        ),
        ProxyFailure,
    > {
        let load_planning_snapshot = |runtime| {
            self.repository.load_planning_snapshot_with_probe(
                route_facts.clone(),
                runtime,
                planning_context,
                health_probe.clone(),
                health_probe_mode,
            )
        };
        let mut planning_snapshot = load_planning_snapshot(self.routing_runtime.snapshot())
            .await
            .map_err(|error| planning_snapshot_repository_failure(error, false))?
            .ok_or_else(routing_configuration_required_failure)?;

        if let Some(station_key_id) = self.affinity_station_key_id(
            request,
            settings,
            model,
            &planning_snapshot,
            now_millis_for_services() as i64,
        ) {
            let mut runtime_overlay = self.routing_runtime.snapshot();
            runtime_overlay.affinity_station_key_id = Some(station_key_id);
            planning_snapshot = load_planning_snapshot(runtime_overlay)
                .await
                .map_err(|error| planning_snapshot_repository_failure(error, true))?
                .ok_or_else(routing_configuration_required_failure)?;
        }

        let snapshot = self
            .repository
            .load_operational_route_snapshot(route_facts, planning_snapshot.clone())
            .await
            .map_err(|error| {
                internal_failure(format!("load execution route index failed: {error}"))
            })?;
        Ok((planning_snapshot, snapshot))
    }

    async fn acquire_health_probe(
        &self,
        planning_snapshot: &crate::application::routing_engine::planning_snapshot::PlanningSnapshot,
    ) -> Result<HealthProbeAcquireOutcome, ProxyFailure> {
        if !planning_snapshot.policy.protection_profile.enabled {
            return Ok(HealthProbeAcquireOutcome::None);
        }
        let statuses = self
            .repository
            .load_health_protection_statuses(now_millis_for_services().max(0) as i64)
            .await
            .map_err(|error| {
                internal_failure(format!("load health protection statuses failed: {error}"))
            })?;
        let mut eligible_scope_seen = false;
        for candidate in &planning_snapshot.candidates {
            let Some(scopes) = crate::application::error_rate_protection::candidate_health_scopes(
                &candidate.station_id,
                &candidate.station_key_id,
                candidate.endpoint_revision,
            ) else {
                continue;
            };
            let now_ms = now_millis_for_services().min(i64::MAX as u128) as i64;
            for scope in scopes.iter() {
                let eligible = statuses.iter().any(|status| {
                    status.scope == *scope && match status.state {
                        crate::application::health_protection::HealthProtectionState::Open => {
                            status
                                .cooldown_until_ms
                                .is_some_and(|until| until <= now_ms)
                        }
                        crate::application::health_protection::HealthProtectionState::HalfOpen => {
                            !status.half_open_probe_in_flight
                        }
                        crate::application::health_protection::HealthProtectionState::Closed => {
                            false
                        }
                    }
                });
                if !eligible {
                    continue;
                }
                eligible_scope_seen = true;
                if let Some(probe) = self
                    .repository
                    .begin_health_protection_probe(scope.clone(), now_ms)
                    .await
                    .map_err(|error| {
                        internal_failure(format!("begin health protection probe failed: {error}"))
                    })?
                {
                    return Ok(HealthProbeAcquireOutcome::Lease(probe));
                }
            }
        }
        Ok(if eligible_scope_seen {
            HealthProbeAcquireOutcome::LeaseRace
        } else {
            HealthProbeAcquireOutcome::None
        })
    }

    fn affinity_station_key_id(
        &self,
        request: &CanonicalProxyRequest,
        settings: &RoutingExecutionSettings,
        model: Option<&str>,
        planning_snapshot: &crate::application::routing_engine::planning_snapshot::PlanningSnapshot,
        now_ms: i64,
    ) -> Option<String> {
        if !planning_snapshot.policy.affinity_enabled {
            return None;
        }
        let mut registry = self.affinity.lock().ok()?;
        for candidate in &planning_snapshot.candidates {
            for lookup in affinity_lookups(request, settings, model, candidate.endpoint_revision) {
                if let Ok(hit) = registry.lookup(&lookup, now_ms) {
                    if hit.station_key_id == candidate.station_key_id {
                        return Some(hit.station_key_id);
                    }
                }
            }
        }
        None
    }

    fn bind_success_affinity(
        &self,
        request: &CanonicalProxyRequest,
        settings: &RoutingExecutionSettings,
        model: Option<&str>,
        candidate: &RoutePlanCandidate,
        policy: &crate::models::routing_policy::RoutingPolicyConfigV2,
        now_ms: i64,
    ) {
        if !policy.affinity_enabled {
            return;
        }
        let Ok(mut registry) = self.affinity.lock() else {
            return;
        };
        let ttl_ms = i64::from(policy.affinity_ttl_seconds).saturating_mul(1_000);
        for lookup in affinity_lookups(request, settings, model, candidate.endpoint_revision) {
            let _ = registry.bind(lookup, &candidate.station_key_id, now_ms, ttl_ms);
        }
    }

    async fn finish_attempt(&self, record: AttemptTerminalRecord) -> Result<(), ProxyFailure> {
        let Some(writer) = self.lifecycle_writer.as_ref() else {
            return Ok(());
        };
        let reservation = writer
            .try_reserve_attempt()
            .map_err(attempt_lifecycle_admission_failure)?;
        reservation
            .send(record)
            .await
            .map_err(|_| attempt_lifecycle_unavailable_failure("attempt-terminal ack dropped"))?
            .map_err(attempt_lifecycle_write_failure)?;
        Ok(())
    }

    async fn cancel_health_protection_probe(
        &self,
        probe: crate::application::health_protection::HealthProtectionProbe,
    ) -> Result<(), ProxyFailure> {
        self.repository
            .cancel_health_protection_probe(probe, now_millis_for_services() as i64)
            .await
            .map(|_| ())
            .map_err(|error| {
                internal_failure(format!("cancel health protection probe failed: {error}"))
            })
    }
}

fn planning_snapshot_repository_failure(
    error: RoutingExecutionReadError,
    affinity_reload: bool,
) -> ProxyFailure {
    if matches!(error, RoutingExecutionReadError::DeadlineExceeded) {
        return precommit_timeout_failure();
    }
    if affinity_reload {
        internal_failure(format!("reload affinity planning snapshot failed: {error}"))
    } else {
        internal_failure(format!(
            "load intelligent planning snapshot failed: {error}"
        ))
    }
}

fn success_attempt_record(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    started_at_ms: i64,
    output_committed: bool,
    probe_state_revision: Option<u64>,
) -> AttemptTerminalRecord {
    success_attempt_record_with_probe(
        request_id,
        ordinal,
        candidate,
        started_at_ms,
        output_committed,
        None,
        probe_state_revision,
    )
}

fn success_attempt_record_with_probe(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    started_at_ms: i64,
    output_committed: bool,
    probe_scope: Option<crate::application::health_protection::HealthProtectionScope>,
    probe_state_revision: Option<u64>,
) -> AttemptTerminalRecord {
    AttemptTerminalRecord {
        context: attempt_context(
            request_id,
            ordinal,
            candidate,
            started_at_ms,
            probe_scope.clone(),
            probe_state_revision,
        ),
        terminal: AttemptTerminal::Succeeded,
        output_committed,
        terminal_at_ms: now_millis_for_services() as i64,
        probe_scope,
        probe_state_revision,
    }
}

fn failed_attempt_record(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    failure: &ProxyFailure,
    action: RetryAction,
    started_at_ms: i64,
    output_committed: bool,
    probe_state_revision: Option<u64>,
) -> AttemptTerminalRecord {
    failed_attempt_record_with_probe(
        request_id,
        ordinal,
        candidate,
        failure,
        action,
        started_at_ms,
        output_committed,
        None,
        probe_state_revision,
    )
}

fn failed_attempt_record_with_probe(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    failure: &ProxyFailure,
    action: RetryAction,
    started_at_ms: i64,
    output_committed: bool,
    probe_scope: Option<crate::application::health_protection::HealthProtectionScope>,
    probe_state_revision: Option<u64>,
) -> AttemptTerminalRecord {
    AttemptTerminalRecord {
        context: attempt_context(
            request_id,
            ordinal,
            candidate,
            started_at_ms,
            probe_scope.clone(),
            probe_state_revision,
        ),
        terminal: AttemptTerminal::Failed(classified_attempt_failure(failure, action)),
        output_committed,
        terminal_at_ms: now_millis_for_services() as i64,
        probe_scope,
        probe_state_revision,
    }
}

fn attempt_context(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    started_at_ms: i64,
    probe_scope: Option<crate::application::health_protection::HealthProtectionScope>,
    probe_state_revision: Option<u64>,
) -> AttemptContext {
    AttemptContext {
        attempt_id: AttemptId::new(request_id, ordinal),
        station_id: candidate.station_id.clone(),
        station_key_id: candidate.station_key_id.clone(),
        endpoint_revision: candidate.endpoint_revision,
        credential_revision: candidate.credential_revision,
        account_revision: candidate.account_revision,
        group_binding_id: candidate.group_binding_id.clone(),
        group_revision: candidate.group_revision,
        resolved_upstream_model: candidate.resolved_upstream_model.clone(),
        model_alias_revision: candidate.model_alias_revision,
        started_at_ms,
        probe_scope,
        probe_state_revision,
    }
}

fn classified_attempt_failure(
    failure: &ProxyFailure,
    action: RetryAction,
) -> ClassifiedAttemptFailure {
    if let Some(canonical) = failure.canonical() {
        // Canonical classification owns health/capability effects, while the
        // execution action owns replay authorization.  Keep the lifecycle
        // record aligned with the action so a replay-gate stop is never
        // reported as a candidate retry.
        let mut classified = classified_attempt_failure_from_canonical(canonical);
        classified.retry = action.lifecycle_retry();
        return classified;
    }
    ClassifiedAttemptFailure {
        kind: AttemptFailureKind::LocalAdapter,
        blame: failure_blame(failure.source),
        retry: action.lifecycle_retry(),
        health: HealthEffect::Neutral,
        public_code: failure.code.as_str().to_string(),
        sanitized_detail: Some(crate::services::secrets::mask::redact_text(
            &failure.public_message,
        )),
    }
}

fn failure_blame(source: FailureSource) -> FailureBlame {
    match source {
        FailureSource::Local => FailureBlame::LocalAdapter,
        FailureSource::Routing => FailureBlame::LocalAdapter,
        FailureSource::Upstream => FailureBlame::Upstream,
        FailureSource::Downstream => FailureBlame::Downstream,
        FailureSource::Internal => FailureBlame::LocalAdapter,
    }
}

fn attempt_lifecycle_admission_failure(error: WriterAdmissionError) -> ProxyFailure {
    let mut failure =
        attempt_lifecycle_unavailable_failure("local proxy lifecycle writer unavailable");
    failure.internal_detail = Some(format!(
        "attempt lifecycle writer admission rejected: {error:?}"
    ));
    failure
}

fn attempt_lifecycle_write_failure(error: LifecycleWriteError) -> ProxyFailure {
    let mut failure =
        attempt_lifecycle_unavailable_failure("local proxy lifecycle persistence unavailable");
    failure.internal_detail = Some(format!("attempt lifecycle write failed: {error:?}"));
    failure
}

fn attempt_lifecycle_unavailable_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::LocalProxyBusy,
        FailureSource::Local,
        RetryClass::Never,
        StatusCode::SERVICE_UNAVAILABLE,
        message,
    )
}

impl PreparedAttempt {
    fn into_parts(self) -> (StatusCode, HeaderMap, ProxyExecutionBody) {
        match self {
            Self::Buffered {
                status,
                headers,
                body,
            } => (status, headers, ProxyExecutionBody::Buffered(body)),
            Self::Stream {
                status,
                headers,
                chunks,
                diagnostic_memory,
                ..
            } => (
                status,
                headers,
                ProxyExecutionBody::Stream {
                    chunks,
                    diagnostic_memory,
                },
            ),
        }
    }
}

impl ProxyExecutionResponse {
    fn from_prepared(
        prepared: PreparedAttempt,
        candidate: &RoutePlanCandidate,
        fallback_count: i64,
        request: &CanonicalProxyRequest,
        routing_policy: &RoutingPolicy,
        timings: AttemptTimings,
        probe_scope: Option<crate::application::health_protection::HealthProtectionScope>,
        probe_state_revision: Option<u64>,
    ) -> Self {
        let (status, headers, body) = prepared.into_parts();
        let body_bytes = match &body {
            ProxyExecutionBody::Buffered(body) => Some(body.len() as i64),
            ProxyExecutionBody::Stream { .. } => None,
        };
        let selected_attempt = AttemptContext {
            attempt_id: AttemptId::new(request.request_id.clone(), fallback_count as u16),
            station_id: candidate.station_id.clone(),
            station_key_id: candidate.station_key_id.clone(),
            endpoint_revision: candidate.endpoint_revision,
            credential_revision: candidate.credential_revision,
            account_revision: candidate.account_revision,
            group_binding_id: candidate.group_binding_id.clone(),
            group_revision: candidate.group_revision,
            resolved_upstream_model: candidate.resolved_upstream_model.clone(),
            model_alias_revision: candidate.model_alias_revision,
            started_at_ms: timings.request_started_at_ms,
            probe_scope,
            probe_state_revision,
        };
        Self {
            status,
            headers,
            body,
            selected_station_key_id: Some(candidate.station_key_id.clone()),
            selected_station_id: Some(candidate.station_id.clone()),
            fallback_count,
            lifecycle: ExecutionLifecycleEvidence {
                annotations: RequestLogAnnotations {
                    model: request.model.clone(),
                    stream: request.stream,
                    http_status: None,
                    selected_station_key_id: Some(candidate.station_key_id.clone()),
                    selected_station_id: Some(candidate.station_id.clone()),
                    upstream_base_url: None,
                    route_policy: Some(routing_policy_label(routing_policy).to_string()),
                    route_reason: Some(format!(
                        "selected {} for {}",
                        candidate.station_key_id,
                        endpoint_path(&request.endpoint)
                    )),
                    rejected_candidates_json: Some("[]".to_string()),
                    body_bytes,
                    route_wait_ms: Some(0),
                    upstream_headers_ms: Some(timings.upstream_headers_ms.max(0)),
                    failure_source: None,
                    attempts_json: None,
                    completion_source: Some("upstream".to_string()),
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                    cache_creation_tokens: None,
                    cache_read_tokens: None,
                    reasoning_effort: request.reasoning_effort.clone(),
                    first_token_ms: Some(timings.first_token_ms.max(0)),
                    billing_mode: billing_mode_for_pricing(&candidate.pricing),
                },
                selected_attempt: Some(selected_attempt),
                selected_attempt_cost: Some(super::attempt::SelectedAttemptCostSnapshot {
                    ordinal: fallback_count as u16,
                    pricing_basis: pricing_basis_label(candidate.pricing.basis).to_string(),
                    pricing_status_label: candidate.pricing.status_label.clone(),
                    currency: candidate.pricing.currency.clone(),
                    unit: candidate.pricing.unit.clone(),
                    estimated_input_price: candidate.pricing.estimated_input_price,
                    estimated_output_price: candidate.pricing.estimated_output_price,
                    estimated_cache_creation_price: candidate
                        .pricing
                        .estimated_cache_creation_price,
                    estimated_cache_read_price: candidate.pricing.estimated_cache_read_price,
                }),
                attempt_count: (fallback_count + 1).max(1) as u16,
                fallback_count: fallback_count.max(0) as u16,
            },
        }
    }

    // Test assertions inspect the compatibility projection directly.
    #[cfg(test)]
    pub(crate) fn selected_station_key_id(&self) -> Option<&str> {
        self.selected_station_key_id.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn fallback_count(&self) -> i64 {
        self.fallback_count
    }

    fn local_buffered(
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        request: &CanonicalProxyRequest,
        lifecycle_status: &str,
    ) -> Self {
        Self::local_buffered_with_fallback(status, headers, body, request, lifecycle_status, 0, 0)
    }

    fn local_buffered_with_fallback(
        status: StatusCode,
        headers: HeaderMap,
        body: Bytes,
        request: &CanonicalProxyRequest,
        completion_source: &str,
        fallback_count: i64,
        attempt_count: i64,
    ) -> Self {
        let body_bytes = body.len() as i64;
        Self {
            status,
            headers,
            body: ProxyExecutionBody::Buffered(body),
            selected_station_key_id: None,
            selected_station_id: None,
            fallback_count,
            lifecycle: ExecutionLifecycleEvidence {
                annotations: RequestLogAnnotations {
                    model: request.model.clone(),
                    stream: request.stream,
                    http_status: None,
                    selected_station_key_id: None,
                    selected_station_id: None,
                    upstream_base_url: None,
                    route_policy: None,
                    route_reason: None,
                    rejected_candidates_json: Some("[]".to_string()),
                    body_bytes: Some(body_bytes),
                    route_wait_ms: Some(0),
                    upstream_headers_ms: None,
                    failure_source: None,
                    attempts_json: None,
                    completion_source: Some(completion_source.to_string()),
                    prompt_tokens: None,
                    completion_tokens: None,
                    total_tokens: None,
                    cache_creation_tokens: None,
                    cache_read_tokens: None,
                    reasoning_effort: request.reasoning_effort.clone(),
                    first_token_ms: None,
                    billing_mode: None,
                },
                selected_attempt: None,
                selected_attempt_cost: None,
                attempt_count: attempt_count.max(0) as u16,
                fallback_count: fallback_count.max(0) as u16,
            },
        }
    }
}

pub(crate) struct UpstreamAttemptExecutor {
    pool: UpstreamClientPool,
}

impl UpstreamAttemptExecutor {
    pub(crate) fn new(pool: UpstreamClientPool) -> Self {
        Self { pool }
    }
}

impl AttemptExecutor for UpstreamAttemptExecutor {
    fn attempt<'a>(
        &'a self,
        request: &'a CanonicalProxyRequest,
        target: &'a ExecutionTargetHandle,
        mapped_model: Option<&'a str>,
    ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>> {
        Box::pin(async move {
            let adapter = EndpointAdapter::for_endpoint(&request.endpoint);
            let prepared = adapter.prepare_for_format(
                request,
                target.upstream_api_format.clone(),
                mapped_model,
            )?;
            let response_plan = prepared.response_plan;
            let attempt = self
                .pool
                .send_resolved_with_policy(prepared, target, request.transport_policy())
                .await?;
            match attempt {
                UpstreamAttempt::Buffered {
                    status,
                    headers,
                    body,
                    diagnostic_memory,
                } => {
                    if !status.is_success() {
                        let parser_memory = self
                            .pool
                            .diagnostic_memory_budget()
                            .try_reserve(
                                crate::services::proxy::diagnostic_memory::JSON_PARSER_SCRATCH_BYTES,
                            )
                            .map_err(|_| diagnostic_memory_saturated_failure())?;
                        let failure = upstream_http_failure(
                            status,
                            &headers,
                            Some(&body),
                            request,
                            target,
                            mapped_model,
                        )
                        .with_request_send_phase(RequestSendPhase::ResponseStarted);
                        drop(parser_memory);
                        drop(diagnostic_memory);
                        return Err(failure);
                    }
                    drop(diagnostic_memory);
                    if let Some(failure) = detect_buffered_semantic_error(
                        status,
                        &headers,
                        &body,
                        request,
                        target,
                        mapped_model,
                    ) {
                        return Err(failure);
                    }
                    let body = validate_buffered_body(body, response_plan.completion_policy)?;
                    let body = transform_buffered_body(
                        body,
                        response_plan.downstream_transform,
                        mapped_model,
                    )?;
                    Ok(PreparedAttempt::Buffered {
                        status,
                        headers: response_headers_for_downstream(&headers),
                        body,
                    })
                }
                UpstreamAttempt::Stream {
                    status,
                    headers,
                    chunks,
                } => {
                    if !status.is_success() {
                        return Err(upstream_http_failure(
                            status,
                            &headers,
                            None,
                            request,
                            target,
                            mapped_model,
                        )
                        .with_request_send_phase(RequestSendPhase::ResponseStarted));
                    }
                    let chunks = transform_stream_body(
                        chunks,
                        response_plan.downstream_transform,
                        mapped_model,
                    );
                    Ok(PreparedAttempt::Stream {
                        status,
                        headers: response_headers_for_downstream(&headers),
                        chunks,
                        completion_policy: response_plan.completion_policy,
                        diagnostic_memory: None,
                    })
                }
            }
        })
    }
}

fn transform_stream_body(
    chunks: ByteStream,
    downstream_transform: DownstreamTransform,
    mapped_model: Option<&str>,
) -> ByteStream {
    if downstream_transform == DownstreamTransform::ChatToResponses {
        chat_sse_to_responses_stream(chunks, mapped_model)
    } else {
        chunks
    }
}

impl TransportPolicySnapshot {
    #[cfg(test)]
    fn for_tests(
        precommit_budget: Duration,
        first_byte_timeout: Duration,
        buffered_budget: Duration,
    ) -> Self {
        Self {
            version: Self::VERSION,
            connect_timeout: Duration::from_secs(10),
            request_deadline: precommit_budget,
            first_byte_timeout,
            buffered_execution_timeout: buffered_budget,
            stream_idle_timeout: Duration::from_secs(90),
            upstream_pool_idle_timeout: Duration::from_secs(90),
            source_routing_policy_revision: 1,
        }
    }

    // Exposed only for retry-budget unit assertions.
    #[cfg(test)]
    pub(crate) fn precommit_budget(&self) -> Duration {
        self.request_deadline
    }

    #[cfg(test)]
    pub(crate) fn buffered_budget(&self) -> Duration {
        self.buffered_execution_timeout
    }
}

fn capacity_domain_from_failure(
    failure: &crate::application::request_finalization::failure::CanonicalFailure,
) -> Option<CapacityDomainCommitment> {
    let crate::application::request_finalization::failure::FailureTarget::ProviderCapacity {
        domain_commitment,
    } = &failure.target
    else {
        return None;
    };
    CapacityDomainCommitment::from_canonical(domain_commitment)
}

impl fmt::Debug for ProxyExecutionResponse {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProxyExecutionResponse")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .field("body", &self.body)
            .field("selected_station_key_id", &self.selected_station_key_id)
            .field("selected_station_id", &self.selected_station_id)
            .field("fallback_count", &self.fallback_count)
            .field("lifecycle", &self.lifecycle)
            .finish()
    }
}

impl fmt::Debug for ProxyExecutionBody {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Buffered(body) => formatter
                .debug_struct("Buffered")
                .field("body_len", &body.len())
                .finish(),
            Self::Stream { .. } => formatter.write_str("Stream"),
        }
    }
}

fn record_trace_event(
    builder: &mut Option<DecisionTraceBuilder>,
    kind: DecisionTraceEventKind,
    code: &'static str,
    ordinal: u32,
    detail: Option<&str>,
) {
    if let Some(builder) = builder {
        if let Ok(event) = DecisionTraceEvent::new(kind, code, ordinal, detail) {
            let _ = builder.record(event);
        }
    }
}

fn retry_action_trace_detail(action: &RetryAction) -> String {
    // DecisionTraceProfileV1 validates the detail alphabet and length. Keep
    // this compact and deterministic; no endpoint, request body or secret is
    // ever included.
    let delay = action.wait_delay_ms.unwrap_or(0);
    format!(
        "action_{}_failure_{}_attempt_{}_remaining_{}_budget_{}_policy_{}_excluded_{}_delay_{}",
        action.kind.as_trace_label(),
        action.failure_code,
        action.attempt_ordinal,
        action.remaining_attempt_budget,
        action.remaining_precommit_budget_ms.unwrap_or(0),
        action.policy_revision,
        action.excluded_failure_domain_count,
        delay
    )
}

fn attempt_budget_trace_detail(profile: AttemptBudgetProfileV1) -> String {
    format!(
        "profile_total_{}_same_{}_wait_{}_cross_{}",
        profile.max_total_attempts,
        profile.max_same_target_capacity_retries,
        profile.capacity_retry_wait_budget_ms,
        u8::from(profile.allow_cross_capacity_domain_fallback),
    )
}

fn finish_decision_trace(
    builder: Option<DecisionTraceBuilder>,
    runtime: &crate::services::proxy::routing_runtime::RoutingRuntimeState,
) {
    if let Some(builder) = builder {
        runtime.record_decision_trace(builder.finish());
    }
}

fn internal_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::InternalProxyError,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::INTERNAL_SERVER_ERROR,
        message,
    )
}

fn model_mapping_rejection_failure(plan: &model_mapping::ResolvedModelPlan) -> ProxyFailure {
    let (code, status, message) = match plan.rejection_kind {
        Some(crate::models::model_mapping::RejectionKind::UnsupportedModel) => (
            ProxyFailureCode::UpstreamModelUnavailable,
            StatusCode::NOT_FOUND,
            "requested model is not available under the active mapping policy",
        ),
        Some(crate::models::model_mapping::RejectionKind::PolicyDenied) => (
            ProxyFailureCode::RoutePolicyRejected,
            StatusCode::FORBIDDEN,
            "request rejected by the active model mapping policy",
        ),
        Some(crate::models::model_mapping::RejectionKind::ClientNotAllowed) => (
            ProxyFailureCode::RequestBodyInvalid,
            StatusCode::BAD_REQUEST,
            "request is not allowed by the active model mapping policy",
        ),
        None => (
            ProxyFailureCode::RoutePolicyRejected,
            StatusCode::FORBIDDEN,
            "request rejected by the active model mapping policy",
        ),
    };
    ProxyFailure::new(
        code,
        FailureSource::Routing,
        RetryClass::Never,
        status,
        message,
    )
}

fn routing_configuration_required_failure() -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::RouteConfigRequired,
        FailureSource::Routing,
        RetryClass::Never,
        StatusCode::SERVICE_UNAVAILABLE,
        "routing configuration is required",
    )
}

fn duration_millis_i64(duration: Duration) -> i64 {
    duration.as_millis().min(i64::MAX as u128) as i64
}

fn duration_millis_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

/// Await a precommit operation against the ingress-owned request deadline.
/// Keeping this boundary around the whole repository future also covers all
/// nested planning, affinity, route-index and pricing reads without creating a
/// second timeout owner in the application layer.
async fn await_request_deadline<T, E, F>(
    request_deadline: Duration,
    started: Instant,
    future: F,
    map_error: impl FnOnce(E) -> ProxyFailure,
) -> Result<T, ProxyFailure>
where
    F: Future<Output = Result<T, E>>,
{
    let Some(remaining) = request_deadline.checked_sub(started.elapsed()) else {
        return Err(precommit_timeout_failure());
    };
    match tokio::time::timeout(remaining, future).await {
        Err(_) => Err(precommit_timeout_failure()),
        Ok(Ok(value)) => Ok(value),
        Ok(Err(error)) => Err(map_error(error)),
    }
}

fn precommit_deadline_ms(request_started_at_ms: i64, budget: Duration) -> i64 {
    request_started_at_ms.saturating_add(duration_millis_i64(budget))
}

fn controller_now_ms(request_started_at_ms: i64, precommit_started: Instant) -> i64 {
    request_started_at_ms.saturating_add(duration_millis_i64(precommit_started.elapsed()))
}

fn precommit_timeout_failure() -> ProxyFailure {
    // Deadline is a canonical planning outcome. Marking it as rejected before
    // acceptance keeps replay-safe deadline suppression from becoming a
    // generic PossiblyAccepted terminal merely because the action was Stop.
    ProxyFailure::from_canonical(planning_failure(
        FailureClass::Deadline,
        FailureTarget::Request,
        CanonicalRetryDisposition::StopRequest,
    ))
}

fn actual_terminal_for_action(
    failure: &ProxyFailure,
    _action: RetryAction,
) -> ActualAttemptTerminal {
    let Some(canonical) = failure.canonical() else {
        if matches!(
            failure.source,
            FailureSource::Local | FailureSource::Routing | FailureSource::Internal
        ) {
            return ActualAttemptTerminal::FailedBeforeCommit;
        }
        return if failure
            .request_send_phase
            .definitely_no_request_bytes_sent()
        {
            ActualAttemptTerminal::FailedBeforeCommit
        } else {
            ActualAttemptTerminal::PossiblyAccepted
        };
    };
    if failure
        .request_send_phase
        .definitely_no_request_bytes_sent()
    {
        return ActualAttemptTerminal::FailedBeforeCommit;
    }
    match canonical.request_acceptance {
        crate::application::request_finalization::failure::RequestAcceptance::RejectedBeforeAcceptance => {
            ActualAttemptTerminal::FailedBeforeCommit
        }
        crate::application::request_finalization::failure::RequestAcceptance::AcceptedOrMayHaveBeenAccepted => {
            ActualAttemptTerminal::PossiblyAccepted
        }
        crate::application::request_finalization::failure::RequestAcceptance::Unknown => {
            ActualAttemptTerminal::PossiblyAccepted
        }
    }
}

fn route_request_facts(
    request: &CanonicalProxyRequest,
    settings: &RoutingExecutionSettings,
    admitted_at_ms: i64,
    mapped_model: Option<&str>,
) -> RouteRequestFacts {
    RouteRequestClassifier::classify(
        CanonicalRouteRequest {
            route_kind: if matches!(request.endpoint, RouteEndpointKind::Models) {
                RouteKind::ModelCatalog
            } else {
                RouteKind::Inference
            },
            requested_model: mapped_model.map(ToString::to_string),
            stream: request.stream,
            uses_tools: request.requirements.uses_tools,
            uses_vision: request.requirements.uses_vision,
            uses_reasoning: request.requirements.uses_reasoning,
            untrusted_headers: Vec::new(),
        },
        ValidatedLocalRouteSettings {
            ordering_profile: ordering_profile(&settings.policy),
            max_rate_multiplier: settings.max_rate_multiplier,
            group_filter_mode: group_filter_mode(&settings.routing_group_scope),
            required_group_stable_key: required_group_stable_key(&settings.routing_group_scope),
            preferred_models: Vec::new(),
            required_tags: Vec::new(),
            allow_depleted_fallback: settings.allow_depleted_fallback,
            // The canonical policy is loaded with the planning snapshot. Do
            // not let the legacy execution settings enable affinity before
            // that policy has admitted it.
            affinity_enabled: false,
        },
        admitted_at_ms,
    )
}

fn mapping_endpoint_kind(
    endpoint: &RouteEndpointKind,
) -> crate::models::model_mapping::EndpointKind {
    match endpoint {
        RouteEndpointKind::Models => crate::models::model_mapping::EndpointKind::Models,
        RouteEndpointKind::ChatCompletions => {
            crate::models::model_mapping::EndpointKind::ChatCompletions
        }
        RouteEndpointKind::Responses => crate::models::model_mapping::EndpointKind::Responses,
        RouteEndpointKind::Embeddings => crate::models::model_mapping::EndpointKind::Embeddings,
    }
}

fn affinity_lookups(
    request: &CanonicalProxyRequest,
    settings: &RoutingExecutionSettings,
    model: Option<&str>,
    endpoint_revision: i64,
) -> Vec<AffinityLookup> {
    let routing_group_scope = routing_group_scope_label(&settings.routing_group_scope);
    let mut lookups = Vec::with_capacity(2);
    if let Some(previous_response_id) = nonempty(request.previous_response_id.as_deref()) {
        lookups.push(AffinityLookup::new(
            AffinityKind::PreviousResponse,
            routing_group_scope.clone(),
            previous_response_id,
            endpoint_revision,
            model,
        ));
    }
    if let Some(session_hash) = nonempty(request.session_hash.as_deref()) {
        lookups.push(AffinityLookup::new(
            AffinityKind::Session,
            routing_group_scope,
            session_hash,
            endpoint_revision,
            model,
        ));
    }
    lookups
}

fn nonempty(value: Option<&str>) -> Option<&str> {
    value.and_then(|value| {
        let value = value.trim();
        if value.is_empty() {
            None
        } else {
            Some(value)
        }
    })
}

fn ordering_profile(policy: &RoutingPolicy) -> OrderingProfile {
    match policy {
        RoutingPolicy::CheapFirst | RoutingPolicy::CostStableFirst => OrderingProfile::CostFirst,
        RoutingPolicy::AutomaticBalanced
        | RoutingPolicy::PriorityFallback
        | RoutingPolicy::StableFirst
        | RoutingPolicy::BackupOnly => OrderingProfile::PriorityFirst,
    }
}

fn group_filter_mode(filter: &crate::models::routing::RoutingGroupFilter) -> GroupFilterMode {
    match filter {
        crate::models::routing::RoutingGroupFilter::AllGroups => GroupFilterMode::Any,
        crate::models::routing::RoutingGroupFilter::UngroupedOnly => GroupFilterMode::UngroupedOnly,
        crate::models::routing::RoutingGroupFilter::GroupBindingId(_)
        | crate::models::routing::RoutingGroupFilter::GroupIdHash(_)
        | crate::models::routing::RoutingGroupFilter::GroupType(_) => GroupFilterMode::Required,
    }
}

fn required_group_stable_key(
    filter: &crate::models::routing::RoutingGroupFilter,
) -> Option<String> {
    match filter {
        crate::models::routing::RoutingGroupFilter::GroupBindingId(id) => {
            Some(format!("binding:{id}"))
        }
        crate::models::routing::RoutingGroupFilter::GroupIdHash(hash) => {
            Some(format!("group-id:{hash}"))
        }
        crate::models::routing::RoutingGroupFilter::GroupType(group_type) => {
            Some(format!("group-type:{group_type:?}").to_lowercase())
        }
        crate::models::routing::RoutingGroupFilter::AllGroups
        | crate::models::routing::RoutingGroupFilter::UngroupedOnly => None,
    }
}

fn routing_group_scope_label(filter: &RoutingGroupFilter) -> String {
    match filter {
        RoutingGroupFilter::AllGroups => "all_groups".to_string(),
        RoutingGroupFilter::UngroupedOnly => "ungrouped_only".to_string(),
        RoutingGroupFilter::GroupBindingId(id) => format!("binding:{id}"),
        RoutingGroupFilter::GroupIdHash(hash) => format!("group-id:{hash}"),
        RoutingGroupFilter::GroupType(group_type) => {
            format!("group-type:{group_type:?}").to_lowercase()
        }
    }
}

fn execution_target_failure(
    error: crate::application::operational_facts::target_resolver::ExecutionTargetError,
) -> ProxyFailure {
    let mut failure = ProxyFailure::new(
        ProxyFailureCode::RouteFactsUnavailable,
        FailureSource::Routing,
        RetryClass::BeforeOutput,
        StatusCode::SERVICE_UNAVAILABLE,
        "selected route target unavailable",
    );
    match error {
        crate::application::operational_facts::target_resolver::ExecutionTargetError::StaleTarget {
            station_key_id,
            ..
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::StaleCredentialRef {
            station_key_id,
            ..
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::TargetUnavailable {
            station_key_id,
            ..
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::MissingCredentialRef {
            station_key_id,
            ..
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::InvalidEndpoint {
            station_key_id,
            ..
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::SecretUnavailable {
            station_key_id,
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::InvalidCommitment {
            station_key_id,
        }
        | crate::application::operational_facts::target_resolver::ExecutionTargetError::CommitmentChanged {
            station_key_id,
        } => {
            failure.context_mut().candidate_id = Some(station_key_id);
        }
    }
    failure
}

fn selected_route_or_failure(
    decision: AdmissionDecision,
) -> Result<SelectedRoute, AdmissionFailure> {
    match decision {
        AdmissionDecision::Selected(selected) => {
            let _selection_evidence_count = selected.evidence.len();
            Ok(selected)
        }
        AdmissionDecision::Wait { constraint, permit } => {
            drop(permit);
            Err(AdmissionFailure {
                kind: AdmissionFailureKind::CapacityExhausted,
                evidence: vec![AdmissionEvidence {
                    code: "capacity_wait_required",
                    detail: format!("{constraint:?}"),
                }],
            })
        }
        AdmissionDecision::Replan { reason } => Err(AdmissionFailure {
            kind: AdmissionFailureKind::TemporaryHealth,
            evidence: vec![AdmissionEvidence {
                code: "replan_required",
                detail: format!("{reason:?}"),
            }],
        }),
    }
}

fn controller_failure(failure: AdmissionFailure, policy: &RoutingPolicy) -> ProxyFailure {
    if let Some(planning_failure) = route_planning_failure(&failure) {
        let stable_code = planning_failure.stable_code();
        let mut proxy_failure =
            ProxyFailure::from_public_error(planning_failure.into_canonical().public);
        proxy_failure.internal_detail = Some(stable_code.to_string());
        proxy_failure.context_mut().route_policy = Some(routing_policy_label(policy).to_string());
        return proxy_failure;
    }

    let (code, status, message) = match failure.kind {
        AdmissionFailureKind::NoEligible => (
            ProxyFailureCode::RouteNoCandidate,
            StatusCode::SERVICE_UNAVAILABLE,
            "no eligible route candidate",
        ),
        AdmissionFailureKind::TemporaryHealth => (
            ProxyFailureCode::RouteHealthUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "route health unavailable",
        ),
        AdmissionFailureKind::CapacityExhausted => (
            ProxyFailureCode::RouteCapacityExhausted,
            StatusCode::SERVICE_UNAVAILABLE,
            "route capacity exhausted",
        ),
        AdmissionFailureKind::Deadline => (
            ProxyFailureCode::RouteDeadlineExceeded,
            StatusCode::GATEWAY_TIMEOUT,
            "route deadline exceeded",
        ),
        AdmissionFailureKind::ConfigUnstable => (
            ProxyFailureCode::RouteConfigUnstable,
            StatusCode::SERVICE_UNAVAILABLE,
            "route configuration changed during planning",
        ),
        AdmissionFailureKind::AttemptLimit => (
            ProxyFailureCode::RouteNoCandidate,
            StatusCode::BAD_GATEWAY,
            "all route candidates failed",
        ),
        AdmissionFailureKind::CommitUncertain => (
            ProxyFailureCode::RouteInvariantViolation,
            StatusCode::BAD_GATEWAY,
            "route commit certainty prevents retry",
        ),
    };
    let mut proxy_failure = ProxyFailure::new(
        code,
        FailureSource::Routing,
        RetryClass::BeforeOutput,
        status,
        message,
    );
    proxy_failure.context_mut().route_policy = Some(routing_policy_label(policy).to_string());
    proxy_failure
}

fn route_planning_failure(failure: &AdmissionFailure) -> Option<RoutePlanningFailure> {
    match failure.kind {
        AdmissionFailureKind::TemporaryHealth => Some(RoutePlanningFailure::HealthUnavailable),
        AdmissionFailureKind::CapacityExhausted => Some(RoutePlanningFailure::CapacityExhausted),
        AdmissionFailureKind::Deadline => Some(RoutePlanningFailure::DeadlineExceeded),
        AdmissionFailureKind::ConfigUnstable => Some(RoutePlanningFailure::ConfigUnstable),
        AdmissionFailureKind::CommitUncertain => Some(RoutePlanningFailure::InvariantViolation {
            code: "route_commit_uncertain",
        }),
        AdmissionFailureKind::NoEligible | AdmissionFailureKind::AttemptLimit => None,
    }
}

fn pricing_basis_label(
    basis: crate::application::operational_facts::pricing_projector::RoutingCostBasis,
) -> &'static str {
    match basis {
        crate::application::operational_facts::pricing_projector::RoutingCostBasis::ExactPrice => {
            "exact_price"
        }
        crate::application::operational_facts::pricing_projector::RoutingCostBasis::MultiplierProxy => {
            "multiplier_proxy"
        }
        crate::application::operational_facts::pricing_projector::RoutingCostBasis::Unpriced => {
            "unpriced"
        }
        crate::application::operational_facts::pricing_projector::RoutingCostBasis::NotApplicable => {
            "not_applicable"
        }
    }
}

fn billing_mode_for_pricing(pricing: &RoutePlanPricingSnapshot) -> Option<String> {
    if pricing.basis
        != crate::application::operational_facts::pricing_projector::RoutingCostBasis::ExactPrice
    {
        return None;
    }
    if pricing.estimated_input_price.is_some() || pricing.estimated_output_price.is_some() {
        Some("token".to_string())
    } else {
        match pricing
            .unit
            .as_deref()
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("per_request") => Some("per_request".to_string()),
            Some("per_1m_tokens" | "per_1k_tokens" | "m") => Some("token".to_string()),
            _ => None,
        }
    }
}

fn catalog_planning_exhausted(failure: &AdmissionFailure) -> bool {
    matches!(
        failure.kind,
        AdmissionFailureKind::NoEligible | AdmissionFailureKind::AttemptLimit
    )
}

fn attach_failure_candidate(failure: &mut ProxyFailure, candidate: &RoutePlanCandidate) {
    let context = failure.context_mut();
    context.candidate_id = Some(candidate.station_key_id.clone());
    context.candidate_station_id = Some(candidate.station_id.clone());
    context.candidate_upstream_base_url = None;
}

fn routing_policy_label(policy: &RoutingPolicy) -> &'static str {
    match policy {
        RoutingPolicy::AutomaticBalanced => "automatic_balanced",
        RoutingPolicy::PriorityFallback => "priority_fallback",
        RoutingPolicy::StableFirst => "stable_first",
        RoutingPolicy::BackupOnly => "backup_only",
        RoutingPolicy::CheapFirst => "cheap_first",
        RoutingPolicy::CostStableFirst => "cost_stable_first",
    }
}

fn endpoint_path(endpoint: &crate::models::routing::RouteEndpointKind) -> &'static str {
    match endpoint {
        crate::models::routing::RouteEndpointKind::Models => "/v1/models",
        crate::models::routing::RouteEndpointKind::ChatCompletions => "/v1/chat/completions",
        crate::models::routing::RouteEndpointKind::Responses => "/v1/responses",
        crate::models::routing::RouteEndpointKind::Embeddings => "/v1/embeddings",
    }
}

fn transform_buffered_body(
    body: Bytes,
    downstream_transform: DownstreamTransform,
    mapped_model: Option<&str>,
) -> Result<Bytes, ProxyFailure> {
    if downstream_transform != DownstreamTransform::ChatToResponses {
        return Ok(body);
    }
    let value = serde_json::from_slice::<Value>(&body).map_err(|error| {
        upstream_malformed_response_failure(format!(
            "upstream chat fallback response was not JSON: {error}"
        ))
    })?;
    serde_json::to_vec(&render_responses_response(value, mapped_model))
        .map(Bytes::from)
        .map_err(|error| internal_failure(format!("serialize responses fallback failed: {error}")))
}

fn validate_buffered_body(
    body: Bytes,
    completion_policy: CompletionPolicy,
) -> Result<Bytes, ProxyFailure> {
    if completion_policy == CompletionPolicy::ValidatedJsonBody {
        serde_json::from_slice::<Value>(&body).map_err(|error| {
            upstream_malformed_response_failure(format!(
                "upstream buffered response was not JSON: {error}"
            ))
        })?;
    }
    Ok(body)
}

fn json_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        http::header::CONTENT_TYPE,
        http::HeaderValue::from_static("application/json"),
    );
    headers
}

fn extract_models(body: &Bytes) -> Result<Vec<Value>, String> {
    let value: Value = serde_json::from_slice(body)
        .map_err(|error| format!("model list JSON could not be parsed: {error}"))?;
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        return Ok(data.clone());
    }
    if let Some(data) = value.as_array() {
        return Ok(data.clone());
    }
    Err("model list response did not contain data array".to_string())
}

fn local_usage_body(snapshots: Vec<BalanceSnapshot>) -> Result<Bytes, ProxyFailure> {
    let mut latest_by_station: HashMap<String, BalanceSnapshot> = HashMap::new();
    for snapshot in snapshots {
        let should_replace = latest_by_station
            .get(&snapshot.station_id)
            .map(|current| balance_snapshot_rank(&snapshot) > balance_snapshot_rank(current))
            .unwrap_or(true);
        if snapshot.scope == "station" && should_replace {
            latest_by_station.insert(snapshot.station_id.clone(), snapshot);
        }
    }

    let latest_station_balances = latest_by_station.values().collect::<Vec<_>>();
    let total_balance = latest_station_balances
        .iter()
        .filter_map(|snapshot| snapshot.value)
        .sum::<f64>();
    let currency = latest_station_balances
        .iter()
        .find_map(|snapshot| {
            let currency = snapshot.currency.trim();
            (!currency.is_empty()).then(|| currency.to_string())
        })
        .unwrap_or_else(|| "CNY".to_string());
    let low_balance_stations = latest_station_balances
        .iter()
        .filter(|snapshot| snapshot.status == "low" || snapshot.status == "depleted")
        .count();
    let updated_at = latest_station_balances
        .iter()
        .map(|snapshot| snapshot.updated_at.as_str())
        .max()
        .map(str::to_string);

    serde_json::to_vec(&serde_json::json!({
        "is_active": true,
        "remaining": total_balance,
        "balance": total_balance,
        "unit": currency,
        "quota": {
            "remaining": total_balance,
            "unit": currency,
        },
        "source": "relay_pool_desktop_balance_snapshots",
        "stations": latest_station_balances.len(),
        "low_balance_stations": low_balance_stations,
        "updated_at": updated_at,
    }))
    .map(Bytes::from)
    .map_err(|error| internal_failure(format!("serialize local usage response failed: {error}")))
}

fn balance_snapshot_rank(snapshot: &BalanceSnapshot) -> (i128, i128, i128) {
    (
        parse_balance_time(&snapshot.updated_at),
        parse_balance_time(&snapshot.created_at),
        snapshot
            .collected_at
            .as_deref()
            .map(parse_balance_time)
            .unwrap_or(0),
    )
}

fn parse_balance_time(value: &str) -> i128 {
    value.trim().parse::<i128>().unwrap_or(0)
}

fn upstream_http_failure(
    status: StatusCode,
    headers: &HeaderMap,
    body: Option<&Bytes>,
    request: &CanonicalProxyRequest,
    target: &ExecutionTargetHandle,
    mapped_model: Option<&str>,
) -> ProxyFailure {
    let applicability = upstream_capability_applicability(&target.upstream_api_format);
    let signal = if matches!(request.endpoint, RouteEndpointKind::Responses)
        && !matches!(
            target.upstream_api_format,
            UpstreamApiFormat::OpenAiChatCompletions
        ) {
        responses_error_semantic_signal(
            status.as_u16(),
            body.and_then(parse_json_error_body).as_ref(),
            &target.station_key_id,
            &target.station_id,
            target.endpoint_revision,
            request.model.as_deref(),
            applicability,
        )
    } else {
        crate::services::proxy::adapters::openai::openai_error_semantic_signal_from_capture_for_profile(
            status.as_u16(),
            body.map(|body| {
                crate::services::proxy::adapters::error_envelope::BodyCapture::Complete(body)
            }),
            headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            headers
                .get(http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            &target.station_key_id,
            &target.station_id,
            target.endpoint_revision,
            request.model.as_deref(),
            applicability,
            trusted_capacity_domain_commitment(target, mapped_model).as_deref(),
            provider_rule_profile(target),
            target.group_binding_id.as_deref(),
        )
    };
    let canonical = failure_from_provider_signal(signal, applicability);
    let mut failure = ProxyFailure::from_canonical(canonical);
    failure.internal_detail = Some(format!("upstream HTTP {}", status.as_u16()));
    failure
}

/// A successful buffered status can still carry an OpenAI-compatible error
/// envelope. Detection uses the same bounded evidence parser/classifier as
/// non-success responses and only rejects when the envelope was recognized;
/// normal large success payloads pass through untouched.
fn detect_buffered_semantic_error(
    status: StatusCode,
    headers: &HeaderMap,
    body: &Bytes,
    request: &CanonicalProxyRequest,
    target: &ExecutionTargetHandle,
    mapped_model: Option<&str>,
) -> Option<ProxyFailure> {
    if !status.is_success() || body.is_empty() {
        return None;
    }
    let applicability = upstream_capability_applicability(&target.upstream_api_format);
    let evidence = collect_upstream_failure_evidence_for_profile(
        ErrorEnvelopeInput {
            status: status.as_u16(),
            transport: FailureTransport::Http,
            content_type: headers
                .get(http::header::CONTENT_TYPE)
                .and_then(|value| value.to_str().ok()),
            body: BodyCapture::Complete(body),
            retry_after: headers
                .get(http::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok()),
            received_at: std::time::SystemTime::now(),
        },
        provider_rule_profile(target),
    );
    if !evidence.flags.error_on_success_status || evidence.semantic_candidates.is_empty() {
        return None;
    }
    let signal = openai_semantic_signal_from_evidence(
        &evidence,
        &target.station_key_id,
        &target.station_id,
        target.endpoint_revision,
        request.model.as_deref(),
        applicability,
        trusted_capacity_domain_commitment(target, mapped_model).as_deref(),
        target.group_binding_id.as_deref(),
    );
    let canonical = failure_from_provider_signal(signal, applicability);
    let mut failure = ProxyFailure::from_canonical(canonical);
    failure.internal_detail = Some(format!(
        "upstream HTTP {} semantic error envelope",
        status.as_u16()
    ));
    Some(failure.with_request_send_phase(RequestSendPhase::ResponseStarted))
}

fn trusted_capacity_domain_commitment(
    target: &ExecutionTargetHandle,
    mapped_model: Option<&str>,
) -> Option<String> {
    let explicit_openai_protocol = matches!(
        target.upstream_api_format,
        UpstreamApiFormat::OpenAiChatCompletions | UpstreamApiFormat::OpenAiResponses
    );
    if !explicit_openai_protocol {
        return None;
    }
    let _ = mapped_model;
    target
        .commitment
        .capacity_domain
        .as_ref()
        .map(|commitment| format!("v{}:{}", commitment.schema_version, commitment.digest_hex))
}

fn provider_rule_profile(
    target: &ExecutionTargetHandle,
) -> crate::services::proxy::adapters::error_rules::ProviderRuleProfile {
    match target.station_type.trim().to_ascii_lowercase().as_str() {
        "sub2api" => crate::services::proxy::adapters::error_rules::ProviderRuleProfile::Sub2ApiV1,
        "openai" | "native_openai" => {
            crate::services::proxy::adapters::error_rules::ProviderRuleProfile::NativeOpenAiV1
        }
        _ => crate::services::proxy::adapters::error_rules::ProviderRuleProfile::GenericOpenAiCompatibleV1,
    }
}

fn upstream_malformed_response_failure(detail: impl Into<String>) -> ProxyFailure {
    let canonical = failure_from_provider_signal(
        crate::application::request_finalization::failure::ProviderErrorSemanticSignal::MalformedResponse,
        CapabilityApplicabilitySet::UnknownModelCatalog,
    );
    let mut failure = ProxyFailure::from_canonical(canonical);
    failure.internal_detail = Some(detail.into());
    failure
}

fn parse_json_error_body(body: &Bytes) -> Option<Value> {
    serde_json::from_slice(body).ok()
}

fn upstream_capability_applicability(format: &UpstreamApiFormat) -> CapabilityApplicabilitySet {
    match format {
        UpstreamApiFormat::OpenAiChatCompletions | UpstreamApiFormat::OpenAiResponses => {
            CapabilityApplicabilitySet::ConfirmedModelCatalog
        }
        UpstreamApiFormat::Auto | UpstreamApiFormat::CustomOpenAiCompatible => {
            CapabilityApplicabilitySet::UnknownModelCatalog
        }
    }
}

fn precommit_stream_failure(failure: ProxyFailure, target: &ExecutionTargetHandle) -> ProxyFailure {
    upstream_first_byte_failure(
        format!(
            "upstream stream failed before first byte: {}",
            failure.public_message
        ),
        ProviderErrorSemanticSignal::Transport {
            station_id: target.station_id.clone(),
            endpoint_revision: target.endpoint_revision,
        },
    )
}

fn precommit_stream_ended_failure(target: &ExecutionTargetHandle) -> ProxyFailure {
    upstream_first_byte_failure(
        "upstream stream ended before first byte",
        ProviderErrorSemanticSignal::Transport {
            station_id: target.station_id.clone(),
            endpoint_revision: target.endpoint_revision,
        },
    )
}

fn protocol_bootstrap_failure(
    failure: crate::services::proxy::protocol::ProtocolFailure,
) -> ProxyFailure {
    upstream_malformed_response_failure(format!("{}: {}", failure.code, failure.detail))
}

fn diagnostic_memory_saturated_failure() -> ProxyFailure {
    let mut failure = ProxyFailure::new(
        ProxyFailureCode::LocalProxyMemoryBusy,
        FailureSource::Internal,
        RetryClass::Never,
        StatusCode::SERVICE_UNAVAILABLE,
        "local diagnostic memory is saturated",
    );
    failure.internal_detail = Some("diagnostic_memory_saturated".to_string());
    failure
}

#[allow(clippy::too_many_arguments)]
fn precommit_protocol_terminal_failure(
    terminal: ProtocolTerminal,
    event: &Bytes,
    completion_policy: CompletionPolicy,
    headers: &HeaderMap,
    request: &CanonicalProxyRequest,
    target: &ExecutionTargetHandle,
    mapped_model: Option<&str>,
) -> ProxyFailure {
    match terminal {
        ProtocolTerminal::Failed => {
            let Some(json) = failure_event_json(event) else {
                return upstream_malformed_response_failure(
                    "upstream emitted an empty failure event before semantic output",
                );
            };
            let transport = match completion_policy {
                CompletionPolicy::ChatDoneSentinel => FailureTransport::ChatSseError,
                CompletionPolicy::ResponsesTerminalEvent => FailureTransport::ResponsesSseFailure,
                CompletionPolicy::ValidatedJsonBody | CompletionPolicy::LocalConstruction => {
                    return upstream_malformed_response_failure(
                        "non-SSE completion policy emitted an SSE failure event",
                    );
                }
            };
            let applicability = upstream_capability_applicability(&target.upstream_api_format);
            let signal = crate::services::proxy::adapters::openai::openai_sse_error_semantic_signal_from_capture_for_profile(
                transport,
                BodyCapture::Complete(&json),
                headers
                    .get(http::header::RETRY_AFTER)
                    .and_then(|value| value.to_str().ok()),
                &target.station_key_id,
                &target.station_id,
                target.endpoint_revision,
                request.model.as_deref(),
                applicability,
                trusted_capacity_domain_commitment(target, mapped_model).as_deref(),
                provider_rule_profile(target),
                target.group_binding_id.as_deref(),
            );
            let canonical = failure_from_provider_signal(signal, applicability);
            let mut failure = ProxyFailure::from_canonical(canonical);
            failure.internal_detail = Some(
                "upstream emitted a classified failure event before semantic output".to_string(),
            );
            failure.with_request_send_phase(RequestSendPhase::ResponseStarted)
        }
        ProtocolTerminal::Incomplete => upstream_first_byte_failure(
            "upstream stream ended incomplete before semantic output",
            ProviderErrorSemanticSignal::Transport {
                station_id: target.station_id.clone(),
                endpoint_revision: target.endpoint_revision,
            },
        ),
        ProtocolTerminal::Completed => upstream_first_byte_failure(
            "unexpected completed precommit terminal",
            ProviderErrorSemanticSignal::Transport {
                station_id: target.station_id.clone(),
                endpoint_revision: target.endpoint_revision,
            },
        ),
    }
}

fn upstream_first_byte_timeout_failure(target: &ExecutionTargetHandle) -> ProxyFailure {
    upstream_first_byte_failure(
        "upstream first byte timed out",
        ProviderErrorSemanticSignal::Timeout {
            station_id: target.station_id.clone(),
            endpoint_revision: target.endpoint_revision,
        },
    )
}

fn upstream_first_byte_failure(
    message: impl Into<String>,
    signal: ProviderErrorSemanticSignal,
) -> ProxyFailure {
    // Timeout/transport evidence keeps the canonical outcome so health and
    // observability consumers never re-derive semantics from a status code.
    // Request bytes were already sent, so non-idempotent requests stop; the
    // unified replay-safety gate decides actual retry permission.
    let canonical =
        failure_from_provider_signal(signal, CapabilityApplicabilitySet::UnknownModelCatalog);
    let mut failure = ProxyFailure::from_canonical(canonical);
    failure.internal_detail = Some(message.into());
    // No production phase proves "response started"; Unknown keeps the
    // replay gate conservative for non-idempotent requests.
    failure.with_request_send_phase(RequestSendPhase::Unknown)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc, Mutex,
        },
        time::{Duration, Instant},
    };

    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream, StreamExt};
    use http::{HeaderMap, HeaderValue, StatusCode};

    use crate::{
        application::{
            credentials::{
                ExecutionCredentialError, ExecutionCredentialResolver, SecretBytes, SecretRef,
            },
            operational_facts::target_resolver::{ExecutionTargetHandle, ExecutionTargetRef},
            routing_engine::request::RouteRequestFacts,
            routing_engine::{
                algorithm_profile::DispatchAlgorithmProfile,
                planning_snapshot::{CandidateSnapshot, PlanningSnapshot},
            },
            routing_execution_reader::RoutingExecutionReadError,
        },
        models::{
            pricing::BalanceSnapshot,
            proxy::UpstreamApiFormat,
            routing::{CanonicalRoutingCandidate, RouteEndpointKind, StationKeyCapabilities},
        },
        services::proxy::{
            error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
            limits::{BodyBudget, RequestLease},
            request::{CanonicalProxyRequest, RequestRequirements},
            routing_repository::{
                admission_profile_from_candidate, route_projection_from_runtime,
                OperationalRouteSnapshot, RoutingExecutionSettings, RoutingRepository,
            },
        },
    };

    use super::{
        actual_terminal_for_action, await_request_deadline, internal_failure,
        transform_stream_body, validate_buffered_body, ActualAttemptTerminal, AffinityKind,
        AffinityLookup, AttemptExecutor, ExecutionEngine, HealthProbeAdmissionMode,
        ModelsAggregationDisposition, ModelsRetryAdapter, PreparedAttempt, ReplayGateResult,
        RetryActionContext, RetryActionKind, RetryActionPlanner, TransportPolicySnapshot,
    };
    use crate::services::proxy::request_send::RequestSendPhase;
    use crate::{
        observability::decision_trace::DecisionTraceEventKind,
        services::proxy::protocol::{CompletionPolicy, DownstreamTransform},
    };

    #[test]
    fn validated_buffered_body_rejects_non_json_without_exposing_body() {
        let failure = validate_buffered_body(
            Bytes::from_static(b"not-json"),
            CompletionPolicy::ValidatedJsonBody,
        )
        .expect_err("invalid buffered response must fail");

        assert_eq!(failure.code, ProxyFailureCode::UpstreamMalformedResponse);
        assert!(failure
            .internal_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("was not JSON")));
        assert!(!failure
            .internal_detail
            .as_deref()
            .is_some_and(|detail| detail.contains("not-json")));
    }

    #[test]
    fn non_validated_buffered_body_is_preserved() {
        let body = validate_buffered_body(
            Bytes::from_static(b"event-stream-payload"),
            CompletionPolicy::ChatDoneSentinel,
        )
        .expect("non-JSON policy must not parse buffered body");

        assert_eq!(body, Bytes::from_static(b"event-stream-payload"));
    }

    #[test]
    fn transport_policy_uses_the_approved_transport_budgets() {
        let policy = TransportPolicySnapshot::default();

        assert_eq!(policy.precommit_budget(), Duration::from_secs(60));
        assert_eq!(policy.buffered_budget(), Duration::from_secs(300));
    }

    #[test]
    fn replay_gate_consumes_every_transport_boundary_and_fails_closed() {
        for (phase, expected) in [
            (
                RequestSendPhase::NotConnected,
                RetryActionKind::TryDifferentFailureDomain,
            ),
            (
                RequestSendPhase::ConnectedNoHeaders,
                RetryActionKind::TryDifferentFailureDomain,
            ),
            (RequestSendPhase::HeadersSent, RetryActionKind::StopRequest),
            (
                RequestSendPhase::BodyPartiallySent,
                RetryActionKind::StopRequest,
            ),
            (
                RequestSendPhase::BodyFullySent,
                RetryActionKind::StopRequest,
            ),
            (
                RequestSendPhase::ResponseStarted,
                RetryActionKind::StopRequest,
            ),
            (RequestSendPhase::Unknown, RetryActionKind::StopRequest),
        ] {
            let failure = failure(500).with_request_send_phase(phase);
            assert_eq!(
                RetryActionPlanner::plan(&failure, false, false).kind,
                expected,
                "phase {phase:?}"
            );
        }

        let idempotent = failure(500).with_request_send_phase(RequestSendPhase::Unknown);
        assert_eq!(
            RetryActionPlanner::plan(&idempotent, true, false).kind,
            RetryActionKind::TryDifferentFailureDomain
        );
    }

    #[test]
    fn retry_action_planner_preserves_canonical_intent() {
        assert_eq!(
            RetryActionPlanner::plan(&failure(429), true, false).kind,
            RetryActionKind::WaitThenReplan
        );
        assert_eq!(
            RetryActionPlanner::plan(&failure(500), true, false).kind,
            RetryActionKind::TryDifferentFailureDomain
        );
        let capacity = ProxyFailure::from_canonical(super::failure_from_provider_signal(
            crate::application::request_finalization::failure::ProviderErrorSemanticSignal::ProviderCapacity {
                domain_commitment: format!("v1:{}", "a".repeat(64)),
                retry_after_ms: None,
            },
            super::CapabilityApplicabilitySet::UnknownModelCatalog,
        ));
        assert_eq!(
            RetryActionPlanner::plan(&capacity, true, false).kind,
            RetryActionKind::RetrySameTarget
        );
        assert_eq!(
            RetryActionPlanner::plan(&failure(400), true, false).kind,
            RetryActionKind::StopRequest
        );
    }

    #[test]
    fn models_retry_adapter_keeps_read_only_aggregation_distinct_from_inference() {
        for (failure, expected) in [
            (
                failure(429),
                ModelsAggregationDisposition::ContinueCandidate,
            ),
            (
                failure(500),
                ModelsAggregationDisposition::ContinueCandidate,
            ),
            (failure(400), ModelsAggregationDisposition::StopAggregation),
        ] {
            let action = RetryActionPlanner::plan(&failure, true, false);
            assert_eq!(ModelsRetryAdapter::disposition(&action), expected);
            assert_eq!(
                ModelsRetryAdapter::allows_next_candidate(&action),
                matches!(expected, ModelsAggregationDisposition::ContinueCandidate)
            );
        }
    }

    #[tokio::test]
    async fn request_deadline_helper_cancels_a_slow_precommit_future() {
        let started = Instant::now();
        let failure = await_request_deadline(
            Duration::from_millis(10),
            started,
            async {
                tokio::time::sleep(Duration::from_millis(100)).await;
                Ok::<_, ()>(())
            },
            |_| internal_failure("unexpected test error"),
        )
        .await
        .expect_err("slow precommit future must be cancelled");
        assert_eq!(failure.code, ProxyFailureCode::RouteDeadlineExceeded);
    }

    #[test]
    fn typed_repository_deadline_maps_to_route_deadline_failure() {
        let failure = super::planning_snapshot_repository_failure(
            crate::application::routing_execution_reader::RoutingExecutionReadError::DeadlineExceeded,
            false,
        );
        assert_eq!(failure.code, ProxyFailureCode::RouteDeadlineExceeded);
    }

    #[test]
    fn retry_action_carries_bounded_execution_evidence() {
        let action = RetryActionPlanner::plan_with_context(
            &failure(429),
            true,
            false,
            RetryActionContext {
                attempt_ordinal: 2,
                policy_revision: 17,
                remaining_attempt_budget: 1,
                remaining_precommit_budget_ms: Some(900),
                excluded_failure_domain_count: 0,
            },
        );
        assert_eq!(action.kind, RetryActionKind::WaitThenReplan);
        assert_eq!(action.reason_key, "rate_limit_wait");
        assert_eq!(action.explanation_key, "routing.retry.waitThenReplan");
        assert_eq!(action.failure_code, "upstream_rate_limited");
        assert_eq!(action.replay, ReplayGateResult::Allowed);
        assert_eq!(action.attempt_ordinal, 2);
        assert_eq!(action.policy_revision, 17);
        assert_eq!(action.remaining_attempt_budget, 1);
        assert_eq!(action.remaining_precommit_budget_ms, Some(900));
        assert_eq!(action.wait_delay_ms, None);
    }

    #[test]
    fn replay_safe_deadline_stop_is_failed_before_commit() {
        let failure = super::precommit_timeout_failure();
        let action = RetryActionPlanner::plan_with_context(
            &failure,
            true,
            false,
            RetryActionContext {
                attempt_ordinal: 1,
                policy_revision: 9,
                remaining_attempt_budget: 0,
                remaining_precommit_budget_ms: Some(0),
                excluded_failure_domain_count: 0,
            },
        );
        assert_eq!(action.kind, RetryActionKind::StopRequest);
        assert_eq!(
            super::actual_terminal_for_action(&failure, action),
            ActualAttemptTerminal::FailedBeforeCommit
        );
    }

    #[test]
    fn capacity_action_carries_retry_after_delay_when_it_is_safe_to_wait() {
        let failure = ProxyFailure::from_canonical(super::failure_from_provider_signal(
            crate::application::request_finalization::failure::ProviderErrorSemanticSignal::ProviderCapacity {
                domain_commitment: format!("v1:{}", "a".repeat(64)),
                retry_after_ms: Some(120),
            },
            super::CapabilityApplicabilitySet::UnknownModelCatalog,
        ));
        let action = RetryActionPlanner::plan_with_context(
            &failure,
            true,
            false,
            RetryActionContext {
                attempt_ordinal: 0,
                policy_revision: 4,
                remaining_attempt_budget: 2,
                remaining_precommit_budget_ms: Some(500),
                excluded_failure_domain_count: 0,
            },
        );
        assert_eq!(action.kind, RetryActionKind::RetrySameTarget);
        assert_eq!(action.wait_delay_ms, Some(120));
        assert_eq!(action.failure_code, "upstream_overloaded");
    }

    #[test]
    fn wait_action_fails_closed_when_delay_consumes_remaining_deadline() {
        let failure = ProxyFailure::from_canonical(super::failure_from_provider_signal(
            crate::application::request_finalization::failure::ProviderErrorSemanticSignal::RateLimited {
                station_id: "fixture-station".to_string(),
                retry_after_ms: Some(1_000),
            },
            super::CapabilityApplicabilitySet::UnknownModelCatalog,
        ));
        let action = RetryActionPlanner::plan_with_context(
            &failure,
            true,
            false,
            RetryActionContext {
                attempt_ordinal: 0,
                policy_revision: 3,
                remaining_attempt_budget: 2,
                remaining_precommit_budget_ms: Some(500),
                excluded_failure_domain_count: 0,
            },
        );
        assert_eq!(action.kind, RetryActionKind::StopRequest);
        assert_eq!(action.reason_key, "deadline_budget_exhausted");
        assert_eq!(
            action.replay,
            ReplayGateResult::Allowed,
            "the replay gate allowed the action; the deadline suppressed it"
        );
        assert_eq!(action.remaining_attempt_budget, 2);
        assert_eq!(action.remaining_precommit_budget_ms, Some(500));
        assert_eq!(action.wait_delay_ms, None);
    }

    #[test]
    fn rejected_before_acceptance_never_becomes_possibly_accepted_terminal() {
        let failure = failure(400);
        let action = RetryActionPlanner::plan_with_context(
            &failure,
            true,
            false,
            RetryActionContext::default(),
        );
        assert_eq!(
            actual_terminal_for_action(&failure, action),
            ActualAttemptTerminal::FailedBeforeCommit
        );
    }

    #[tokio::test]
    async fn fake_transport_boundaries_drive_the_production_replay_path() {
        for (phase, expected_ids) in [
            (RequestSendPhase::NotConnected, vec!["a", "b"]),
            (RequestSendPhase::ConnectedNoHeaders, vec!["a", "b"]),
            (RequestSendPhase::HeadersSent, vec!["a"]),
            (RequestSendPhase::BodyPartiallySent, vec!["a"]),
            (RequestSendPhase::BodyFullySent, vec!["a"]),
            (RequestSendPhase::ResponseStarted, vec!["a"]),
            (RequestSendPhase::Unknown, vec!["a"]),
        ] {
            let repository = Arc::new(FakeRepository::with_candidates(vec![
                rich_candidate("a"),
                rich_candidate("b"),
            ]));
            let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
                Err(failure(500).with_request_send_phase(phase)),
                Ok(buffered_success(b"{\"ok\":true}")),
            ]));
            let result = test_engine(repository, attempts.clone())
                .execute(canonical_chat_request().await)
                .await;

            assert_eq!(attempts.seen_ids(), expected_ids, "phase {phase:?}");
            if phase.definitely_no_request_bytes_sent() {
                assert!(result.is_ok(), "phase {phase:?} should replay");
            } else {
                assert!(result.is_err(), "phase {phase:?} must fail closed");
            }
        }
    }

    #[tokio::test]
    async fn execution_engine_preserves_route_order_and_finalizes_one_candidate() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(429)),
            Ok(buffered_success(b"{\"ok\":true}")),
        ]));
        let engine = test_engine(repository.clone(), attempts.clone());

        let response = engine
            .execute(canonical_chat_request().await)
            .await
            .expect("response");

        assert_eq!(attempts.seen_ids(), ["a", "b"]);
        assert_eq!(response.selected_station_key_id(), Some("b"));
        assert_eq!(response.fallback_count(), 1);
    }

    #[tokio::test]
    async fn probe_lease_race_forces_strict_snapshot_refresh() {
        let repository = Arc::new(FakeRepository::with_probe_race(
            vec![rich_candidate("a"), rich_candidate("b")],
            "a",
            crate::application::health_protection::HealthProtectionStatus {
                version: crate::application::health_protection::HEALTH_PROTECTION_VERSION
                    .to_string(),
                scope: crate::application::error_rate_protection::admission_scope(
                    crate::application::health_protection::HealthProtectionScopeKind::Credential,
                    "a",
                ),
                state: crate::application::health_protection::HealthProtectionState::Open,
                persistence_kind:
                    crate::application::health_protection::HealthProtectionPersistenceKind::Durable,
                state_revision: 7,
                opened_at_ms: Some(1),
                cooldown_until_ms: Some(1),
                cooldown_remaining_ms: Some(0),
                half_open_probe_in_flight: false,
                recent_failure_code: Some(
                    crate::application::health_protection::HealthProtectionFailureCode::ConnectFailure,
                ),
                sample_count: 5,
                failure_rate_percent: 100,
                updated_at_ms: 1,
                detail_available: true,
            },
        ));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![Ok(buffered_success(
            b"{\"ok\":true}",
        ))]));
        let engine = test_engine(repository.clone(), attempts.clone());

        let response = engine
            .execute(canonical_chat_request().await)
            .await
            .expect("healthy candidate should remain available after lease race");

        assert_eq!(attempts.seen_ids(), ["b"]);
        assert_eq!(
            repository.planning_probe_modes(),
            [
                HealthProbeAdmissionMode::Normal,
                HealthProbeAdmissionMode::StrictAfterLeaseRace
            ]
        );
    }

    #[tokio::test]
    async fn execution_prefers_live_session_affinity_when_candidate_is_eligible() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![Ok(buffered_success(
            b"{\"ok\":true}",
        ))]));
        let engine = test_engine(repository, attempts.clone());
        engine
            .affinity
            .lock()
            .expect("affinity lock")
            .bind(
                AffinityLookup::new(
                    AffinityKind::Session,
                    "all_groups",
                    "session-test",
                    1,
                    Some("gpt-test"),
                ),
                "b",
                crate::services::time::now_millis_for_services() as i64,
                300_000,
            )
            .expect("bind affinity");

        let response = engine
            .execute(canonical_chat_request_with_session("session-test").await)
            .await
            .expect("response");

        assert_eq!(attempts.seen_ids(), ["b"]);
        assert_eq!(response.selected_station_key_id(), Some("b"));
    }

    #[tokio::test]
    async fn execution_reloads_snapshots_after_runtime_revision_changes() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(429)),
            Ok(buffered_success(b"{\"ok\":true}")),
        ]));
        let engine = test_engine(repository.clone(), attempts.clone());
        attempts.bump_runtime_on_next_attempt(engine.routing_runtime.clone());

        let response = engine
            .execute(canonical_chat_request().await)
            .await
            .expect("response after replan");

        assert_eq!(attempts.seen_ids(), ["a", "b"]);
        assert_eq!(response.selected_station_key_id(), Some("b"));
        assert!(repository.planning_loads() >= 2);
    }

    #[tokio::test]
    async fn execution_try_different_failure_domain_excludes_shared_domain_on_replan() {
        let mut first = rich_candidate("a");
        first.station_id = "shared-station".to_string();
        let mut sibling = rich_candidate("b");
        sibling.station_id = "shared-station".to_string();
        let distinct = rich_candidate("c");
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            first, sibling, distinct,
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(500)),
            Ok(buffered_success(b"{\"ok\":true}")),
        ]));
        let engine = test_engine(repository.clone(), attempts.clone());

        let mut request = canonical_chat_request().await;
        request.idempotency_key = Some("fixture-idempotency-key".to_string());
        let response = engine
            .execute(request)
            .await
            .expect("distinct failure domain should be selected");

        assert_eq!(attempts.seen_ids(), ["a", "c"]);
        assert_eq!(response.selected_station_key_id(), Some("c"));
        assert!(repository.planning_loads() >= 2);
        let deadlines = repository.planning_deadlines();
        assert!(deadlines.len() >= 2);
        assert!(deadlines.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[tokio::test]
    async fn affinity_binding_uses_the_canonical_policy_ttl_and_enabled_flag() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![rich_candidate("a")]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(Vec::new()));
        let engine = test_engine(repository, attempts);
        let request = canonical_chat_request_with_session("session-test").await;
        let settings = RoutingExecutionSettings::default();
        let candidate = crate::application::routing_engine::candidate_plan::RoutePlanCandidate {
            station_key_id: "a".to_string(),
            station_id: "station-a".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: None,
            model_alias_revision: 1,
            model_variant: None,
            capacity_domain: None,
            capacity_domain_revision: None,
            priority: 0,
            tier: crate::application::routing_engine::candidate_plan::AvailabilityTier::Primary,
            pricing: crate::application::routing_engine::candidate_plan::RoutePlanPricingSnapshot {
                basis: crate::application::operational_facts::pricing_projector::RoutingCostBasis::Unpriced,
                rate_multiplier: None,
                currency: None,
                unit: None,
                estimated_input_price: None,
                estimated_output_price: None,
                estimated_cache_creation_price: None,
                estimated_cache_read_price: None,
                status_label: "test".to_string(),
            },
            evidence: Vec::new(),
        };
        let now_ms = 10_000;
        let mut enabled = crate::models::routing_policy::RoutingPolicyConfigV1::default();
        enabled.affinity_enabled = true;
        enabled.affinity_ttl_seconds = 1_200;
        engine.bind_success_affinity(
            &request,
            &settings,
            Some("gpt-test"),
            &candidate,
            &enabled.clone().into(),
            now_ms,
        );
        let lookup = AffinityLookup::new(
            AffinityKind::Session,
            "all_groups",
            "session-test",
            1,
            Some("gpt-test"),
        );
        let hit = engine
            .affinity
            .lock()
            .expect("affinity lock")
            .lookup(&lookup, now_ms + 1)
            .expect("canonical affinity binding");
        assert_eq!(hit.expires_at_ms, now_ms + 1_200_000);

        let disabled = crate::models::routing_policy::RoutingPolicyConfigV1::default();
        engine.bind_success_affinity(
            &request,
            &settings,
            Some("gpt-test"),
            &candidate,
            &disabled.clone().into(),
            now_ms + 2,
        );
        let retained = engine
            .affinity
            .lock()
            .expect("affinity lock")
            .lookup(&lookup, now_ms + 3)
            .expect("disabled policy must not overwrite the binding");
        assert_eq!(retained.expires_at_ms, now_ms + 1_200_000);
    }

    #[tokio::test]
    async fn models_aggregation_preserves_attempt_count_in_lifecycle_evidence() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(500)),
            Ok(buffered_success(
                br#"{"data":[{"id":"gpt-test","object":"model"}]}"#,
            )),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let response = engine
            .execute(canonical_models_request().await)
            .await
            .expect("models response");

        assert_eq!(attempts.seen_ids(), ["a", "b"]);
        assert_eq!(response.lifecycle.attempt_count, 2);
        assert_eq!(response.lifecycle.fallback_count, 1);
    }

    #[tokio::test]
    async fn models_aggregation_stop_action_does_not_probe_later_candidates() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(400)),
            Ok(buffered_success(br#"{"data":[{"id":"gpt-test"}]}"#)),
        ]));
        let failure = test_engine(repository, attempts.clone())
            .execute(canonical_models_request().await)
            .await
            .expect_err("authentication is a terminal aggregation action");

        assert_eq!(failure.code, ProxyFailureCode::UpstreamRequestRejected);
        assert_eq!(attempts.seen_ids(), ["a"]);
    }

    #[tokio::test]
    async fn possibly_accepted_5xx_does_not_replay_non_idempotent_request() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
            rich_candidate("c"),
            rich_candidate("d"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Err(failure(500)),
            Err(failure(500)),
            Err(failure(500)),
            Ok(buffered_success(b"{\"unexpected\":true}")),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let failure = engine
            .execute(canonical_chat_request().await)
            .await
            .expect_err("first three failed candidates stop request");

        assert_eq!(attempts.seen_ids(), ["a"]);
        assert_eq!(failure.code, ProxyFailureCode::UpstreamUnavailable);
    }

    #[tokio::test]
    async fn execution_engine_enforces_one_precommit_budget_across_candidates() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::delayed_responses(
            vec![
                Err(failure(500)),
                Ok(buffered_success(b"{\"too_late\":true}")),
            ],
            Duration::from_millis(100),
        ));
        let engine = test_engine(repository, attempts.clone()).with_transport_policy(
            TransportPolicySnapshot::for_tests(
                Duration::from_millis(50),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ),
        );

        let failure = engine
            .execute(canonical_chat_request().await)
            .await
            .expect_err("precommit budget exhausted");

        assert_eq!(failure.code, ProxyFailureCode::RouteDeadlineExceeded);
        assert_eq!(attempts.seen_ids(), ["a"]);
    }

    #[tokio::test]
    async fn execution_deadline_covers_planning_repository_future() {
        let repository = Arc::new(FakeRepository::with_candidates_and_planning_delay(
            vec![rich_candidate("a")],
            Duration::from_millis(100),
        ));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![Ok(buffered_success(
            b"{\"unexpected\":true}",
        ))]));
        let engine = test_engine(repository, attempts.clone()).with_transport_policy(
            TransportPolicySnapshot::for_tests(
                Duration::from_millis(10),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ),
        );

        let failure = engine
            .execute(canonical_chat_request().await)
            .await
            .expect_err("planning must not outlive request deadline");
        assert_eq!(failure.code, ProxyFailureCode::RouteDeadlineExceeded);
        assert!(attempts.seen_ids().is_empty());
    }

    #[tokio::test]
    async fn execution_records_precommit_wait_in_upstream_headers_and_first_token_timing() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![rich_candidate("a")]));
        let attempts = Arc::new(FakeAttemptExecutor::delayed_responses(
            vec![Ok(stream_success(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
            ))],
            Duration::from_millis(30),
        ));
        let engine = test_engine(repository, attempts);

        let response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("stream response");

        assert!(
            response
                .lifecycle
                .annotations
                .upstream_headers_ms
                .is_some_and(|value| value >= 20),
            "upstream header timing must include the delayed attempt"
        );
        assert!(
            response
                .lifecycle
                .annotations
                .first_token_ms
                .is_some_and(|value| value >= 20),
            "first-token timing must include precommit wait before the bootstrapped chunk"
        );
    }

    #[tokio::test]
    async fn precommit_stream_transport_error_does_not_imply_replay_safety() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(stream_error_before_data()),
            Ok(stream_success(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
            )),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let failure = engine
            .execute(streaming_chat_request().await)
            .await
            .expect_err("precommit output state alone cannot prove replay safety");

        assert_eq!(attempts.seen_ids(), ["a"]);
        // The pre-first-byte stream error is a canonical Transport outcome
        // (not replay-safe for a non-idempotent request), so the unified
        // classifier maps it to the connect/transport failure code instead of
        // the old status-switch classification.
        assert_eq!(failure.code, ProxyFailureCode::UpstreamConnectFailed);
        assert!(failure.canonical().is_some());
    }

    #[tokio::test]
    async fn committed_stream_error_never_selects_another_candidate() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(stream_then_error(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n",
            )),
            Ok(stream_success(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"forbidden\"}}]}\n\n",
            )),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let mut response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("committed stream response");

        assert_eq!(response.selected_station_key_id(), Some("a"));
        assert_eq!(attempts.seen_ids(), ["a"]);
        let super::ProxyExecutionBody::Stream { chunks, .. } = &mut response.body else {
            panic!("expected stream body");
        };
        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n")
        );
        assert!(chunks.next().await.unwrap().is_err());
        assert_eq!(attempts.seen_ids(), ["a"]);
    }

    #[tokio::test]
    async fn execution_stream_transform_bridges_chat_sse_to_responses_sse() {
        let upstream = Box::pin(stream::iter(vec![
            Ok(Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n",
            )),
            Ok(Bytes::from_static(b"data: [DONE]\n\n")),
        ]));
        let mut bridged = transform_stream_body(
            upstream,
            DownstreamTransform::ChatToResponses,
            Some("gpt-test"),
        );
        let mut output = String::new();

        while let Some(chunk) = bridged.next().await {
            let chunk = chunk.expect("bridged chunk");
            output.push_str(std::str::from_utf8(&chunk).expect("utf8"));
        }

        assert!(output.contains("response.created"));
        assert!(output.contains("response.output_text.delta"));
        assert!(output.contains("response.completed"));
        assert!(output.contains("Hi"));
    }

    #[tokio::test]
    async fn precommit_chat_capacity_event_enters_same_target_retry_path() {
        let mut candidate = rich_candidate("a");
        candidate.station_type = "openai".to_string();
        candidate.upstream_api_format = UpstreamApiFormat::OpenAiChatCompletions;
        let repository = Arc::new(FakeRepository::with_candidates(vec![candidate]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(chat_capacity_event()),
            Ok(stream_success(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"ok\"}}]}\n\n",
            )),
        ]));
        let engine = test_engine(repository, attempts);

        let response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("capacity event must retry the same target");
        assert_eq!(response.lifecycle.attempt_count, 2);
    }

    #[tokio::test]
    async fn trusted_distinct_capacity_domain_receives_one_terminal_fallback() {
        let mut first = rich_candidate("a");
        first.station_type = "openai".to_string();
        first.upstream_api_format = UpstreamApiFormat::OpenAiChatCompletions;
        let mut sibling = rich_candidate("b");
        sibling.station_type = "openai".to_string();
        sibling.upstream_api_format = UpstreamApiFormat::OpenAiChatCompletions;
        let repository = Arc::new(FakeRepository::with_candidates(vec![first, sibling]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(chat_capacity_event()),
            Ok(chat_capacity_event()),
            Ok(chat_capacity_event()),
            Ok(chat_capacity_event()),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let failure = engine
            .execute(streaming_chat_request().await)
            .await
            .expect_err("cross-domain fallback remains terminal after one outbound attempt");

        assert_eq!(attempts.seen_ids(), ["a", "a", "a", "b"]);
        assert_eq!(failure.code, ProxyFailureCode::UpstreamOverloaded);
        let traces = engine.routing_runtime.decision_trace_snapshot();
        let trace = traces.last().expect("completed request trace");
        assert!(trace.events.iter().any(|event| {
            event.kind == DecisionTraceEventKind::CrossDomainFallback
                && event.code == "capacity_cross_domain_fallback"
        }));
    }

    #[tokio::test]
    async fn trusted_same_capacity_domain_never_rotates_to_a_sibling_key() {
        let mut first = rich_candidate("same-a");
        first.station_type = "openai".to_string();
        first.upstream_api_format = UpstreamApiFormat::OpenAiChatCompletions;
        let mut sibling = rich_candidate("same-b");
        sibling.station_type = "openai".to_string();
        sibling.upstream_api_format = UpstreamApiFormat::OpenAiChatCompletions;
        let repository = Arc::new(FakeRepository::with_candidates(vec![first, sibling]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(chat_capacity_event()),
            Ok(chat_capacity_event()),
            Ok(chat_capacity_event()),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let failure = engine
            .execute(streaming_chat_request().await)
            .await
            .expect_err("same capacity domain must not rotate to sibling key");

        assert_eq!(attempts.seen_ids(), ["same-a", "same-a", "same-a"]);
        assert_eq!(failure.code, ProxyFailureCode::UpstreamOverloaded);
        let trace = engine
            .routing_runtime
            .decision_trace_snapshot()
            .pop()
            .expect("completed request trace");
        assert!(
            !trace.events.iter().any(|event| {
                event.kind == DecisionTraceEventKind::CrossDomainFallback
                    && event.code == "capacity_cross_domain_fallback"
            }),
            "an exhausted domain without a selected trusted alternative is not a fallback"
        );
        assert!(trace.events.iter().any(|event| {
            event.kind == DecisionTraceEventKind::SameDomainFallbackSuppressed
                && event.code == "capacity_same_domain_fallback_suppressed"
        }));
    }

    #[tokio::test]
    async fn precommit_response_failed_rate_limit_uses_shared_evidence() {
        let mut candidate = rich_candidate("a");
        candidate.upstream_api_format = UpstreamApiFormat::OpenAiResponses;
        let repository = Arc::new(FakeRepository::with_candidates(vec![candidate]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![Ok(
            responses_rate_limit_event(),
        )]));
        let engine = test_engine(repository, attempts);

        let failure = engine
            .execute(streaming_responses_request().await)
            .await
            .expect_err("response.failed must remain precommit");

        assert_eq!(failure.code, ProxyFailureCode::UpstreamRateLimited);
        assert_eq!(failure.retry_after_ms, Some(3_000));
        assert_eq!(
            failure.canonical().expect("canonical SSE failure").class,
            crate::application::request_finalization::failure::FailureClass::RateLimited
        );
    }

    struct FakeRepository {
        candidates: Vec<CanonicalRoutingCandidate>,
        planning_loads: AtomicUsize,
        planning_delay: Option<Duration>,
        planning_deadlines: Arc<Mutex<Vec<Instant>>>,
        planning_probe_modes: Arc<Mutex<Vec<HealthProbeAdmissionMode>>>,
        health_statuses: Arc<Vec<crate::application::health_protection::HealthProtectionStatus>>,
        probe_race_station_key: Option<String>,
    }

    impl FakeRepository {
        fn with_candidates(candidates: Vec<CanonicalRoutingCandidate>) -> Self {
            Self {
                candidates,
                planning_loads: AtomicUsize::new(0),
                planning_delay: None,
                planning_deadlines: Arc::new(Mutex::new(Vec::new())),
                planning_probe_modes: Arc::new(Mutex::new(Vec::new())),
                health_statuses: Arc::new(Vec::new()),
                probe_race_station_key: None,
            }
        }

        fn with_probe_race(
            candidates: Vec<CanonicalRoutingCandidate>,
            race_station_key: impl Into<String>,
            status: crate::application::health_protection::HealthProtectionStatus,
        ) -> Self {
            let mut repository = Self::with_candidates(candidates);
            repository.health_statuses = Arc::new(vec![status]);
            repository.probe_race_station_key = Some(race_station_key.into());
            repository
        }

        fn with_candidates_and_planning_delay(
            candidates: Vec<CanonicalRoutingCandidate>,
            planning_delay: Duration,
        ) -> Self {
            Self {
                candidates,
                planning_loads: AtomicUsize::new(0),
                planning_delay: Some(planning_delay),
                planning_deadlines: Arc::new(Mutex::new(Vec::new())),
                planning_probe_modes: Arc::new(Mutex::new(Vec::new())),
                health_statuses: Arc::new(Vec::new()),
                probe_race_station_key: None,
            }
        }

        fn planning_loads(&self) -> usize {
            self.planning_loads.load(Ordering::Acquire)
        }

        fn planning_deadlines(&self) -> Vec<Instant> {
            self.planning_deadlines
                .lock()
                .expect("planning deadline lock")
                .clone()
        }

        fn planning_probe_modes(&self) -> Vec<HealthProbeAdmissionMode> {
            self.planning_probe_modes
                .lock()
                .expect("planning probe mode lock")
                .clone()
        }
    }

    impl RoutingRepository for FakeRepository {
        fn load_execution_settings(
            &self,
        ) -> BoxFuture<'static, Result<RoutingExecutionSettings, RoutingExecutionReadError>>
        {
            Box::pin(async { Ok(RoutingExecutionSettings::default()) })
        }

        fn load_balance_snapshots(
            &self,
        ) -> BoxFuture<'static, Result<Vec<BalanceSnapshot>, RoutingExecutionReadError>> {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn load_planning_snapshot_with_probe(
            &self,
            request: RouteRequestFacts,
            runtime: crate::application::routing_engine::planning_snapshot::RuntimeOverlaySnapshot,
            context: crate::application::routing_engine::request::PlanningRequestContext,
            _probe: Option<crate::application::health_protection::HealthProtectionProbe>,
            probe_mode: HealthProbeAdmissionMode,
        ) -> BoxFuture<
            'static,
            Result<
                Option<crate::application::routing_engine::planning_snapshot::PlanningSnapshot>,
                RoutingExecutionReadError,
            >,
        > {
            self.planning_probe_modes
                .lock()
                .expect("planning probe mode lock")
                .push(probe_mode);
            let race_station_key = self.probe_race_station_key.clone();
            let planning = self.load_planning_snapshot(request, runtime, context);
            Box::pin(async move {
                let mut snapshot = planning.await?;
                if probe_mode == HealthProbeAdmissionMode::StrictAfterLeaseRace {
                    if let (Some(snapshot), Some(race_station_key)) =
                        (snapshot.as_mut(), race_station_key.as_deref())
                    {
                        for candidate in &mut snapshot.candidates {
                            if candidate.station_key_id == race_station_key {
                                candidate.hard_eligible = false;
                            }
                        }
                    }
                }
                Ok(snapshot)
            })
        }

        fn load_health_protection_statuses(
            &self,
            _now_ms: i64,
        ) -> BoxFuture<
            'static,
            Result<
                Vec<crate::application::health_protection::HealthProtectionStatus>,
                RoutingExecutionReadError,
            >,
        > {
            let statuses = self.health_statuses.clone();
            Box::pin(async move { Ok(statuses.as_ref().clone()) })
        }

        fn begin_health_protection_probe(
            &self,
            _scope: crate::application::health_protection::HealthProtectionScope,
            _now_ms: i64,
        ) -> BoxFuture<
            'static,
            Result<
                Option<crate::application::health_protection::HealthProtectionProbe>,
                RoutingExecutionReadError,
            >,
        > {
            // The race fixture models another request atomically winning the
            // lease between status read and reservation.
            if self.probe_race_station_key.is_some() {
                Box::pin(async { Ok(None) })
            } else {
                Box::pin(async { Ok(None) })
            }
        }

        fn cancel_health_protection_probe(
            &self,
            _probe: crate::application::health_protection::HealthProtectionProbe,
            _now_ms: i64,
        ) -> BoxFuture<'static, Result<bool, RoutingExecutionReadError>> {
            Box::pin(async { Ok(false) })
        }

        fn load_planning_snapshot(
            &self,
            _request: RouteRequestFacts,
            runtime: crate::application::routing_engine::planning_snapshot::RuntimeOverlaySnapshot,
            context: crate::application::routing_engine::request::PlanningRequestContext,
        ) -> BoxFuture<
            'static,
            Result<
                Option<crate::application::routing_engine::planning_snapshot::PlanningSnapshot>,
                RoutingExecutionReadError,
            >,
        > {
            self.planning_loads.fetch_add(1, Ordering::AcqRel);
            self.planning_deadlines
                .lock()
                .expect("planning deadline lock")
                .push(context.deadline());
            let candidates = self
                .candidates
                .iter()
                .enumerate()
                .map(|(index, candidate)| CandidateSnapshot {
                    station_key_id: candidate.station_key_id.clone(),
                    station_id: candidate.station_id.clone(),
                    endpoint_revision: candidate.station_endpoint_revision,
                    credential_revision: 1,
                    account_revision: 1,
                    group_binding_id: None,
                    group_revision: None,
                    resolved_upstream_model: Some("gpt-test".to_string()),
                    model_alias_revision: 1,
                    model_variants: Vec::new(),
                    capacity_domain: fixture_capacity_domain(&candidate.station_key_id),
                    capacity_domain_revision: fixture_capacity_domain(&candidate.station_key_id)
                        .as_ref()
                        .map(|_| 1),
                    credential_available: candidate.api_key.is_some()
                        || candidate.api_key_secret.is_some(),
                    hard_eligible: candidate.schedulable,
                    backup_only: candidate.capabilities.only_use_as_backup,
                    depleted: false,
                    capability_basis_points: 10_000,
                    reliability_basis_points: 8_000,
                    responsiveness_basis_points: 8_000,
                    cost_basis_points: Some(5_000),
                    pricing: crate::application::routing_engine::candidate_plan::RoutePlanPricingSnapshot::unpriced("test"),
                    preference_basis_points: 10_000_u16
                        .saturating_sub((index as u16).saturating_mul(100)),
                    failure_domains: vec![format!("station:{}", candidate.station_id)],
                })
                .collect();
            let planning_delay = self.planning_delay;
            Box::pin(async move {
                if let Some(delay) = planning_delay {
                    tokio::time::sleep(delay).await;
                }
                Ok(Some(PlanningSnapshot {
                    snapshot_id: "test-planning-snapshot".to_string(),
                    durable_revision: 1,
                    routing_policy_revision: 1,
                    policy: {
                        let mut policy =
                            crate::models::routing_policy::RoutingPolicyConfigV1::default();
                        policy.affinity_enabled = true;
                        let mut policy: crate::models::routing_policy::RoutingPolicyConfigV2 =
                            policy.into();
                        policy.protection_profile.enabled = true;
                        policy
                    },
                    attempt_budget:
                        crate::application::routing_policy::AttemptBudgetProfileV1::from_policy(
                            1,
                            &crate::models::routing_policy::RetryFailoverPolicyV2::default(),
                        )
                        .expect("attempt budget"),
                    profile: {
                        let mut profile = DispatchAlgorithmProfile::default();
                        profile.exploit_band_basis_points = 0;
                        profile
                    },
                    candidates,
                    model_fallback_trigger: None,
                    runtime,
                }))
            })
        }

        fn load_operational_route_snapshot(
            &self,
            request: RouteRequestFacts,
            planning_snapshot: PlanningSnapshot,
        ) -> BoxFuture<'static, Result<OperationalRouteSnapshot, RoutingExecutionReadError>>
        {
            let candidates = self.candidates.clone();
            Box::pin(async move {
                let mut targets = BTreeMap::new();
                let mut profiles = BTreeMap::new();
                let mut projections = Vec::new();
                for candidate in candidates {
                    targets.insert(candidate.station_key_id.clone(), target_ref(&candidate));
                    profiles.insert(
                        candidate.station_key_id.clone(),
                        admission_profile_from_candidate(&candidate),
                    );
                    projections.push(
                        route_projection_from_runtime(&request, candidate)
                            .map_err(RoutingExecutionReadError::Internal)?,
                    );
                }
                Ok(OperationalRouteSnapshot {
                    candidates: planning_snapshot.candidates.iter().map(|candidate| crate::application::routing_engine::candidate_plan::RoutePlanCandidate {
                        station_key_id: candidate.station_key_id.clone(),
                        station_id: candidate.station_id.clone(),
                        endpoint_revision: candidate.endpoint_revision,
                        credential_revision: candidate.credential_revision,
                        account_revision: candidate.account_revision,
                        group_binding_id: candidate.group_binding_id.clone(),
                        group_revision: candidate.group_revision,
                        resolved_upstream_model: candidate.resolved_upstream_model.clone(),
                        model_alias_revision: candidate.model_alias_revision,
                        model_variant: candidate.model_variants.first().cloned(),
                        capacity_domain: candidate.capacity_domain.clone(),
                        capacity_domain_revision: candidate.capacity_domain_revision,
                        priority: 0,
                        tier: crate::application::routing_engine::candidate_plan::AvailabilityTier::Primary,
                        pricing: crate::application::routing_engine::candidate_plan::RoutePlanPricingSnapshot {
                            basis: crate::application::operational_facts::pricing_projector::RoutingCostBasis::Unpriced,
                            rate_multiplier: None,
                            currency: None,
                            unit: None,
                            estimated_input_price: None,
                            estimated_output_price: None,
                            estimated_cache_creation_price: None,
                            estimated_cache_read_price: None,
                            status_label: "test".to_string(),
                        },
                        evidence: vec![],
                    }).collect(),
                    targets,
                    profiles,
                    legacy_candidates: projections,
                })
            })
        }

        fn load_current_execution_target(
            &self,
            station_key_id: String,
        ) -> BoxFuture<'static, Result<Option<ExecutionTargetRef>, RoutingExecutionReadError>>
        {
            let target = self
                .candidates
                .iter()
                .find(|candidate| candidate.station_key_id == station_key_id)
                .map(target_ref);
            Box::pin(async move { Ok(target) })
        }
    }

    struct FakeAttemptExecutor {
        responses: Mutex<Vec<Result<PreparedAttempt, ProxyFailure>>>,
        seen_ids: Mutex<Vec<String>>,
        delay: Option<Duration>,
        runtime_to_bump: Mutex<Option<Arc<super::RoutingRuntimeState>>>,
    }

    impl FakeAttemptExecutor {
        fn responses(responses: Vec<Result<PreparedAttempt, ProxyFailure>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen_ids: Mutex::new(Vec::new()),
                delay: None,
                runtime_to_bump: Mutex::new(None),
            }
        }

        fn delayed_responses(
            responses: Vec<Result<PreparedAttempt, ProxyFailure>>,
            delay: Duration,
        ) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen_ids: Mutex::new(Vec::new()),
                delay: Some(delay),
                runtime_to_bump: Mutex::new(None),
            }
        }

        fn seen_ids(&self) -> Vec<String> {
            self.seen_ids.lock().expect("seen lock").clone()
        }

        fn bump_runtime_on_next_attempt(&self, runtime: Arc<super::RoutingRuntimeState>) {
            *self.runtime_to_bump.lock().expect("runtime bump lock") = Some(runtime);
        }
    }

    impl AttemptExecutor for FakeAttemptExecutor {
        fn attempt<'a>(
            &'a self,
            _request: &'a CanonicalProxyRequest,
            target: &'a ExecutionTargetHandle,
            _mapped_model: Option<&'a str>,
        ) -> BoxFuture<'a, Result<PreparedAttempt, ProxyFailure>> {
            self.seen_ids
                .lock()
                .expect("seen lock")
                .push(target.station_key_id.clone());
            if let Some(runtime) = self
                .runtime_to_bump
                .lock()
                .expect("runtime bump lock")
                .take()
            {
                runtime.mark_runtime_changed();
            }
            Box::pin(async move {
                if let Some(delay) = self.delay {
                    tokio::time::sleep(delay).await;
                }
                self.responses.lock().expect("responses lock").remove(0)
            })
        }
    }

    struct FakeCredentialResolver;

    impl ExecutionCredentialResolver for FakeCredentialResolver {
        fn resolve_station_key_secret_ref(
            &self,
            _station_key_id: String,
            _secret_ref: SecretRef,
        ) -> BoxFuture<'static, Result<SecretBytes, ExecutionCredentialError>> {
            Box::pin(async { Ok("test-api-key".to_string().into()) })
        }
    }

    fn test_engine(
        repository: Arc<dyn RoutingRepository>,
        attempts: Arc<dyn AttemptExecutor>,
    ) -> ExecutionEngine {
        ExecutionEngine::new(repository, Arc::new(FakeCredentialResolver), attempts)
    }

    fn failure(status: u16) -> ProxyFailure {
        let signal = match status {
            401 | 403 => crate::application::request_finalization::failure::ProviderErrorSemanticSignal::ConfirmedAuthentication { station_key_id: "fixture-key".to_string() },
            402 => crate::application::request_finalization::failure::ProviderErrorSemanticSignal::ConfirmedInsufficientBalance { station_id: "fixture-station".to_string() },
            429 => crate::application::request_finalization::failure::ProviderErrorSemanticSignal::RateLimited { station_id: "fixture-station".to_string(), retry_after_ms: None },
            400 | 409 | 422 => crate::application::request_finalization::failure::ProviderErrorSemanticSignal::BadRequest,
            500..=599 => crate::application::request_finalization::failure::ProviderErrorSemanticSignal::ServerError { station_id: "fixture-station".to_string(), endpoint_revision: 1 },
            _ => crate::application::request_finalization::failure::ProviderErrorSemanticSignal::GenericStatus {
                status,
                confidence: crate::application::request_finalization::failure::EvidenceConfidence::Unknown,
            },
        };
        ProxyFailure::from_canonical(super::failure_from_provider_signal(
            signal,
            super::CapabilityApplicabilitySet::UnknownModelCatalog,
        ))
    }

    fn stream_failure() -> ProxyFailure {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamStreamFailed,
            FailureSource::Upstream,
            RetryClass::AfterCommitStop,
            StatusCode::BAD_GATEWAY,
            "upstream stream failed",
        )
    }

    fn buffered_success(body: &'static [u8]) -> PreparedAttempt {
        PreparedAttempt::Buffered {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            body: Bytes::from_static(body),
        }
    }

    fn stream_success(first: &'static [u8]) -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![Ok(Bytes::from_static(first))])),
            completion_policy: CompletionPolicy::ChatDoneSentinel,
            diagnostic_memory: None,
        }
    }

    fn stream_error_before_data() -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![Err(stream_failure())])),
            completion_policy: CompletionPolicy::ChatDoneSentinel,
            diagnostic_memory: None,
        }
    }

    fn stream_then_error(first: &'static [u8]) -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![
                Ok(Bytes::from_static(first)),
                Err(stream_failure()),
            ])),
            completion_policy: CompletionPolicy::ChatDoneSentinel,
            diagnostic_memory: None,
        }
    }

    fn chat_capacity_event() -> PreparedAttempt {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("1"));
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers,
            chunks: Box::pin(stream::iter(vec![Ok(Bytes::from_static(
                b"event: error\ndata: {\"error\":{\"code\":\"server_is_overloaded\",\"message\":\"Please retry later.\"}}\n\n",
            ))])),
            completion_policy: CompletionPolicy::ChatDoneSentinel,
            diagnostic_memory: None,
        }
    }

    fn responses_rate_limit_event() -> PreparedAttempt {
        let mut headers = HeaderMap::new();
        headers.insert(http::header::RETRY_AFTER, HeaderValue::from_static("3"));
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers,
            chunks: Box::pin(stream::iter(vec![Ok(Bytes::from_static(
                b"event: response.failed\ndata: {\"type\":\"response.failed\",\"response\":{\"status\":\"failed\",\"error\":{\"code\":\"rate_limit_exceeded\",\"message\":\"retry\"}}}\n\n",
            ))])),
            completion_policy: CompletionPolicy::ResponsesTerminalEvent,
            diagnostic_memory: None,
        }
    }

    async fn canonical_chat_request() -> CanonicalProxyRequest {
        canonical_chat_request_with_session_value(None).await
    }

    async fn canonical_chat_request_with_session(session_hash: &str) -> CanonicalProxyRequest {
        canonical_chat_request_with_session_value(Some(session_hash.to_string())).await
    }

    async fn canonical_chat_request_with_session_value(
        session_hash: Option<String>,
    ) -> CanonicalProxyRequest {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}]}"#,
        );
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        CanonicalProxyRequest::new(
            "req-exec".to_string(),
            "/v1/chat/completions".to_string(),
            RouteEndpointKind::ChatCompletions,
            Some("gpt-test".to_string()),
            false,
            None,
            RequestRequirements::default(),
            body,
            HeaderMap::new(),
            None,
            session_hash,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(std::sync::atomic::AtomicU32::new(0))),
        )
    }

    async fn streaming_chat_request() -> CanonicalProxyRequest {
        let body = Bytes::from_static(
            br#"{"model":"gpt-test","messages":[{"role":"user","content":"hi"}],"stream":true}"#,
        );
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        CanonicalProxyRequest::new(
            "req-stream".to_string(),
            "/v1/chat/completions".to_string(),
            RouteEndpointKind::ChatCompletions,
            Some("gpt-test".to_string()),
            true,
            None,
            RequestRequirements::default(),
            body,
            HeaderMap::new(),
            None,
            None,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(std::sync::atomic::AtomicU32::new(0))),
        )
    }

    async fn streaming_responses_request() -> CanonicalProxyRequest {
        let body = Bytes::from_static(br#"{"model":"gpt-test","input":"hi","stream":true}"#);
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        CanonicalProxyRequest::new(
            "req-responses-stream".to_string(),
            "/v1/responses".to_string(),
            RouteEndpointKind::Responses,
            Some("gpt-test".to_string()),
            true,
            None,
            RequestRequirements::default(),
            body,
            HeaderMap::new(),
            None,
            None,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(std::sync::atomic::AtomicU32::new(0))),
        )
    }

    async fn canonical_models_request() -> CanonicalProxyRequest {
        let body = Bytes::new();
        let budget = BodyBudget::new(1024 * 1024);
        let body_budget = budget.acquire(body.len()).await.expect("budget");
        let permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("permit");
        CanonicalProxyRequest::new(
            "req-models".to_string(),
            "/v1/models".to_string(),
            RouteEndpointKind::Models,
            None,
            false,
            None,
            RequestRequirements::default(),
            body,
            HeaderMap::new(),
            None,
            None,
            None,
            None,
            body_budget,
            RequestLease::new(permit, Arc::new(std::sync::atomic::AtomicU32::new(0))),
        )
    }

    fn target_ref(candidate: &CanonicalRoutingCandidate) -> ExecutionTargetRef {
        ExecutionTargetRef {
            station_key_id: candidate.station_key_id.clone(),
            station_id: candidate.station_id.clone(),
            station_type: candidate.station_type.clone(),
            capacity_provider_family: Some("fixture-provider".to_string()),
            capacity_deployment_identity: Some(fixture_deployment_identity(
                &candidate.station_key_id,
            )),
            capacity_region_identity: None,
            capacity_domain_revision: Some(1),
            group_binding_id: candidate
                .economic_snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.group_binding_id.clone()),
            endpoint_revision: candidate.station_endpoint_revision,
            credential_revision: 1,
            account_revision: 1,
            group_revision: None,
            api_base_url: "https://upstream.example.test/v1".to_string(),
            upstream_api_format: candidate.upstream_api_format.clone(),
            collector_proxy_mode: candidate.collector_proxy_mode.clone(),
            collector_proxy_url: candidate.collector_proxy_url.clone(),
            enabled: candidate.schedulable,
            api_key_secret_ref: Some(SecretRef {
                id: format!("secret-{}", candidate.station_key_id),
                scope: "station_key".to_string(),
                owner_id: candidate.station_key_id.clone(),
                kind: "api_key".to_string(),
            }),
            inline_api_key_present: false,
            station_account_max_concurrency: 0,
            station_key_max_concurrency: 0,
        }
    }

    fn fixture_capacity_domain(
        station_key_id: &str,
    ) -> Option<crate::application::routing_engine::failure_domains::CapacityDomainCommitment> {
        crate::application::routing_engine::failure_domains::ProviderCapacityDomain::from_trusted_identity(
            "fixture-provider",
            "gpt-test",
            Some(&fixture_deployment_identity(station_key_id)),
            None,
        )
        .map(|domain| domain.commitment())
    }

    fn fixture_deployment_identity(station_key_id: &str) -> String {
        if station_key_id.starts_with("same-") {
            "fixture-deployment-shared".to_string()
        } else {
            format!("fixture-deployment-{station_key_id}")
        }
    }

    fn rich_candidate(id: &str) -> CanonicalRoutingCandidate {
        CanonicalRoutingCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            station_type: "newapi".to_string(),
            capacity_provider_family: None,
            capacity_deployment_identity: None,
            capacity_region_identity: None,
            capacity_domain_revision: None,
            station_account_concurrency_limit: None,
            station_endpoint_revision: 1,
            sanitized_origin: "https://upstream.example.test".to_string(),
            upstream_api_format: UpstreamApiFormat::Auto,
            routing_order: None,
            priority: 0,
            max_concurrency: 0,
            load_factor: None,
            schedulable: true,
            collector_proxy_mode: "direct".to_string(),
            collector_proxy_url: None,
            station_name: format!("Station {id}"),
            key_name: format!("Key {id}"),
            capabilities: StationKeyCapabilities {
                station_key_id: id.to_string(),
                supports_chat_completions: true,
                supports_responses: true,
                supports_embeddings: true,
                supports_stream: true,
                supports_tools: true,
                supports_vision: true,
                supports_reasoning: true,
                model_allowlist: Vec::new(),
                model_blocklist: Vec::new(),
                preferred_models: Vec::new(),
                only_use_as_backup: false,
                routing_tags: Vec::new(),
                updated_at: "0".to_string(),
            },
            health: None,
            balance_snapshot: None,
            economic_snapshot: None,
            api_key: Some("sk-test".to_string()),
            api_key_secret: None,
        }
    }
}
