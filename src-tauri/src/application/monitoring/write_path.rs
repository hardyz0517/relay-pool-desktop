#[cfg(test)]
use crate::application::error_rate_protection::ErrorRateProtectionService;

use crate::{
    application::health_transitions::HealthTransitionService,
    application::observation_ingestion::ObservationIngestion,
    application::spendability::{sample_disposition, SampleExclusionReason, TechnicalHealthEffect},
    models::{
        health::{
            HealthObservation, HealthObservationOutcome, HealthObservationSource,
            HealthWritebackMode as ObservationWritebackMode, TrafficEquivalence,
        },
        monitoring::{
            ClientProfileId, FailureKind, HealthWritebackMode, ProbeOutcome, SemanticConfidence,
            TriggerKind,
        },
        routing_observation::{
            FailureAttribution, ObservationOrder, ObservationOutcome, ObservationRetryDisposition,
            ObservationScope, ObservationSource, RecoveryOrigin, ResponseOrigin,
            RoutingObservation, TrafficEquivalence as RoutingTrafficEquivalence,
        },
    },
    persistence::{
        error::PersistenceError,
        stores::monitoring::executions::{
            ExecutionSummaryRow, FinalizeTargetRow, MonitoringExecutionRepository, NewAttemptRow,
            NewExecutionRow,
        },
        stores::monitoring::retention::MonitoringRetentionRepository,
        WriteSession,
    },
};

use super::{
    planner::{ProbeModelRole, ProbePlan, ProbeTargetPlan},
    recorder::{BufferedExecution, RecordedAttempt, RecordedTargetResult},
};

#[derive(Clone, Debug, Default)]
pub(crate) struct MonitoringExecutionCommitter {
    executions: MonitoringExecutionRepository,
    health: HealthTransitionService,
    observations: ObservationIngestion,
    retention: MonitoringRetentionRepository,
}

impl MonitoringExecutionCommitter {
    pub(crate) fn new() -> Self {
        Self {
            executions: MonitoringExecutionRepository,
            health: HealthTransitionService::new(),
            observations: ObservationIngestion::new(),
            retention: MonitoringRetentionRepository,
        }
    }

    #[cfg(test)]
    pub(crate) fn new_with_error_rate(error_rate: ErrorRateProtectionService) -> Self {
        Self {
            executions: MonitoringExecutionRepository,
            health: HealthTransitionService::new(),
            observations: ObservationIngestion::with_error_rate(error_rate),
            retention: MonitoringRetentionRepository,
        }
    }

    pub(crate) async fn commit(
        &self,
        write: &mut WriteSession,
        execution: &BufferedExecution,
    ) -> Result<ExecutionSummaryRow, PersistenceError> {
        let summary = execution
            .summary
            .as_ref()
            .ok_or(PersistenceError::ConstraintViolation)?;
        let finished_at_ms = execution
            .targets
            .iter()
            .filter_map(|target| target_finished_at(execution, target))
            .max()
            .unwrap_or(execution.started_at_ms);

        self.executions
            .insert_execution(
                write.connection(),
                &NewExecutionRow {
                    id: execution.execution_id.clone(),
                    monitor_id: execution.plan.monitor_id.clone(),
                    trigger_kind: execution.plan.trigger_kind.as_str().to_string(),
                    trigger_request_id: execution.manual_idempotency_key.clone(),
                    status: "running".to_string(),
                    planned_at_ms: execution.started_at_ms,
                    started_at_ms: Some(execution.started_at_ms),
                    config_revision: execution.plan.revision.0 as i64,
                    config_snapshot_hash: execution.plan.config_snapshot_hash.clone(),
                    endpoint_revision: execution_endpoint_revision(&execution.plan)?,
                    target_count: i64::from(summary.target_count),
                    created_at_ms: execution.started_at_ms,
                },
            )
            .await?;

        for attempt in &execution.attempts {
            self.executions
                .append_attempt(write.connection(), &attempt_row(&execution.plan, attempt)?)
                .await?;
        }

        for target in &execution.targets {
            let target_row = target_row(execution, target)?;
            self.executions
                .finalize_target(write.connection(), &target_row)
                .await?;

            let observation = health_observation(execution, target, &target_row);
            self.health
                .record_observation(write, observation.clone())
                .await?;
            self.observations
                .append(
                    write,
                    routing_observation_from_health(
                        &observation,
                        target_plan(&execution.plan, &target.station_key_id)?,
                        &target_row,
                        monitor_sequence(&execution.execution_id, &target_row.id),
                        monitor_producer_id(&execution.execution_id),
                    ),
                )
                .await?;
            self.retention
                .mark_dirty_range(
                    write.connection(),
                    &format!("dirty:{}", target_row.id),
                    &execution.plan.monitor_id,
                    Some(&target.station_key_id),
                    target_row.started_at_ms,
                    target_row
                        .finished_at_ms
                        .unwrap_or(target_row.started_at_ms)
                        .saturating_add(1),
                    "target_result_committed",
                    target_row.created_at_ms,
                )
                .await?;
        }

        self.executions
            .finalize_execution_and_advance_schedule(
                write.connection(),
                &execution.execution_id,
                &execution.plan.monitor_id,
                finished_at_ms,
                next_due_at_ms(execution),
            )
            .await
    }
}

