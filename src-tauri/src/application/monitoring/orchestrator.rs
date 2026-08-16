use crate::models::monitoring::{FailureKind, ProbeOutcome, SemanticConfidence, TriggerKind};
use futures_util::future::BoxFuture;

use super::{
    commands::MonitorExecutionRequest,
    planner::{PlanError, ProbePlan, ProbePlanner, ProbeTargetPlan},
    recorder::{
        MonitorExecutionReceipt, MonitoringRecorder, RecordedAttempt, RecordedExecutionSummary,
        RecordedTargetResult,
    },
};

pub(crate) trait MonitorClock {
    fn now_ms(&self) -> i64;
    fn advance_ms(&self, duration_ms: u64);
}

pub(crate) trait MonitorIdGenerator {
    fn next_id(&self) -> String;
}

pub(crate) trait ProbeTransport {
    fn send(&mut self, request: ProbeTransportRequest) -> BoxFuture<'_, ProbeTransportResult>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeTransportRequest {
    pub(crate) execution_id: String,
    pub(crate) station_key_id: String,
    pub(crate) model: String,
    pub(crate) model_index: u8,
    pub(crate) attempt_number: u8,
    pub(crate) deadline_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeTransportResult {
    pub(crate) outcome: ProbeOutcome,
    pub(crate) failure_kind: Option<FailureKind>,
    pub(crate) retryable: bool,
    pub(crate) retry_after_ms: Option<u64>,
    pub(crate) latency_ms: u64,
    pub(crate) http_status: Option<u16>,
    pub(crate) response_model: Option<String>,
    pub(crate) output_bytes: usize,
    pub(crate) semantic_confidence: SemanticConfidence,
    /// A closed, implementation-defined diagnostic code. It must never hold
    /// upstream response text or credentials.
    pub(crate) error_summary: Option<String>,
}

impl ProbeTransportResult {
    pub(crate) fn failure(
        failure_kind: FailureKind,
        retryable: bool,
        retry_after_ms: Option<u64>,
        latency_ms: u64,
    ) -> Self {
        Self {
            outcome: ProbeOutcome::Unavailable,
            failure_kind: Some(failure_kind),
            retryable,
            retry_after_ms,
            latency_ms,
            http_status: None,
            response_model: None,
            output_bytes: 0,
            semantic_confidence: SemanticConfidence::ProtocolValidated,
            error_summary: Some(failure_kind.as_str().to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OrchestratorError {
    Plan(PlanError),
}

pub(crate) struct MonitorOrchestrator<C, I, R, T> {
    planner: ProbePlanner,
    clock: C,
    ids: I,
    recorder: R,
    transport: T,
}

impl<C, I, R, T> MonitorOrchestrator<C, I, R, T>
where
    C: MonitorClock,
    I: MonitorIdGenerator,
    R: MonitoringRecorder,
    T: ProbeTransport,
{
    pub(crate) fn new(clock: C, ids: I, recorder: R, transport: T) -> Self {
        Self {
            planner: ProbePlanner,
            clock,
            ids,
            recorder,
            transport,
        }
    }

    pub(crate) async fn request_execution(
        &mut self,
        request: MonitorExecutionRequest,
    ) -> Result<MonitorExecutionReceipt, OrchestratorError> {
        if matches!(request.trigger_kind, TriggerKind::Manual) {
            if let Some(key) = request.manual_idempotency_key.as_deref() {
                if let Some(receipt) = self.recorder.find_manual_execution(key) {
                    return Ok(MonitorExecutionReceipt {
                        execution_id: receipt.execution_id,
                        reused_existing: true,
                    });
                }
            }
        }

        let plan = self
            .planner
            .build_plan(request.snapshot, &request.targets, request.trigger_kind)
            .map_err(OrchestratorError::Plan)?;
        let started_at_ms = self.clock.now_ms();
        let deadline_at_ms = started_at_ms + plan.schedule_policy.execution_timeout_ms as i64;
        let new_execution_id = self.ids.next_id();
        let receipt = self.recorder.begin_execution(
            new_execution_id,
            &plan,
            request.manual_idempotency_key.as_deref(),
            started_at_ms,
        );
        let execution_id = receipt.execution_id.clone();

        let mut target_results = Vec::with_capacity(plan.target_plans.len());
        for target in &plan.target_plans {
            let result = if let Some(skip_failure_kind) = target.skip_failure_kind {
                skipped_target_result(&execution_id, target, skip_failure_kind)
            } else {
                self.execute_target(&execution_id, &plan, target, deadline_at_ms)
                    .await
            };
            self.recorder.finalize_target(result.clone());
            target_results.push(result);
        }

        self.recorder
            .finalize_execution(summarize_execution(&execution_id, &target_results));
        Ok(receipt)
    }

    pub(crate) fn into_parts(self) -> (C, I, R, T) {
        (self.clock, self.ids, self.recorder, self.transport)
    }

    async fn execute_target(
        &mut self,
        execution_id: &str,
        plan: &ProbePlan,
        target: &ProbeTargetPlan,
        deadline_at_ms: i64,
    ) -> RecordedTargetResult {
        let mut attempts = Vec::new();
        let mut terminal_hard_failure = None;

        for (model_index, model_plan) in plan.model_plans.iter().enumerate() {
            if model_index > 0 {
                let last_failure = attempts
                    .last()
                    .and_then(|attempt: &RecordedAttempt| attempt.failure_kind);
                if !last_failure.is_some_and(fallback_allowed_after_failure) {
                    break;
                }
            }

            let mut semantic_verification_used = false;
            for attempt_number in 0..plan.retry_policy.max_attempts_per_model.saturating_add(1) {
                let remaining_before_attempt = deadline_at_ms - self.clock.now_ms();
                if remaining_before_attempt < plan.schedule_policy.attempt_timeout_ms as i64 {
                    terminal_hard_failure = Some(FailureKind::Timeout);
                    break;
                }

                let started_at_ms = self.clock.now_ms();
                let attempt_deadline_at_ms = deadline_at_ms.min(
                    started_at_ms.saturating_add(plan.schedule_policy.attempt_timeout_ms as i64),
                );
                let transport_result = self
                    .transport
                    .send(ProbeTransportRequest {
                        execution_id: execution_id.to_string(),
                        station_key_id: target.station_key_id.clone(),
                        model: model_plan.model.clone(),
                        model_index: model_index as u8,
                        attempt_number,
                        deadline_at_ms: attempt_deadline_at_ms,
                    })
                    .await;
                self.clock.advance_ms(transport_result.latency_ms);
                let finished_at_ms = self.clock.now_ms();
                let slow_success = transport_result.outcome == ProbeOutcome::Available
                    && transport_result.latency_ms
                        >= plan.schedule_policy.slow_latency_threshold_ms;
                let attempt = RecordedAttempt {
                    execution_id: execution_id.to_string(),
                    station_key_id: target.station_key_id.clone(),
                    model: model_plan.model.clone(),
                    model_index: model_index as u8,
                    attempt_number,
                    started_at_ms,
                    finished_at_ms,
                    outcome: if slow_success {
                        ProbeOutcome::Degraded
                    } else {
                        transport_result.outcome
                    },
                    failure_kind: if slow_success {
                        Some(FailureKind::SlowLatency)
                    } else {
                        transport_result.failure_kind
                    },
                    retryable: transport_result.retryable,
                    http_status: transport_result.http_status,
                    response_model: transport_result.response_model.clone(),
                    output_bytes: transport_result.output_bytes,
                    semantic_confidence: transport_result.semantic_confidence,
                    error_summary: if slow_success {
                        Some(FailureKind::SlowLatency.as_str().to_string())
                    } else {
                        transport_result.error_summary.clone()
                    },
                };
                self.recorder.append_attempt(attempt.clone());
                attempts.push(attempt);

                if transport_result.outcome.is_route_available() {
                    return target_result_from_attempts(execution_id, target, &attempts);
                }
                let semantic_verification = transport_result.failure_kind.is_some_and(|failure| {
                    matches!(
                        failure,
                        FailureKind::EmptyResponse | FailureKind::ContentMismatch
                    )
                }) && !semantic_verification_used;
                let configured_retry = transport_result.retryable
                    && transport_result
                        .failure_kind
                        .is_some_and(retry_allowed_for_failure)
                    && attempt_number + 1 < plan.retry_policy.max_attempts_per_model;
                if !semantic_verification && !configured_retry {
                    break;
                }
                semantic_verification_used |= semantic_verification;

                let delay_ms = transport_result.retry_after_ms.unwrap_or_else(|| {
                    exponential_delay(
                        plan.retry_policy.base_delay_ms,
                        plan.retry_policy.max_delay_ms,
                        attempt_number,
                    )
                });
                if self.clock.now_ms()
                    + delay_ms as i64
                    + plan.schedule_policy.attempt_timeout_ms as i64
                    > deadline_at_ms
                {
                    terminal_hard_failure = Some(FailureKind::Timeout);
                    break;
                }
                self.clock.advance_ms(delay_ms);
            }

            if terminal_hard_failure.is_some() {
                break;
            }
        }

        if attempts.is_empty() {
            return skipped_target_result(
                execution_id,
                target,
                terminal_hard_failure.unwrap_or(FailureKind::Timeout),
            );
        }
        target_result_from_attempts(execution_id, target, &attempts)
    }
}

fn retry_allowed_for_failure(failure_kind: FailureKind) -> bool {
    matches!(
        failure_kind,
        FailureKind::RateLimit
            | FailureKind::ServerError
            | FailureKind::Network
            | FailureKind::Timeout
    )
}

fn skipped_target_result(
    execution_id: &str,
    target: &ProbeTargetPlan,
    failure_kind: FailureKind,
) -> RecordedTargetResult {
    RecordedTargetResult {
        execution_id: execution_id.to_string(),
        station_id: target.station_id.clone(),
        station_key_id: target.station_key_id.clone(),
        terminal_outcome: ProbeOutcome::Skipped,
        terminal_failure_kind: Some(failure_kind),
        decisive_attempt_id: None,
        requested_model: None,
        effective_model: None,
        used_fallback: false,
        attempt_count: 0,
        protocol_kind: target.protocol_kind,
        request_profile_hash: target.request_profile_hash.clone(),
        endpoint_revision: target.endpoint_revision,
    }
}

fn fallback_allowed_after_failure(failure_kind: FailureKind) -> bool {
    matches!(
        failure_kind,
        FailureKind::ServerError | FailureKind::Network | FailureKind::Timeout
    )
}

fn exponential_delay(base_ms: u64, max_ms: u64, attempt_number: u8) -> u64 {
    let factor = 1_u64 << u32::from(attempt_number.min(10));
    base_ms.saturating_mul(factor).min(max_ms)
}

fn target_result_from_attempts(
    execution_id: &str,
    target: &ProbeTargetPlan,
    attempts: &[RecordedAttempt],
) -> RecordedTargetResult {
    let decisive = attempts
        .iter()
        .find(|attempt| attempt.outcome.is_route_available())
        .unwrap_or_else(|| attempts.last().expect("attempts is non-empty"));
    let used_fallback = attempts.iter().any(|attempt| attempt.model_index > 0);
    let recovered = decisive.outcome.is_route_available()
        && (used_fallback || decisive.attempt_number > 0 || attempts.len() > 1);
    RecordedTargetResult {
        execution_id: execution_id.to_string(),
        station_id: target.station_id.clone(),
        station_key_id: target.station_key_id.clone(),
        terminal_outcome: if recovered {
            ProbeOutcome::Degraded
        } else {
            decisive.outcome
        },
        terminal_failure_kind: if recovered {
            Some(FailureKind::RecoveredAfterRetry)
        } else {
            decisive.failure_kind
        },
        decisive_attempt_id: Some(format!(
            "{}:{}:{}:{}",
            execution_id, target.station_key_id, decisive.model_index, decisive.attempt_number
        )),
        requested_model: attempts.first().map(|attempt| attempt.model.clone()),
        effective_model: Some(decisive.model.clone()),
        used_fallback,
        attempt_count: attempts.len() as u32,
        protocol_kind: target.protocol_kind,
        request_profile_hash: target.request_profile_hash.clone(),
        endpoint_revision: target.endpoint_revision,
    }
}

fn summarize_execution(
    execution_id: &str,
    target_results: &[RecordedTargetResult],
) -> RecordedExecutionSummary {
    let available_count = target_results
        .iter()
        .filter(|target| target.terminal_outcome == ProbeOutcome::Available)
        .count() as u32;
    let degraded_count = target_results
        .iter()
        .filter(|target| target.terminal_outcome == ProbeOutcome::Degraded)
        .count() as u32;
    let unavailable_count = target_results
        .iter()
        .filter(|target| target.terminal_outcome == ProbeOutcome::Unavailable)
        .count() as u32;
    let skipped_count = target_results
        .iter()
        .filter(|target| target.terminal_outcome == ProbeOutcome::Skipped)
        .count() as u32;
    let summary_outcome = if available_count > 0 && degraded_count == 0 && unavailable_count == 0 {
        ProbeOutcome::Available
    } else if available_count > 0 || degraded_count > 0 {
        ProbeOutcome::Degraded
    } else if unavailable_count > 0 {
        ProbeOutcome::Unavailable
    } else {
        ProbeOutcome::Skipped
    };
    RecordedExecutionSummary {
        execution_id: execution_id.to_string(),
        target_count: target_results.len() as u32,
        available_count,
        degraded_count,
        unavailable_count,
        skipped_count,
        summary_outcome,
    }
}
