use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
    sync::Arc,
    time::{Duration, Instant},
};

use bytes::Bytes;
use futures_util::{future::BoxFuture, stream, StreamExt};
use http::{HeaderMap, StatusCode};
use serde_json::Value;

use super::{
    adapters::responses::render_responses_response,
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
    limits::ProxyServerLimits,
    protocol::DownstreamTransform,
    request::{ByteStream, CanonicalProxyRequest},
    responses_chat_stream::chat_sse_to_responses_stream,
    routing_repository::{OperationalRouteSnapshot, RoutingExecutionSettings, RoutingRepository},
    upstream::{UpstreamAttempt, UpstreamClientPool},
};

use crate::{
    application::{
        operational_facts::target_resolver::{
            ExecutionCredentialResolver, ExecutionTargetHandle, ExecutionTargetRef,
            ExecutionTargetResolver, LeasedSelectedTarget,
        },
        routing_engine::{
            capacity::CompositeCapacityRegistry,
            controller::{
                ActualAttemptTerminal, ControllerDecision, ControllerFailure,
                ControllerFailureKind, ControllerPlanningInput, FallbackPolicy,
                RouteAdmissionController, RouteControllerSettings, SelectedRoute,
            },
            request::{
                CanonicalRouteRequest, GroupFilterMode, OrderingProfile, RouteKind,
                RouteRequestClassifier, RouteRequestFacts, ValidatedLocalRouteSettings,
            },
            routing_policy,
            selector::RoutePlanCandidate,
        },
    },
    models::{
        pricing::BalanceSnapshot,
        routing::{RouteEndpointKind, RoutingPolicy},
    },
    services::time::now_millis_for_services,
};

#[derive(Clone)]
pub(crate) struct ExecutionEngine {
    repository: Arc<dyn RoutingRepository>,
    credentials: Arc<dyn ExecutionCredentialResolver>,
    attempts: Arc<dyn AttemptExecutor>,
    retry_policy: RetryPolicy,
    capacity: CompositeCapacityRegistry,
    lifecycle_writer: Option<LifecycleWriter>,
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
    pub fallback_count: u16,
}

#[derive(Debug, Clone, Copy)]
struct AttemptTimings {
    request_started_at_ms: i64,
    upstream_headers_ms: i64,
    first_token_ms: i64,
}

pub(crate) enum ProxyExecutionBody {
    Buffered(Bytes),
    Stream(ByteStream),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryDecision {
    NextCandidate,
    Stop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RetryPolicy {
    max_candidate_attempts: usize,
    precommit_budget: Duration,
    first_byte_timeout: Duration,
    buffered_budget: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_candidate_attempts: 3,
            precommit_budget: Duration::from_secs(180),
            first_byte_timeout: Duration::from_secs(120),
            buffered_budget: Duration::from_secs(300),
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
            retry_policy: RetryPolicy::default(),
            capacity: CompositeCapacityRegistry::default(),
            lifecycle_writer: None,
        }
    }

    pub(crate) fn new_with_limits_and_lifecycle(
        repository: Arc<dyn RoutingRepository>,
        credentials: Arc<dyn ExecutionCredentialResolver>,
        attempts: Arc<dyn AttemptExecutor>,
        limits: &ProxyServerLimits,
        lifecycle_writer: LifecycleWriter,
    ) -> Self {
        Self {
            repository,
            credentials,
            attempts,
            retry_policy: RetryPolicy::from_limits(limits),
            capacity: CompositeCapacityRegistry::default(),
            lifecycle_writer: Some(lifecycle_writer),
        }
    }

    #[cfg(test)]
    fn with_retry_policy(mut self, retry_policy: RetryPolicy) -> Self {
        self.retry_policy = retry_policy;
        self
    }