fn monitor_sequence(execution_id: &str, target_id: &str) -> u64 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"relay-pool:monitor-observation-sequence:v1:");
    hasher.update(execution_id.as_bytes());
    hasher.update(b":");
    hasher.update(target_id.as_bytes());
    let digest = hasher.finalize();
    // SQLite INTEGER is a signed 64-bit value; keep the deterministic hash
    // sequence in the non-negative range used by the persistence contract.
    (u64::from_be_bytes(digest[..8].try_into().expect("hash width")) & (i64::MAX as u64)).max(1)
}

fn monitor_producer_id(execution_id: &str) -> String {
    format!("monitoring_execution:{execution_id}")
}

fn routing_observation_from_health(
    observation: &HealthObservation,
    target_plan: &ProbeTargetPlan,
    target_result: &FinalizeTargetRow,
    producer_sequence: u64,
    producer_id: String,
) -> RoutingObservation {
    let outcome = match observation.outcome {
        HealthObservationOutcome::Success => ObservationOutcome::Success,
        HealthObservationOutcome::Cooldown => ObservationOutcome::RateLimited,
        HealthObservationOutcome::HardFail => ObservationOutcome::CredentialFailure,
        HealthObservationOutcome::ObserveFailure => ObservationOutcome::EndpointFailure,
        HealthObservationOutcome::Skipped => ObservationOutcome::Cancelled,
        HealthObservationOutcome::Neutral => ObservationOutcome::Unknown,
    };
    let comparability_key = probe_comparability_key(target_plan, target_result);
    let (response_origin, failure_attribution) = if matches!(
        observation.outcome,
        HealthObservationOutcome::Skipped | HealthObservationOutcome::Neutral
    ) {
        (ResponseOrigin::Relay, FailureAttribution::Local)
    } else {
        (ResponseOrigin::Upstream, FailureAttribution::Key)
    };
    RoutingObservation {
        id: format!("routing-monitor-observation-{}", observation.id),
        order: ObservationOrder {
            producer_id,
            producer_sequence,
            event_at_ms: observation.observed_at_ms.max(0),
            ingested_at_ms: observation.observed_at_ms.max(0),
        },
        scope: ObservationScope {
            station_id: Some(target_plan.station_id.clone()),
            station_key_id: Some(observation.station_key_id.clone()),
            model: target_result
                .effective_model
                .clone()
                .or_else(|| Some(target_result.requested_model.clone())),
            endpoint_revision: Some(observation.endpoint_revision),
        },
        source: ObservationSource::ActiveProbe,
        traffic_equivalence: match (&observation.traffic_equivalence, &comparability_key) {
            (TrafficEquivalence::SyntheticStandard, Some(_)) => {
                RoutingTrafficEquivalence::SameModelShape
            }
            _ => RoutingTrafficEquivalence::EndpointOnly,
        },
        outcome,
        latency_ms: observation
            .latency_ms
            .and_then(|value| u32::try_from(value).ok()),
        evidence_mass_basis_points: 5_000,
        comparability_key,
        correlation_id: observation.id.clone(),
        attempt_index: 0,
        station_key_lifecycle_revision: target_plan.station_key_lifecycle_revision,
        cluster_finalized: true,
        cluster_expected_attempt_count: 1,
        boundary_crossed: true,
        event_time_status: crate::models::routing_observation::EventTimeStatus::Valid,
        response_origin,
        failure_code: observation.failure_kind.clone(),
        failure_attribution,
        recovery_origin: RecoveryOrigin::Normal,
        retry_disposition: ObservationRetryDisposition::End,
        probe_scope: None,
        probe_state_revision: None,
    }
}

fn probe_comparability_key(
    target_plan: &ProbeTargetPlan,
    target_result: &FinalizeTargetRow,
) -> Option<String> {
    if !matches!(target_plan.client_profile.id, ClientProfileId::StandardApi) {
        return None;
    }
    let protocol = target_plan.protocol_kind?.as_str();
    let request_profile_hash = target_plan.request_profile_hash.as_deref()?;
    if target_result.requested_model.trim().is_empty()
        || target_result.protocol_kind.as_deref() != Some(protocol)
        || target_result.request_profile_hash.as_deref() != Some(request_profile_hash)
    {
        return None;
    }
    let effective_model = target_result
        .effective_model
        .as_deref()
        .unwrap_or(&target_result.requested_model)
        .trim();
    if effective_model.is_empty() {
        return None;
    }

    crate::models::routing_observation::routing_comparability_key_v1(
        protocol,
        target_plan.client_profile.id.as_str(),
        target_plan.client_profile.version,
        effective_model,
        request_profile_hash,
    )
}

fn attempt_row(
    plan: &ProbePlan,
    attempt: &RecordedAttempt,
) -> Result<NewAttemptRow, PersistenceError> {
    let target = target_plan(plan, &attempt.station_key_id)?;
    let model = plan
        .model_plans
        .get(attempt.model_index as usize)
        .ok_or(PersistenceError::ConstraintViolation)?;
    let protocol_kind = target
        .protocol_kind
        .ok_or(PersistenceError::ConstraintViolation)?;
    let request_profile_hash = target
        .request_profile_hash
        .clone()
        .ok_or(PersistenceError::ConstraintViolation)?;
    Ok(NewAttemptRow {
        id: attempt_id(&attempt.execution_id, attempt),
        execution_id: attempt.execution_id.clone(),
        monitor_id: plan.monitor_id.clone(),
        station_id: target.station_id.clone(),
        station_key_id: attempt.station_key_id.clone(),
        model: attempt.model.clone(),
        model_role: model_role(model.role).to_string(),
        model_index: i64::from(attempt.model_index),
        attempt_number: i64::from(attempt.attempt_number),
        protocol_kind: protocol_kind.as_str().to_string(),
        client_profile_id: target.client_profile.id.as_str().to_string(),
        client_profile_version: i64::from(target.client_profile.version),
        request_profile_hash,
        transport_mode: "warm".to_string(),
        started_at_ms: attempt.started_at_ms,
        finished_at_ms: Some(attempt.finished_at_ms),
        latency_ms: Some((attempt.finished_at_ms - attempt.started_at_ms).max(0)),
        ttfb_ms: attempt.ttfb_ms.and_then(|value| i64::try_from(value).ok()),
        first_content_ms: attempt
            .first_content_ms
            .and_then(|value| i64::try_from(value).ok()),
        http_status: attempt.http_status.map(i64::from),
        outcome: attempt.outcome.as_str().to_string(),
        failure_kind: attempt
            .failure_kind
            .map(|failure_kind| failure_kind.as_str().to_string()),
        retryable: attempt.retryable,
        response_model: attempt.response_model.clone(),
        content_extracted: attempt.output_bytes > 0,
        validation_passed: attempt.outcome.is_route_available(),
        output_bytes: i64::try_from(attempt.output_bytes)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
        error_summary: attempt.error_summary.clone().or_else(|| {
            attempt
                .failure_kind
                .map(|failure_kind| failure_kind.as_str().to_string())
        }),
        canonical_failure_class: attempt
            .failure_kind
            .map(|failure_kind| failure_kind.as_str().to_string()),
        failure_origin: attempt.failure_kind.map(|failure_kind| {
            if matches!(failure_kind, FailureKind::BudgetExceeded) {
                "business_or_local_budget".to_string()
            } else {
                "probe".to_string()
            }
        }),
        failure_scope_kind: attempt.failure_kind.and_then(|failure_kind| {
            matches!(failure_kind, FailureKind::BudgetExceeded).then(|| "station_key".to_string())
        }),
        failure_dimension: attempt.failure_kind.and_then(|failure_kind| {
            matches!(failure_kind, FailureKind::BudgetExceeded)
                .then(|| "balance_or_quota".to_string())
        }),
        evidence_code: attempt.error_summary.clone(),
        evidence_confidence: attempt.failure_kind.and_then(|failure_kind| {
            matches!(failure_kind, FailureKind::BudgetExceeded).then_some("confirmed".to_string())
        }),
        classifier_profile_version: attempt
            .failure_kind
            .map(|_| "monitoring-provider-error-v1".to_string()),
        created_at_ms: attempt.started_at_ms,
    })
}