    pub(crate) async fn execute(
        &self,
        request: CanonicalProxyRequest,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let request_started_at_ms = now_millis_for_services() as i64;
        let precommit_started = Instant::now();
        if request.local_path == "/usage" || request.local_path == "/v1/usage" {
            return self.execute_usage(request).await;
        }

        let aliases = self
            .repository
            .load_model_alias_pairs()
            .await
            .map_err(|error| internal_failure(format!("load model aliases failed: {error}")))?;
        let execution_settings = self
            .repository
            .load_execution_settings()
            .await
            .map_err(|error| internal_failure(format!("load routing settings failed: {error}")))?;
        let mapped_model = routing_policy::mapped_model(request.model.as_deref(), &aliases);
        let route_facts = route_request_facts(
            &request,
            &execution_settings,
            request_started_at_ms,
            mapped_model.as_deref(),
        );
        let snapshot = self
            .repository
            .load_operational_route_snapshot(route_facts.clone())
            .await
            .map_err(|error| {
                internal_failure(format!("load operational route snapshot failed: {error}"))
            })?;

        if matches!(request.endpoint, RouteEndpointKind::Models) {
            return self
                .execute_models(request, route_facts, snapshot, mapped_model)
                .await;
        }

        let idempotent = request.idempotency_key.is_some();
        let mut last_failure = None;
        let mut attempted_count = 0_i64;
        let mut controller = RouteAdmissionController::new(
            route_facts,
            RouteControllerSettings {
                deadline_ms: request_started_at_ms
                    + self.retry_policy.precommit_budget.as_millis() as i64,
                initial_snapshot_id: snapshot.snapshot_id.clone(),
                initial_runtime_overlay_revision: snapshot.runtime_overlay_revision,
                initial_durable_generation: snapshot.durable_generation,
                fallback_policy: FallbackPolicy {
                    has_stable_idempotency_key: idempotent,
                    non_idempotent: !idempotent,
                },
            },
            0,
        );

        for attempt_index in 0..self.retry_policy.max_candidate_attempts {
            let decision = controller
                .next(ControllerPlanningInput {
                    candidates: &snapshot.candidates,
                    affinity_station_key_id: None,
                    profiles: &snapshot.profiles,
                    capacity: &self.capacity,
                    current_runtime_overlay_revision: snapshot.runtime_overlay_revision,
                    now_ms: now_millis_for_services() as i64,
                    max_waiters_per_constraint: 0,
                })
                .map_err(|failure| controller_failure(failure, &execution_settings.policy))?;
            let ControllerDecision::Selected(selected) = decision else {
                return Err(controller_failure(
                    ControllerFailure {
                        kind: ControllerFailureKind::CapacityExhausted,
                        evidence: Vec::new(),
                    },
                    &execution_settings.policy,
                ));
            };
            attempted_count = attempted_count.max(attempt_index as i64 + 1);
            let candidate = selected.candidate.clone();
            let attempt_started_at_ms = now_millis_for_services() as i64;
            let attempt_started = Instant::now();
            let Some(remaining) = self
                .retry_policy
                .remaining_precommit_budget(precommit_started)
            else {
                return Err(precommit_timeout_failure());
            };
            let target = self
                .resolve_selected_target(selected, &snapshot.targets)
                .await;
            let attempt_result = tokio::time::timeout(remaining, async {
                let target = target?;
                let prepared = self
                    .attempts
                    .attempt(&request, &target, mapped_model.as_deref())
                    .await?;
                let upstream_headers_ms = attempt_started.elapsed().as_millis() as i64;
                let prepared = self.bootstrap_stream(prepared).await?;
                Ok((prepared, upstream_headers_ms))
            })
            .await
            .unwrap_or_else(|_| Err(precommit_timeout_failure()));
            match attempt_result {
                Ok((prepared, upstream_headers_ms)) => {
                    controller
                        .record_actual_terminal_for_station_key(
                            candidate.station_key_id.clone(),
                            ActualAttemptTerminal::Succeeded,
                        )
                        .map_err(|failure| {
                            controller_failure(failure, &execution_settings.policy)
                        })?;
                    let first_token_ms = precommit_started.elapsed().as_millis() as i64;
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
                    ));
                }
                Err(mut failure) => {
                    attach_failure_candidate(&mut failure, &candidate);
                    let decision = self.retry_policy.decide(&failure, idempotent, false);
                    self.finish_attempt(failed_attempt_record(
                        &request.request_id,
                        attempt_index as u16,
                        &candidate,
                        &failure,
                        decision,
                        attempt_started_at_ms,
                        false,
                    ))
                    .await?;
                    controller
                        .record_actual_terminal_for_station_key(
                            candidate.station_key_id.clone(),
                            if decision == RetryDecision::NextCandidate {
                                ActualAttemptTerminal::FailedBeforeCommit
                            } else {
                                ActualAttemptTerminal::PossiblyAccepted
                            },
                        )
                        .map_err(|failure| {
                            controller_failure(failure, &execution_settings.policy)
                        })?;
                    last_failure = Some(failure);
                    if decision == RetryDecision::Stop {
                        break;
                    }
                }
            }
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
        Err(failure)
    }

    async fn resolve_selected_target(
        &self,
        selected: SelectedRoute,
        targets: &BTreeMap<String, ExecutionTargetRef>,
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
        ExecutionTargetResolver::resolve(
            LeasedSelectedTarget {
                station_key_id,
                expected_endpoint_revision: selected.candidate.endpoint_revision,
                expected_secret_ref_id,
                lease: selected.lease,
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
    ) -> Result<PreparedAttempt, ProxyFailure> {
        let PreparedAttempt::Stream {
            status,
            headers,
            mut chunks,
        } = prepared
        else {
            return Ok(prepared);
        };

        loop {
            match tokio::time::timeout(self.retry_policy.first_byte_timeout(), chunks.next()).await
            {
                Ok(Some(Ok(bytes))) if bytes.is_empty() => continue,
                Ok(Some(Ok(bytes))) => {
                    let prefixed = stream::once(async move { Ok(bytes) }).chain(chunks).boxed();
                    return Ok(PreparedAttempt::Stream {
                        status,
                        headers,
                        chunks: prefixed,
                    });
                }
                Ok(Some(Err(failure))) => return Err(precommit_stream_failure(failure)),
                Ok(None) => return Err(precommit_stream_ended_failure()),
                Err(_) => return Err(upstream_first_byte_timeout_failure()),
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
        snapshot: OperationalRouteSnapshot,
        mapped_model: Option<String>,
    ) -> Result<ProxyExecutionResponse, ProxyFailure> {
        let mut seen_ids = HashSet::new();
        let mut models = Vec::new();
        let mut attempted_count = 0_i64;
        let mut failed_count = 0_i64;
        let mut last_failure = None;
        let mut headers = HeaderMap::new();
        let mut controller = RouteAdmissionController::new(
            route_facts,
            RouteControllerSettings {
                deadline_ms: now_millis_for_services() as i64
                    + self.retry_policy.precommit_budget.as_millis() as i64,
                initial_snapshot_id: snapshot.snapshot_id.clone(),
                initial_runtime_overlay_revision: snapshot.runtime_overlay_revision,
                initial_durable_generation: snapshot.durable_generation,
                fallback_policy: FallbackPolicy {
                    has_stable_idempotency_key: true,
                    non_idempotent: false,
                },
            },
            0,
        );

        for attempt_index in 0..self.retry_policy.max_candidate_attempts {
            let decision = match controller.next(ControllerPlanningInput {
                candidates: &snapshot.candidates,
                affinity_station_key_id: None,
                profiles: &snapshot.profiles,
                capacity: &self.capacity,
                current_runtime_overlay_revision: snapshot.runtime_overlay_revision,
                now_ms: now_millis_for_services() as i64,
                max_waiters_per_constraint: 0,
            }) {
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
            let ControllerDecision::Selected(selected) = decision else {
                break;
            };
            let candidate = selected.candidate.clone();
            attempted_count = attempted_count.max(attempt_index as i64 + 1);
            let attempt_started_at_ms = now_millis_for_services() as i64;
            let target = match self
                .resolve_selected_target(selected, &snapshot.targets)
                .await
            {
                Ok(target) => target,
                Err(mut failure) => {
                    attach_failure_candidate(&mut failure, &candidate);
                    failed_count += 1;
                    last_failure = Some(failure);
                    controller
                        .record_actual_terminal_for_station_key(
                            candidate.station_key_id.clone(),
                            ActualAttemptTerminal::FailedBeforeCommit,
                        )
                        .map_err(|failure| {
                            controller_failure(failure, &RoutingPolicy::PriorityFallback)
                        })?;
                    continue;
                }
            };
            match self
                .attempts
                .attempt(&request, &target, mapped_model.as_deref())
                .await
            {
                Ok(prepared) => {
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
                                ))
                                .await?;
                                controller
                                    .record_actual_terminal_for_station_key(
                                        candidate.station_key_id.clone(),
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
                                self.finish_attempt(failed_attempt_record(
                                    &request.request_id,
                                    attempt_index as u16,
                                    &candidate,
                                    &failure,
                                    RetryDecision::Stop,
                                    attempt_started_at_ms,
                                    true,
                                ))
                                .await?;
                                failed_count += 1;
                                last_failure = Some(failure);
                                controller
                                    .record_actual_terminal_for_station_key(
                                        candidate.station_key_id.clone(),
                                        ActualAttemptTerminal::FailedBeforeCommit,
                                    )
                                    .map_err(|failure| {
                                        controller_failure(
                                            failure,
                                            &RoutingPolicy::PriorityFallback,
                                        )
                                    })?;
                            }
                        },
                        ProxyExecutionBody::Stream(_) => {
                            let failure =
                                internal_failure("model list upstream returned a stream response");
                            self.finish_attempt(failed_attempt_record(
                                &request.request_id,
                                attempt_index as u16,
                                &candidate,
                                &failure,
                                RetryDecision::Stop,
                                attempt_started_at_ms,
                                false,
                            ))
                            .await?;
                            failed_count += 1;
                            last_failure = Some(failure);
                            controller
                                .record_actual_terminal_for_station_key(
                                    candidate.station_key_id.clone(),
                                    ActualAttemptTerminal::FailedBeforeCommit,
                                )
                                .map_err(|failure| {
                                    controller_failure(failure, &RoutingPolicy::PriorityFallback)
                                })?;
                        }
                    }
                }
                Err(mut failure) => {
                    attach_failure_candidate(&mut failure, &candidate);
                    let decision = self.retry_policy.decide(&failure, true, false);
                    self.finish_attempt(failed_attempt_record(
                        &request.request_id,
                        attempt_index as u16,
                        &candidate,
                        &failure,
                        decision,
                        attempt_started_at_ms,
                        false,
                    ))
                    .await?;
                    failed_count += 1;
                    last_failure = Some(failure);
                    controller
                        .record_actual_terminal_for_station_key(
                            candidate.station_key_id.clone(),
                            ActualAttemptTerminal::FailedBeforeCommit,
                        )
                        .map_err(|failure| {
                            controller_failure(failure, &RoutingPolicy::PriorityFallback)
                        })?;
                }
            }
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
}

fn success_attempt_record(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    started_at_ms: i64,
    output_committed: bool,
) -> AttemptTerminalRecord {
    AttemptTerminalRecord {
        context: attempt_context(request_id, ordinal, candidate, started_at_ms),
        terminal: AttemptTerminal::Succeeded,
        output_committed,
        terminal_at_ms: now_millis_for_services() as i64,
    }
}

fn failed_attempt_record(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    failure: &ProxyFailure,
    decision: RetryDecision,
    started_at_ms: i64,
    output_committed: bool,
) -> AttemptTerminalRecord {
    AttemptTerminalRecord {
        context: attempt_context(request_id, ordinal, candidate, started_at_ms),
        terminal: AttemptTerminal::Failed(classified_attempt_failure(failure, decision)),
        output_committed,
        terminal_at_ms: now_millis_for_services() as i64,
    }
}

fn attempt_context(
    request_id: &str,
    ordinal: u16,
    candidate: &RoutePlanCandidate,
    started_at_ms: i64,
) -> AttemptContext {
    AttemptContext {
        attempt_id: AttemptId::new(request_id, ordinal),
        station_id: candidate.station_id.clone(),
        station_key_id: candidate.station_key_id.clone(),
        endpoint_revision: candidate.endpoint_revision,
        started_at_ms,
    }
}

fn classified_attempt_failure(
    failure: &ProxyFailure,
    decision: RetryDecision,
) -> ClassifiedAttemptFailure {
    ClassifiedAttemptFailure {
        kind: attempt_failure_kind(failure),
        blame: failure_blame(failure.source),
        retry: match decision {
            RetryDecision::NextCandidate => RetryDisposition::TryNextCandidate,
            RetryDecision::Stop => RetryDisposition::StopRequest,
        },
        health: health_effect(failure),
        public_code: failure.code.as_str().to_string(),
        sanitized_detail: Some(crate::services::secrets::mask::redact_text(
            &failure.public_message,
        )),
    }
}

fn attempt_failure_kind(failure: &ProxyFailure) -> AttemptFailureKind {
    match failure.code {
        ProxyFailureCode::UpstreamConnectFailed => AttemptFailureKind::Connect,
        ProxyFailureCode::RouteWaitTimeout | ProxyFailureCode::UpstreamFirstByteTimeout => {
            AttemptFailureKind::Timeout
        }
        ProxyFailureCode::UpstreamStreamFailed => AttemptFailureKind::StreamInterrupted,
        ProxyFailureCode::ResponsesChatFallbackIncompatible => {
            AttemptFailureKind::MalformedResponse
        }
        ProxyFailureCode::UpstreamMalformedResponse => AttemptFailureKind::MalformedResponse,
        ProxyFailureCode::UpstreamAuthenticationFailed => AttemptFailureKind::Authentication,
        ProxyFailureCode::UpstreamInsufficientBalance => AttemptFailureKind::Balance,
        ProxyFailureCode::UpstreamRateLimited => AttemptFailureKind::RateLimit,
        ProxyFailureCode::UpstreamModelUnavailable
        | ProxyFailureCode::UpstreamCapabilityMismatch => AttemptFailureKind::CapabilityMismatch,
        ProxyFailureCode::UpstreamUnavailable | ProxyFailureCode::UpstreamUncertain => {
            AttemptFailureKind::HttpStatus
        }
        ProxyFailureCode::UpstreamHttpError => match failure.http_status.as_u16() {
            401 | 403 => AttemptFailureKind::Authentication,
            402 => AttemptFailureKind::Balance,
            429 => AttemptFailureKind::RateLimit,
            400 | 404 | 409 | 422 => AttemptFailureKind::BadRequest,
            500..=599 => AttemptFailureKind::HttpStatus,
            _ => AttemptFailureKind::HttpStatus,
        },
        ProxyFailureCode::RouteNoCandidate => AttemptFailureKind::CapabilityMismatch,
        ProxyFailureCode::RouteConfigRequired
        | ProxyFailureCode::RoutePolicyRejected
        | ProxyFailureCode::RouteEconomicsUnavailable
        | ProxyFailureCode::RouteHealthUnavailable
        | ProxyFailureCode::RouteCandidateLimitExceeded => AttemptFailureKind::CapabilityMismatch,
        ProxyFailureCode::RouteCapacityExhausted
        | ProxyFailureCode::RouteFactsUnavailable
        | ProxyFailureCode::RouteConfigUnstable
        | ProxyFailureCode::RouteLifecycleUnavailable
        | ProxyFailureCode::RouteDeadlineExceeded
        | ProxyFailureCode::RouteInvariantViolation => AttemptFailureKind::LocalAdapter,
        ProxyFailureCode::RequestBodyInvalid
        | ProxyFailureCode::RequestBodyTooLarge
        | ProxyFailureCode::RequestBodyTimeout => AttemptFailureKind::BadRequest,
        ProxyFailureCode::DownstreamDisconnected => AttemptFailureKind::DownstreamDrop,
        ProxyFailureCode::LocalProxyBusy
        | ProxyFailureCode::LocalProxyMemoryBusy
        | ProxyFailureCode::RequestHeaderTimeout
        | ProxyFailureCode::RequestHeaderTooLarge
        | ProxyFailureCode::LocalAuthMissing
        | ProxyFailureCode::LocalAuthInvalid
        | ProxyFailureCode::ApplicationUpdateInProgress
        | ProxyFailureCode::InternalProxyError => AttemptFailureKind::LocalAdapter,
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

fn health_effect(failure: &ProxyFailure) -> HealthEffect {
    if failure.source != FailureSource::Upstream {
        return HealthEffect::Neutral;
    }
    match failure.code {
        ProxyFailureCode::UpstreamAuthenticationFailed
        | ProxyFailureCode::UpstreamInsufficientBalance => return HealthEffect::HardFail,
        ProxyFailureCode::UpstreamRateLimited => {
            return HealthEffect::Cooldown {
                retry_after_ms: None,
            };
        }
        ProxyFailureCode::UpstreamModelUnavailable
        | ProxyFailureCode::UpstreamCapabilityMismatch
        | ProxyFailureCode::UpstreamMalformedResponse
        | ProxyFailureCode::UpstreamUncertain => return HealthEffect::Neutral,
        ProxyFailureCode::UpstreamUnavailable => return HealthEffect::ObserveFailure,
        _ => {}
    }
    match failure.http_status.as_u16() {
        401..=403 => HealthEffect::HardFail,
        429 => HealthEffect::Cooldown {
            retry_after_ms: None,
        },
        408 | 425 | 500..=599 => HealthEffect::ObserveFailure,
        _ => match failure.code {
            ProxyFailureCode::UpstreamConnectFailed
            | ProxyFailureCode::UpstreamFirstByteTimeout
            | ProxyFailureCode::UpstreamStreamFailed => HealthEffect::ObserveFailure,
            _ => HealthEffect::Neutral,
        },
    }
}

fn attempt_lifecycle_admission_failure(error: WriterAdmissionError) -> ProxyFailure {
    attempt_lifecycle_unavailable_failure(format!(
        "attempt lifecycle writer admission rejected: {error:?}"
    ))
}

fn attempt_lifecycle_write_failure(error: LifecycleWriteError) -> ProxyFailure {
    attempt_lifecycle_unavailable_failure(format!("attempt lifecycle write failed: {error:?}"))
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
            } => (status, headers, ProxyExecutionBody::Stream(chunks)),
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
    ) -> Self {
        let (status, headers, body) = prepared.into_parts();
        let body_bytes = match &body {
            ProxyExecutionBody::Buffered(body) => Some(body.len() as i64),
            ProxyExecutionBody::Stream(_) => None,
        };
        let selected_attempt = AttemptContext {
            attempt_id: AttemptId::new(request.request_id.clone(), fallback_count as u16),
            station_id: candidate.station_id.clone(),
            station_key_id: candidate.station_key_id.clone(),
            endpoint_revision: candidate.endpoint_revision,
            started_at_ms: timings.request_started_at_ms,
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
                    estimated_fixed_price: candidate.pricing.estimated_fixed_price,
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
            let attempt = self.pool.send_resolved(prepared, target).await?;
            match attempt {
                UpstreamAttempt::Buffered {
                    status,
                    headers,
                    body,
                } => {
                    if !status.is_success() {
                        return Err(upstream_http_failure(status));
                    }
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
                        return Err(upstream_http_failure(status));
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

impl RetryPolicy {
    fn from_limits(limits: &ProxyServerLimits) -> Self {
        Self {
            max_candidate_attempts: 3,
            precommit_budget: limits.precommit_timeout,
            first_byte_timeout: limits.upstream_first_byte_timeout,
            buffered_budget: limits.buffered_execution_timeout,
        }
    }

    #[cfg(test)]
    fn for_tests(
        max_candidate_attempts: usize,
        precommit_budget: Duration,
        first_byte_timeout: Duration,
        buffered_budget: Duration,
    ) -> Self {
        Self {
            max_candidate_attempts,
            precommit_budget,
            first_byte_timeout,
            buffered_budget,
        }
    }

    pub(crate) fn decide(
        &self,
        failure: &ProxyFailure,
        idempotent: bool,
        committed: bool,
    ) -> RetryDecision {
        if committed || failure.retry_class == RetryClass::AfterCommitStop {
            return RetryDecision::Stop;
        }
        if matches!(failure.code, ProxyFailureCode::UpstreamStreamFailed) {
            return RetryDecision::Stop;
        }
        if matches!(failure.code, ProxyFailureCode::RouteWaitTimeout) {
            return RetryDecision::Stop;
        }
        if matches!(failure.code, ProxyFailureCode::UpstreamConnectFailed) {
            return if idempotent
                || failure.internal_detail.as_deref() == Some("connection_not_established")
            {
                RetryDecision::NextCandidate
            } else {
                RetryDecision::Stop
            };
        }

        match failure.http_status.as_u16() {
            401 | 403 | 408 | 425 | 429 | 500..=599 => RetryDecision::NextCandidate,
            404 if failure.internal_detail.as_deref() == Some("capability_mismatch") => {
                RetryDecision::NextCandidate
            }
            400 | 404 | 409 | 422 => RetryDecision::Stop,
            _ if failure.retry_class == RetryClass::BeforeOutput => RetryDecision::NextCandidate,
            _ => RetryDecision::Stop,
        }
    }

    pub(crate) fn max_attempts(&self, eligible_candidates: usize) -> usize {
        eligible_candidates.min(self.max_candidate_attempts)
    }

    // Exposed only for retry-budget unit assertions.
    #[cfg(test)]
    pub(crate) fn precommit_budget(&self) -> Duration {
        self.precommit_budget
    }

    #[cfg(test)]
    pub(crate) fn buffered_budget(&self) -> Duration {
        self.buffered_budget
    }

    pub(crate) fn first_byte_timeout(&self) -> Duration {
        self.first_byte_timeout
    }

    fn remaining_precommit_budget(&self, started: Instant) -> Option<Duration> {
        self.precommit_budget.checked_sub(started.elapsed())
    }
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
            Self::Stream(_) => formatter.write_str("Stream"),
        }
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

fn precommit_timeout_failure() -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::RouteWaitTimeout,
        FailureSource::Routing,
        RetryClass::BeforeOutput,
        StatusCode::GATEWAY_TIMEOUT,
        "route precommit budget exhausted",
    )
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
            group_filter_mode: group_filter_mode(&settings.routing_group_filter),
            required_group_stable_key: required_group_stable_key(&settings.routing_group_filter),
            preferred_models: Vec::new(),
            required_tags: Vec::new(),
            allow_depleted_fallback: settings.allow_depleted_fallback,
            affinity_enabled: false,
        },
        admitted_at_ms,
    )
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
        crate::models::routing::RoutingGroupFilter::AllGroups
        | crate::models::routing::RoutingGroupFilter::UngroupedOnly => GroupFilterMode::Any,
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
        } => {
            failure.context_mut().candidate_id = Some(station_key_id);
        }
    }
    failure
}

fn controller_failure(failure: ControllerFailure, policy: &RoutingPolicy) -> ProxyFailure {
    let (code, status, message) = match failure.kind {
        ControllerFailureKind::NoEligible => (
            ProxyFailureCode::RouteNoCandidate,
            StatusCode::SERVICE_UNAVAILABLE,
            "no eligible route candidate",
        ),
        ControllerFailureKind::TemporaryHealth => (
            ProxyFailureCode::RouteHealthUnavailable,
            StatusCode::SERVICE_UNAVAILABLE,
            "route health unavailable",
        ),
        ControllerFailureKind::CapacityExhausted => (
            ProxyFailureCode::RouteCapacityExhausted,
            StatusCode::SERVICE_UNAVAILABLE,
            "route capacity exhausted",
        ),
        ControllerFailureKind::Deadline => (
            ProxyFailureCode::RouteDeadlineExceeded,
            StatusCode::GATEWAY_TIMEOUT,
            "route deadline exceeded",
        ),
        ControllerFailureKind::ConfigUnstable => (
            ProxyFailureCode::RouteConfigUnstable,
            StatusCode::SERVICE_UNAVAILABLE,
            "route configuration changed during planning",
        ),
        ControllerFailureKind::CandidateLimit => (
            ProxyFailureCode::RouteCandidateLimitExceeded,
            StatusCode::SERVICE_UNAVAILABLE,
            "route candidate limit exceeded",
        ),
        ControllerFailureKind::AttemptLimit => (
            ProxyFailureCode::RouteNoCandidate,
            StatusCode::BAD_GATEWAY,
            "all route candidates failed",
        ),
        ControllerFailureKind::CommitUncertain => (
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

fn catalog_planning_exhausted(failure: &ControllerFailure) -> bool {
    matches!(
        failure.kind,
        ControllerFailureKind::NoEligible | ControllerFailureKind::AttemptLimit
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
        ProxyFailure::new(
            ProxyFailureCode::UpstreamHttpError,
            FailureSource::Upstream,
            RetryClass::Never,
            StatusCode::BAD_GATEWAY,
            format!("upstream chat fallback response was not JSON: {error}"),
        )
    })?;
    serde_json::to_vec(&render_responses_response(value, mapped_model))
        .map(Bytes::from)
        .map_err(|error| internal_failure(format!("serialize responses fallback failed: {error}")))
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

fn upstream_http_failure(status: StatusCode) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamHttpError,
        FailureSource::Upstream,
        RetryClass::BeforeOutput,
        status,
        format!("upstream HTTP {}", status.as_u16()),
    )
}

fn precommit_stream_failure(failure: ProxyFailure) -> ProxyFailure {
    upstream_first_byte_failure(format!(
        "upstream stream failed before first byte: {}",
        failure.public_message
    ))
}

fn precommit_stream_ended_failure() -> ProxyFailure {
    upstream_first_byte_failure("upstream stream ended before first byte")
}

fn upstream_first_byte_timeout_failure() -> ProxyFailure {
    upstream_first_byte_failure("upstream first byte timed out")
}

fn upstream_first_byte_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamFirstByteTimeout,
        FailureSource::Upstream,
        RetryClass::BeforeOutput,
        StatusCode::BAD_GATEWAY,
        message,
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        sync::{Arc, Mutex},
        time::Duration,
    };

    use bytes::Bytes;
    use futures_util::{future::BoxFuture, stream, StreamExt};
    use http::{HeaderMap, StatusCode};

    use crate::{
        application::{
            credentials::{SecretBytes, SecretRef},
            operational_facts::target_resolver::{
                ExecutionCredentialResolver, ExecutionTargetError, ExecutionTargetHandle,
                ExecutionTargetRef,
            },
            routing_engine::request::RouteRequestFacts,
        },
        models::{
            proxy::UpstreamApiFormat,
            routing::{RouteEndpointKind, RuntimeRoutingCandidate, StationKeyCapabilities},
        },
        services::proxy::{
            error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
            limits::{BodyBudget, RequestLease},
            request::{CanonicalProxyRequest, RequestRequirements},
            routing_repository::{
                admission_profile_from_candidate, route_projection_from_runtime,
                OperationalRouteSnapshot, RoutingRepository,
            },
        },
    };

    use super::{
        transform_stream_body, AttemptExecutor, ExecutionEngine, PreparedAttempt, RetryDecision,
        RetryPolicy,
    };
    use crate::services::proxy::protocol::DownstreamTransform;

    #[test]
    fn retry_policy_matches_the_approved_precommit_matrix() {
        let cases = [
            (failure(401), false, false, RetryDecision::NextCandidate),
            (failure(403), false, false, RetryDecision::NextCandidate),
            (
                capability_mismatch(404),
                false,
                false,
                RetryDecision::NextCandidate,
            ),
            (failure(404), false, false, RetryDecision::Stop),
            (failure(408), false, false, RetryDecision::NextCandidate),
            (failure(425), false, false, RetryDecision::NextCandidate),
            (failure(429), false, false, RetryDecision::NextCandidate),
            (failure(500), false, false, RetryDecision::NextCandidate),
            (failure(400), false, false, RetryDecision::Stop),
            (failure(409), false, false, RetryDecision::Stop),
            (failure(422), false, false, RetryDecision::Stop),
            (
                ambiguous_transport_failure(),
                false,
                false,
                RetryDecision::Stop,
            ),
            (
                ambiguous_transport_failure(),
                true,
                false,
                RetryDecision::NextCandidate,
            ),
            (stream_failure(), true, true, RetryDecision::Stop),
        ];

        for (failure, idempotent, committed, expected) in cases {
            assert_eq!(
                RetryPolicy::default().decide(&failure, idempotent, committed),
                expected,
                "failure={failure:?} idempotent={idempotent} committed={committed}"
            );
        }
    }

    #[test]
    fn retry_policy_caps_attempts_and_uses_the_approved_budgets() {
        let policy = RetryPolicy::default();

        assert_eq!(policy.max_attempts(10), 3);
        assert_eq!(policy.max_attempts(2), 2);
        assert_eq!(policy.precommit_budget(), Duration::from_secs(180));
        assert_eq!(policy.buffered_budget(), Duration::from_secs(300));
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
    async fn execution_engine_tries_at_most_three_distinct_candidates() {
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

        assert_eq!(attempts.seen_ids(), ["a", "b", "c"]);
        assert_eq!(failure.code, ProxyFailureCode::UpstreamHttpError);
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
            Duration::from_millis(20),
        ));
        let engine =
            test_engine(repository, attempts.clone()).with_retry_policy(RetryPolicy::for_tests(
                3,
                Duration::from_millis(5),
                Duration::from_secs(120),
                Duration::from_secs(300),
            ));

        let failure = engine
            .execute(canonical_chat_request().await)
            .await
            .expect_err("precommit budget exhausted");

        assert_eq!(failure.code, ProxyFailureCode::RouteWaitTimeout);
        assert_eq!(attempts.seen_ids(), ["a"]);
    }

    #[tokio::test]
    async fn execution_records_precommit_wait_in_upstream_headers_and_first_token_timing() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![rich_candidate("a")]));
        let attempts = Arc::new(FakeAttemptExecutor::delayed_responses(
            vec![Ok(stream_success(b"data: ok\n\n"))],
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
    async fn stream_bootstrap_fails_over_before_first_chunk() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(stream_error_before_data()),
            Ok(stream_success(b"data: ok\n\n")),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let mut response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("fallback stream response");

        assert_eq!(attempts.seen_ids(), ["a", "b"]);
        assert_eq!(response.selected_station_key_id(), Some("b"));
        assert_eq!(response.fallback_count(), 1);
        let super::ProxyExecutionBody::Stream(chunks) = &mut response.body else {
            panic!("expected stream body");
        };
        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"data: ok\n\n")
        );
    }

    #[tokio::test]
    async fn committed_stream_error_never_selects_another_candidate() {
        let repository = Arc::new(FakeRepository::with_candidates(vec![
            rich_candidate("a"),
            rich_candidate("b"),
        ]));
        let attempts = Arc::new(FakeAttemptExecutor::responses(vec![
            Ok(stream_then_error(b"data: first\n\n")),
            Ok(stream_success(b"data: forbidden\n\n")),
        ]));
        let engine = test_engine(repository, attempts.clone());

        let mut response = engine
            .execute(streaming_chat_request().await)
            .await
            .expect("committed stream response");

        assert_eq!(response.selected_station_key_id(), Some("a"));
        assert_eq!(attempts.seen_ids(), ["a"]);
        let super::ProxyExecutionBody::Stream(chunks) = &mut response.body else {
            panic!("expected stream body");
        };
        assert_eq!(
            chunks.next().await.unwrap().unwrap(),
            Bytes::from_static(b"data: first\n\n")
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

    struct FakeRepository {
        candidates: Vec<RuntimeRoutingCandidate>,
    }

    impl FakeRepository {
        fn with_candidates(candidates: Vec<RuntimeRoutingCandidate>) -> Self {
            Self { candidates }
        }
    }

    impl RoutingRepository for FakeRepository {
        fn load_operational_route_snapshot(
            &self,
            request: RouteRequestFacts,
        ) -> BoxFuture<'static, Result<OperationalRouteSnapshot, String>> {
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
                    projections.push(route_projection_from_runtime(&request, candidate)?);
                }
                Ok(OperationalRouteSnapshot {
                    candidates: projections,
                    targets,
                    profiles,
                    snapshot_id: "test-operational-snapshot".to_string(),
                    runtime_overlay_revision: 1,
                    durable_generation: 1,
                })
            })
        }
    }

    struct FakeAttemptExecutor {
        responses: Mutex<Vec<Result<PreparedAttempt, ProxyFailure>>>,
        seen_ids: Mutex<Vec<String>>,
        delay: Option<Duration>,
    }

    impl FakeAttemptExecutor {
        fn responses(responses: Vec<Result<PreparedAttempt, ProxyFailure>>) -> Self {
            Self {
                responses: Mutex::new(responses),
                seen_ids: Mutex::new(Vec::new()),
                delay: None,
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
            }
        }

        fn seen_ids(&self) -> Vec<String> {
            self.seen_ids.lock().expect("seen lock").clone()
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
        ) -> BoxFuture<'static, Result<SecretBytes, ExecutionTargetError>> {
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
        ProxyFailure::new(
            ProxyFailureCode::UpstreamHttpError,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::from_u16(status).expect("status"),
            format!("upstream HTTP {status}"),
        )
    }

    fn capability_mismatch(status: u16) -> ProxyFailure {
        let mut failure = failure(status);
        failure.internal_detail = Some("capability_mismatch".to_string());
        failure
    }

    fn ambiguous_transport_failure() -> ProxyFailure {
        ProxyFailure::new(
            ProxyFailureCode::UpstreamConnectFailed,
            FailureSource::Upstream,
            RetryClass::BeforeOutput,
            StatusCode::BAD_GATEWAY,
            "upstream connection reset after request write",
        )
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
        }
    }

    fn stream_error_before_data() -> PreparedAttempt {
        PreparedAttempt::Stream {
            status: StatusCode::OK,
            headers: HeaderMap::new(),
            chunks: Box::pin(stream::iter(vec![Err(stream_failure())])),
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
        }
    }

    async fn canonical_chat_request() -> CanonicalProxyRequest {
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
            None,
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

    fn target_ref(candidate: &RuntimeRoutingCandidate) -> ExecutionTargetRef {
        ExecutionTargetRef {
            station_key_id: candidate.station_key_id.clone(),
            station_id: candidate.station_id.clone(),
            endpoint_revision: candidate.station_endpoint_revision,
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
        }
    }

    fn rich_candidate(id: &str) -> RuntimeRoutingCandidate {
        RuntimeRoutingCandidate {
            station_key_id: id.to_string(),
            station_id: format!("station-{id}"),
            station_endpoint_revision: 1,
            upstream_base_url: "https://upstream.example.test/v1".to_string(),
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
            api_key: None,
            api_key_secret: None,
        }
    }
}