fn target_row(
    execution: &BufferedExecution,
    target: &RecordedTargetResult,
) -> Result<FinalizeTargetRow, PersistenceError> {
    let target_plan = target_plan(&execution.plan, &target.station_key_id)?;
    let decisive_attempt = target.decisive_attempt_id.as_ref().and_then(|id| {
        execution
            .attempts
            .iter()
            .find(|attempt| attempt_id(&execution.execution_id, attempt) == *id)
    });
    let semantic_confidence = decisive_attempt
        .map(|attempt| attempt.semantic_confidence)
        .unwrap_or(SemanticConfidence::ProtocolValidated);
    let latency_ms =
        decisive_attempt.map(|attempt| (attempt.finished_at_ms - attempt.started_at_ms).max(0));
    let ttfb_ms = decisive_attempt
        .and_then(|attempt| attempt.ttfb_ms)
        .and_then(|value| i64::try_from(value).ok());
    let first_content_ms = decisive_attempt
        .and_then(|attempt| attempt.first_content_ms)
        .and_then(|value| i64::try_from(value).ok());
    let started_at_ms = execution
        .attempts
        .iter()
        .filter(|attempt| attempt.station_key_id == target.station_key_id)
        .map(|attempt| attempt.started_at_ms)
        .min()
        .unwrap_or(execution.started_at_ms);
    let finished_at_ms = target_finished_at(execution, target).or(Some(started_at_ms));
    let disposition = sample_disposition(
        target
            .terminal_failure_kind
            .map(|failure_kind| failure_kind.as_str()),
    );
    Ok(FinalizeTargetRow {
        id: target_id(target),
        execution_id: execution.execution_id.clone(),
        monitor_id: execution.plan.monitor_id.clone(),
        station_id: target.station_id.clone(),
        station_key_id: target.station_key_id.clone(),
        terminal_outcome: target.terminal_outcome.as_str().to_string(),
        terminal_failure_kind: target
            .terminal_failure_kind
            .map(|failure_kind| failure_kind.as_str().to_string()),
        requested_model: target
            .requested_model
            .clone()
            .unwrap_or_else(|| execution.plan.model_plans[0].model.clone()),
        effective_model: target.effective_model.clone(),
        used_fallback: target.used_fallback,
        attempt_count: i64::from(target.attempt_count),
        decisive_attempt_id: target.decisive_attempt_id.clone(),
        protocol_kind: target
            .protocol_kind
            .map(|protocol| protocol.as_str().to_string()),
        resolved_adapter_kind: target
            .protocol_kind
            .map(|protocol| protocol.as_str().to_string())
            .unwrap_or_else(|| "unresolved".to_string()),
        client_profile_id: target_plan.client_profile.id.as_str().to_string(),
        client_profile_version: i64::from(target_plan.client_profile.version),
        request_profile_hash: target.request_profile_hash.clone(),
        traffic_equivalence: target_traffic_equivalence(target_plan.client_profile.id).to_string(),
        latency_ms,
        ttfb_ms,
        first_content_ms,
        semantic_confidence: semantic_confidence.as_str().to_string(),
        availability_eligible: disposition.availability_eligible,
        latency_eligible: disposition.latency_eligible,
        exclusion_reason: disposition.exclusion_reason.map(exclusion_reason_str),
        technical_health_effect: technical_health_effect_str(disposition.health_effect).to_string(),
        disposition_profile_version: "monitoring-disposition-v1".to_string(),
        started_at_ms,
        finished_at_ms,
        created_at_ms: finished_at_ms.unwrap_or(started_at_ms),
    })
}

fn exclusion_reason_str(reason: SampleExclusionReason) -> String {
    match reason {
        SampleExclusionReason::BalanceDepleted => "balance_depleted",
        SampleExclusionReason::SubscriptionUnavailable => "subscription_unavailable",
        SampleExclusionReason::QuotaExhausted => "quota_exhausted",
        SampleExclusionReason::Cancelled => "cancelled",
        SampleExclusionReason::Interrupted => "interrupted",
        SampleExclusionReason::LocalConfiguration => "local_configuration",
        SampleExclusionReason::LocalBudget => "local_budget",
        SampleExclusionReason::LocalInternalBeforeSend => "local_internal_before_send",
    }
    .to_string()
}

fn technical_health_effect_str(effect: TechnicalHealthEffect) -> &'static str {
    match effect {
        TechnicalHealthEffect::Positive => "positive",
        TechnicalHealthEffect::Negative => "negative",
        TechnicalHealthEffect::Neutral => "neutral",
    }
}

fn health_observation(
    execution: &BufferedExecution,
    target: &RecordedTargetResult,
    target_row: &FinalizeTargetRow,
) -> HealthObservation {
    health_observation_from_parts(
        execution,
        target,
        &target_row.id,
        execution.plan.health_policy.writeback_mode,
    )
}

fn health_observation_from_parts(
    execution: &BufferedExecution,
    target: &RecordedTargetResult,
    target_result_id: &str,
    writeback_mode: HealthWritebackMode,
) -> HealthObservation {
    let disposition = sample_disposition(
        target
            .terminal_failure_kind
            .map(|failure_kind| failure_kind.as_str()),
    );
    HealthObservation {
        id: format!("obs:{target_result_id}"),
        station_key_id: target.station_key_id.clone(),
        target_result_id: Some(target_result_id.to_string()),
        source: HealthObservationSource::SyntheticMonitor,
        source_event_id: target_result_id.to_string(),
        observed_at_ms: target_finished_at(execution, target).unwrap_or(execution.started_at_ms),
        endpoint_revision: target.endpoint_revision,
        outcome: health_outcome(target.terminal_outcome, target.terminal_failure_kind),
        failure_kind: target
            .terminal_failure_kind
            .map(|failure_kind| failure_kind.as_str().to_string()),
        latency_ms: disposition
            .latency_eligible
            .then(|| {
                target
                    .decisive_attempt_id
                    .as_ref()
                    .and_then(|id| {
                        execution
                            .attempts
                            .iter()
                            .find(|attempt| attempt_id(&execution.execution_id, attempt) == *id)
                    })
                    .map(|attempt| (attempt.finished_at_ms - attempt.started_at_ms).max(0))
            })
            .flatten(),
        retry_after_ms: None,
        error_summary: target
            .terminal_failure_kind
            .map(|failure_kind| failure_kind.as_str().to_string()),
        writeback_mode: observation_writeback_mode(writeback_mode),
        traffic_equivalence: observation_traffic_equivalence(
            target_plan(&execution.plan, &target.station_key_id)
                .map(|target| target.client_profile.id)
                .unwrap_or(ClientProfileId::StandardApi),
        ),
    }
}

fn health_outcome(
    outcome: ProbeOutcome,
    failure_kind: Option<FailureKind>,
) -> HealthObservationOutcome {
    match outcome {
        ProbeOutcome::Available | ProbeOutcome::Degraded => HealthObservationOutcome::Success,
        ProbeOutcome::Skipped => HealthObservationOutcome::Skipped,
        ProbeOutcome::Unavailable => match failure_kind {
            Some(FailureKind::RateLimit) => HealthObservationOutcome::Cooldown,
            Some(FailureKind::Auth) => HealthObservationOutcome::HardFail,
            Some(FailureKind::BudgetExceeded) => HealthObservationOutcome::Neutral,
            _ => HealthObservationOutcome::ObserveFailure,
        },
    }
}

fn observation_writeback_mode(mode: HealthWritebackMode) -> ObservationWritebackMode {
    match mode {
        HealthWritebackMode::Disabled => ObservationWritebackMode::Disabled,
        HealthWritebackMode::ObserveOnly => ObservationWritebackMode::ObserveOnly,
        HealthWritebackMode::Authoritative => ObservationWritebackMode::Authoritative,
    }
}

fn target_traffic_equivalence(profile: ClientProfileId) -> &'static str {
    match profile {
        ClientProfileId::StandardApi => "standard_api",
        ClientProfileId::CodexCliCompat
        | ClientProfileId::ClaudeCodeCompat
        | ClientProfileId::GeminiCliCompat
        | ClientProfileId::GrokCliCompat => "cli_compat",
    }
}

fn observation_traffic_equivalence(profile: ClientProfileId) -> TrafficEquivalence {
    match profile {
        ClientProfileId::StandardApi => TrafficEquivalence::SyntheticStandard,
        ClientProfileId::CodexCliCompat
        | ClientProfileId::ClaudeCodeCompat
        | ClientProfileId::GeminiCliCompat
        | ClientProfileId::GrokCliCompat => TrafficEquivalence::SyntheticCliCompat,
    }
}

fn model_role(role: ProbeModelRole) -> &'static str {
    match role {
        ProbeModelRole::Primary => "primary",
        ProbeModelRole::Fallback { .. } => "fallback",
    }
}

fn attempt_id(execution_id: &str, attempt: &RecordedAttempt) -> String {
    format!(
        "{}:{}:{}:{}",
        execution_id, attempt.station_key_id, attempt.model_index, attempt.attempt_number
    )
}

fn target_id(target: &RecordedTargetResult) -> String {
    format!("target:{}:{}", target.execution_id, target.station_key_id)
}

fn target_plan<'a>(
    plan: &'a ProbePlan,
    station_key_id: &str,
) -> Result<&'a ProbeTargetPlan, PersistenceError> {
    plan.target_plans
        .iter()
        .find(|target| target.station_key_id == station_key_id)
        .ok_or(PersistenceError::ConstraintViolation)
}

fn target_finished_at(execution: &BufferedExecution, target: &RecordedTargetResult) -> Option<i64> {
    execution
        .attempts
        .iter()
        .filter(|attempt| attempt.station_key_id == target.station_key_id)
        .map(|attempt| attempt.finished_at_ms)
        .max()
}

fn execution_endpoint_revision(plan: &ProbePlan) -> Result<i64, PersistenceError> {
    plan.target_plans
        .iter()
        .map(|target| target.endpoint_revision)
        .min()
        .filter(|revision| *revision > 0)
        .ok_or(PersistenceError::ConstraintViolation)
}

fn next_due_at_ms(execution: &BufferedExecution) -> Option<i64> {
    match execution.plan.trigger_kind {
        TriggerKind::Scheduled | TriggerKind::StartupRecovery => Some(
            execution
                .started_at_ms
                .saturating_add((execution.plan.schedule_policy.interval_seconds * 1000) as i64),
        ),
        TriggerKind::Manual | TriggerKind::LegacyImport => None,
    }
}

#[cfg(test)]
mod tests {
    use super::monitor_sequence;

    #[test]
    fn monitor_sequence_fits_sqlite_integer_for_high_hash_values() {
        let sequence = monitor_sequence("manual-execution", "target:manual-execution:key-1");
        assert!(sequence > 0);
        assert!(sequence <= i64::MAX as u64);
    }
}
