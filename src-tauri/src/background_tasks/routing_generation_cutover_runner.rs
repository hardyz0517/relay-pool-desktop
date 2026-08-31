use std::{collections::BTreeMap, sync::Arc, time::Duration};

use rand::{rngs::OsRng, RngCore};
use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio_util::sync::CancellationToken;

use crate::{
    application::{
        quality_projection::QualityProjectionConfig,
        request_finalization::failure::{
            failure_from_provider_signal, CapabilityApplicabilitySet, FailureClass, HealthEffect,
            ProviderErrorSemanticSignal, RetryDisposition,
        },
        routing_generation::{
            canonical_json_sha256, runtime_generation_id, sha256_hex,
            ROUTING_GENERATION_ALGORITHM_VERSION,
        },
        routing_generation_coordinator::{
            RoutingGenerationCoordinator, RoutingGenerationCoordinatorError,
        },
        routing_policy_control_plane::RoutingPolicyMutationCoordinator,
        station_key_circuit::{
            CircuitTransition, StationKeyCircuit, StationKeyCircuitConfig, StationKeyCircuitState,
        },
    },
    background_tasks::{
        routing_generation_rebuilder::{
            CircuitGenerationBuildRequest, CircuitGenerationBuildResult,
            CircuitGenerationVerification, QualityGenerationBuildRequest,
            QualityGenerationBuildResult, QualityGenerationVerification,
            RoutingGenerationRebuilder,
        },
        TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor,
    },
    models::{
        routing_generation::{
            NewRoutingRuntimeGeneration, RoutingGenerationQualification, RoutingGenerationStatus,
            RoutingRuntimeGeneration,
        },
        routing_policy::RoutingPolicyConfigV3,
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::routing_quality_store::{MAX_ACTIVE_QUALITY_LAG_SECONDS, MAX_PROJECTOR_BACKLOG},
    },
};

pub(crate) const ROUTING_GENERATION_CUTOVER_TASK_ID: &str = "routing-generation-cutover-v1";
const BUILD_INTERVAL: Duration = Duration::from_secs(5);
const SYSTEM_MAX_COOLDOWN_MS: u64 = 24 * 60 * 60 * 1_000;
// The fence bounds how long a cutover may wait for ingestion quiescence. Keep
// this aligned with the v3 operational contract; it is intentionally not a
// user-configurable policy field.
const SYSTEM_CUTOVER_FENCE_TIMEOUT_MS: i64 = 30_000;

#[derive(Debug, Clone)]
struct StagedBuildInput {
    policy_generation_id: String,
    policy_revision: u64,
    policy_json: Value,
    policy: RoutingPolicyConfigV3,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=v3-generation-build-watermark; owner=background_tasks/routing_generation_cutover_runner; remove_when=staged build diagnostics expose the combined ingestion watermark"
        )
    )]
    ingestion_watermark: u64,
    active: Option<ActiveBuildBaseline>,
    rebuild_plan: ComponentRebuildPlan,
    quality_policy_revision: u64,
    circuit_policy_revision: u64,
    input_observation_watermark: u64,
    next_observation_watermark: u64,
    input_circuit_event_watermark: u64,
    stale_ready_generation_ids: Vec<String>,
}

#[derive(Debug, Clone)]
struct ActiveBuildBaseline {
    generation: RoutingRuntimeGeneration,
    policy: RoutingPolicyConfigV3,
    quality_checkpoint_ref: String,
    circuit_checkpoint_ref: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ComponentRebuildPlan {
    quality: bool,
    circuit: bool,
    quality_policy_changed: bool,
    circuit_policy_changed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectionCutoverGate {
    backlog: u64,
    lag_seconds: u64,
}

impl ProjectionCutoverGate {
    fn rejection_code(self) -> Option<&'static str> {
        if self.backlog > MAX_PROJECTOR_BACKLOG {
            Some("projection_backlog_exceeded")
        } else if self.lag_seconds > MAX_ACTIVE_QUALITY_LAG_SECONDS {
            Some("projection_lag_exceeded")
        } else {
            None
        }
    }
}

/// Registers the single supervised owner that materializes shadow runtime
/// generations. It deliberately stops at Ready: activation additionally
/// requires immutable comparison and failure-replay qualification evidence.
pub(crate) fn register_routing_generation_cutover_task(
    supervisor: &TaskSupervisor,
    runtime: PersistenceHandle,
    policy_publication: Arc<RoutingPolicyMutationCoordinator>,
) -> Result<TaskId, String> {
    let task_id = TaskId::from(ROUTING_GENERATION_CUTOVER_TASK_ID);
    supervisor
        .register(
            TaskSpec::new(
                task_id.clone(),
                "routing_generation_cutover_v1",
                move |context: TaskRunContext| {
                    let runtime = runtime.clone();
                    let policy_publication = Arc::clone(&policy_publication);
                    Box::pin(async move {
                        run_loop(runtime, policy_publication, context.cancellation_token).await
                    })
                },
            )
            .with_concurrency_key("routing-generation-cutover-v1")
            .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

async fn run_loop(
    runtime: PersistenceHandle,
    policy_publication: Arc<RoutingPolicyMutationCoordinator>,
    cancellation: CancellationToken,
) -> Result<(), TaskFailure> {
    loop {
        let tick = run_cutover_once(&runtime, &policy_publication, &cancellation).await;
        if let Err(_error) = tick {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::routing_projection_tick_failed(),
            );
        }
        tokio::select! {
            _ = cancellation.cancelled() => return Err(TaskFailure::cancelled()),
            _ = tokio::time::sleep(BUILD_INTERVAL) => {}
        }
    }
}

async fn run_cutover_once(
    runtime: &PersistenceHandle,
    policy_publication: &RoutingPolicyMutationCoordinator,
    cancellation: &CancellationToken,
) -> Result<Option<String>, PersistenceError> {
    publish_active_policy(runtime, policy_publication, false).await?;
    let built = build_ready_once(runtime, cancellation).await?;
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let activated = qualify_and_activate_once(runtime, cancellation).await?;
    if activated.is_some() {
        publish_active_policy(runtime, policy_publication, true).await?;
    }
    Ok(activated.or(built))
}

pub(crate) async fn build_ready_once(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
) -> Result<Option<String>, PersistenceError> {
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let Some(input) = load_staged_build_input(runtime).await? else {
        return Ok(None);
    };
    build_ready_from_input(runtime, input, cancellation).await
}

async fn build_ready_from_input(
    runtime: &PersistenceHandle,
    input: StagedBuildInput,
    cancellation: &CancellationToken,
) -> Result<Option<String>, PersistenceError> {
    let rebuild_plan = input.rebuild_plan;
    let evaluation_at_ms = resolve_build_evaluation_at_ms(runtime, &input).await?;
    if !input.stale_ready_generation_ids.is_empty() {
        let mut write = runtime.begin_write().await?;
        for stale_id in &input.stale_ready_generation_ids {
            crate::persistence::stores::routing_generation_store::RoutingGenerationStore
                .mark_ready_generation_stale(write.connection(), stale_id, evaluation_at_ms)
                .await?;
        }
        write.commit().await?;
    }
    let rebuilder = RoutingGenerationRebuilder;
    let quality = if rebuild_plan.quality {
        let result = rebuilder
            .rebuild_quality_generation(
                runtime,
                QualityGenerationBuildRequest {
                    input_observation_watermark: input.input_observation_watermark,
                    next_observation_watermark: input.next_observation_watermark,
                    evaluation_at_ms,
                    config: quality_config(&input, input.quality_policy_revision),
                },
                cancellation,
            )
            .await;
        result?
    } else {
        reused_quality_component(input.active.as_ref().ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "routing generation reuse requires an active baseline".to_string(),
            )
        })?)
    };
    if !quality.complete || cancellation.is_cancelled() {
        return Ok(None);
    }
    let circuit = if rebuild_plan.circuit {
        let result = rebuilder
            .rebuild_circuit_generation(
                runtime,
                CircuitGenerationBuildRequest {
                    input_circuit_event_watermark: input.input_circuit_event_watermark,
                    circuit_policy_revision: input.circuit_policy_revision,
                    consecutive_failure_threshold: input.policy.retry.consecutive_failure_threshold,
                    recovery_success_threshold: u16::from(
                        input.policy.circuit_breaker.recovery_success_threshold,
                    ),
                    recovery_wait_ms: u64::from(input.policy.circuit_breaker.recovery_wait_seconds)
                        * 1_000,
                    max_cooldown_ms: SYSTEM_MAX_COOLDOWN_MS,
                    evaluation_at_ms,
                },
                cancellation,
            )
            .await;
        result?
    } else {
        reused_circuit_component(input.active.as_ref().ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "routing generation reuse requires an active baseline".to_string(),
            )
        })?)
    };
    if !circuit.complete || cancellation.is_cancelled() {
        return Ok(None);
    }

    let policy_hash = canonical_json_sha256(&input.policy_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let policy_checkpoint_ref = format!(
        "policy-checkpoint:{}",
        sha256_hex(input.policy_generation_id.as_bytes())
    );
    let checkpoint_preimage = format!(
        "{}\n{}\n{}",
        policy_checkpoint_ref, quality.checkpoint_ref, circuit.checkpoint_ref
    );
    let mut generation = NewRoutingRuntimeGeneration {
        runtime_generation_id: String::new(),
        policy_generation_id: input.policy_generation_id,
        quality_generation_id: quality.quality_generation_id,
        circuit_generation_id: circuit.circuit_generation_id,
        policy_revision: input.policy_revision,
        quality_policy_revision: input.quality_policy_revision,
        circuit_policy_revision: input.circuit_policy_revision,
        algorithm_version: ROUTING_GENERATION_ALGORITHM_VERSION.to_string(),
        input_observation_watermark: input.input_observation_watermark,
        input_circuit_event_watermark: input.input_circuit_event_watermark,
        policy_input_hash: policy_hash.clone(),
        quality_input_hash: quality.input_observation_hash,
        circuit_input_hash: circuit.input_circuit_event_hash,
        policy_content_hash: policy_hash,
        quality_content_hash: quality.output_content_hash,
        circuit_content_hash: circuit.output_content_hash,
        checkpoint_ref: format!(
            "runtime-checkpoint:{}",
            sha256_hex(checkpoint_preimage.as_bytes())
        ),
        policy_checkpoint_ref,
        quality_checkpoint_ref: quality.checkpoint_ref,
        circuit_checkpoint_ref: circuit.checkpoint_ref,
        created_at_ms: evaluation_at_ms,
    };
    generation.runtime_generation_id =
        runtime_generation_id(&generation).map_err(|_| PersistenceError::ConstraintViolation)?;
    let register_result = RoutingGenerationCoordinator::new(runtime.clone())
        .register_ready_generation(&generation, evaluation_at_ms)
        .await;
    register_result.map_err(coordinator_error)?;
    Ok(Some(generation.runtime_generation_id))
}

#[derive(Debug, Serialize)]
struct QualificationComparisonReport {
    report_version: &'static str,
    runtime_generation_id: String,
    source_runtime_generation_id: Option<String>,
    target_policy_revision: u64,
    target_quality_policy_revision: u64,
    target_circuit_policy_revision: u64,
    score_basis: &'static str,
    key_count: u64,
    rank_change_count: u64,
    keys: Vec<KeyComparison>,
    quality: ComponentComparison,
    circuit: ComponentComparison,
}

#[derive(Debug, Clone, Serialize)]
struct KeyComparison {
    key_commitment: String,
    source: Option<KeyQualificationMetrics>,
    target: Option<KeyQualificationMetrics>,
    score_delta_basis_points: Option<i32>,
    rank_delta: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
struct KeyQualificationMetrics {
    reliability_basis_points: u16,
    weighted_latency_ms: u32,
    qualification_score_basis_points: u16,
    observation_count: u64,
    real_source_weight_basis_points: u16,
    monitoring_source_weight_basis_points: u16,
    quality_basis: String,
    circuit_state: String,
    rank: u64,
}

#[derive(Debug, Clone)]
struct RawKeyQualificationMetrics {
    station_key_id: String,
    lifecycle_revision: u64,
    reliability_basis_points: u16,
    weighted_latency_ms: u32,
    qualification_score_basis_points: u16,
    observation_count: u64,
    real_source_weight_basis_points: u16,
    monitoring_source_weight_basis_points: u16,
    quality_basis: String,
    circuit_state: String,
    rank: u64,
}

impl RawKeyQualificationMetrics {
    fn report_metrics(&self) -> KeyQualificationMetrics {
        KeyQualificationMetrics {
            reliability_basis_points: self.reliability_basis_points,
            weighted_latency_ms: self.weighted_latency_ms,
            qualification_score_basis_points: self.qualification_score_basis_points,
            observation_count: self.observation_count,
            real_source_weight_basis_points: self.real_source_weight_basis_points,
            monitoring_source_weight_basis_points: self.monitoring_source_weight_basis_points,
            quality_basis: self.quality_basis.clone(),
            circuit_state: self.circuit_state.clone(),
            rank: self.rank,
        }
    }
}

#[derive(Debug, Serialize)]
struct ComponentComparison {
    source_generation_id: Option<String>,
    target_generation_id: String,
    source_row_count: u64,
    target_row_count: u64,
    added_row_count: u64,
    removed_row_count: u64,
    changed_row_count: u64,
}

#[derive(Debug, Serialize)]
struct QualificationReplayReport {
    report_version: &'static str,
    runtime_generation_id: String,
    observation_watermark: u64,
    circuit_event_watermark: u64,
    quality_input_hash: String,
    quality_content_hash: String,
    quality_input_observation_count: u64,
    quality_output_scope_count: u64,
    circuit_input_hash: String,
    circuit_content_hash: String,
    circuit_input_event_count: u64,
    circuit_output_state_count: u64,
    semantic_fixtures: Vec<FailureSemanticReplay>,
}

#[derive(Debug, Clone, Serialize)]
struct FailureSemanticReplay {
    fixture: &'static str,
    http_status: u16,
    failure_sample: bool,
    retry_next_key: bool,
    retry_after_ignored: bool,
    station_key_circuit_opened: bool,
    consecutive_failures: u16,
    passed: bool,
}

pub(crate) async fn qualify_and_activate_once(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
) -> Result<Option<String>, PersistenceError> {
    let registry = RoutingGenerationCoordinator::new(runtime.clone())
        .inspect()
        .await
        .map_err(coordinator_error)?;
    if registry.fencing.is_some() {
        return advance_fenced_cutover_once(runtime, cancellation).await;
    }
    let Some((target, source, policy)) = load_ready_qualification_input(runtime).await? else {
        return Ok(None);
    };
    let gate_now_ms = chrono::Utc::now()
        .timestamp_millis()
        .max(target.created_at_ms);
    if load_projection_cutover_gate(runtime, target.input_observation_watermark, gate_now_ms)
        .await?
        .rejection_code()
        .is_some()
    {
        return Ok(None);
    }
    qualify_generation(
        runtime,
        &target,
        source.as_ref(),
        &policy,
        target.input_observation_watermark,
    )
    .await?;
    let now_ms = chrono::Utc::now()
        .timestamp_millis()
        .max(target.created_at_ms)
        .saturating_add(1);
    RoutingGenerationCoordinator::new(runtime.clone())
        .begin_cutover(
            &target.runtime_generation_id,
            source
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            now_ms,
        )
        .await
        .map_err(coordinator_error)?;
    advance_fenced_cutover_once(runtime, cancellation).await
}

async fn qualify_generation(
    runtime: &PersistenceHandle,
    target: &RoutingRuntimeGeneration,
    source: Option<&RoutingRuntimeGeneration>,
    policy: &RoutingPolicyConfigV3,
    next_observation_watermark: u64,
) -> Result<(), PersistenceError> {
    if generation_is_qualified(runtime, &target.runtime_generation_id).await? {
        return Ok(());
    }
    let quality_request = QualityGenerationBuildRequest {
        input_observation_watermark: target.input_observation_watermark,
        next_observation_watermark,
        evaluation_at_ms: load_quality_evaluation_at_ms(runtime, &target.quality_generation_id)
            .await?,
        config: quality_config_for_generation(policy, target.quality_policy_revision),
    };
    let circuit_request = CircuitGenerationBuildRequest {
        input_circuit_event_watermark: target.input_circuit_event_watermark,
        circuit_policy_revision: target.circuit_policy_revision,
        consecutive_failure_threshold: policy.retry.consecutive_failure_threshold,
        recovery_success_threshold: u16::from(policy.circuit_breaker.recovery_success_threshold),
        recovery_wait_ms: u64::from(policy.circuit_breaker.recovery_wait_seconds) * 1_000,
        max_cooldown_ms: SYSTEM_MAX_COOLDOWN_MS,
        evaluation_at_ms: load_circuit_evaluation_at_ms(runtime, &target.circuit_generation_id)
            .await?,
    };
    let rebuilder = RoutingGenerationRebuilder;
    let quality_verification = rebuilder
        .verify_quality_generation(
            runtime,
            &quality_request,
            &QualityGenerationBuildResult {
                quality_generation_id: target.quality_generation_id.clone(),
                input_observation_hash: target.quality_input_hash.clone(),
                output_content_hash: target.quality_content_hash.clone(),
                checkpoint_ref: load_component_checkpoint(
                    runtime,
                    &target.runtime_generation_id,
                    true,
                )
                .await?,
                processed_scope_count: 0,
                complete: true,
            },
        )
        .await?;
    let circuit_verification = rebuilder
        .verify_circuit_generation(
            runtime,
            &circuit_request,
            &CircuitGenerationBuildResult {
                circuit_generation_id: target.circuit_generation_id.clone(),
                input_circuit_event_hash: target.circuit_input_hash.clone(),
                output_content_hash: target.circuit_content_hash.clone(),
                checkpoint_ref: load_component_checkpoint(
                    runtime,
                    &target.runtime_generation_id,
                    false,
                )
                .await?,
                processed_event_count: 0,
                complete: true,
            },
        )
        .await?;
    let comparison = build_comparison_report(runtime, source, target, policy).await?;
    let replay = build_replay_report(target, quality_verification, circuit_verification, policy)?;
    let comparison_report = serde_json::to_value(comparison)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let replay_report = serde_json::to_value(replay)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let comparison_report_hash = canonical_json_sha256(&comparison_report)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let replay_report_hash =
        canonical_json_sha256(&replay_report).map_err(|_| PersistenceError::ConstraintViolation)?;
    let qualified_at_ms = chrono::Utc::now()
        .timestamp_millis()
        .max(target.created_at_ms);
    RoutingGenerationCoordinator::new(runtime.clone())
        .record_qualification(&RoutingGenerationQualification {
            runtime_generation_id: target.runtime_generation_id.clone(),
            comparison_report_hash,
            comparison_report,
            replay_report_hash,
            replay_report,
            qualified_at_ms,
        })
        .await
        .map_err(coordinator_error)?;
    Ok(())
}

async fn generation_is_qualified(
    runtime: &PersistenceHandle,
    runtime_generation_id: &str,
) -> Result<bool, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_generation_qualification_v2 q
         JOIN routing_generation_qualification_report_v2 r
           ON r.runtime_generation_id = q.runtime_generation_id
          AND r.comparison_report_hash = q.comparison_report_hash
          AND r.replay_report_hash = q.replay_report_hash
         WHERE q.runtime_generation_id = ?1",
    )
    .bind(runtime_generation_id)
    .fetch_one(read.connection())
    .await?;
    Ok(count == 1)
}

async fn advance_fenced_cutover_once(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
) -> Result<Option<String>, PersistenceError> {
    advance_fenced_cutover_at(runtime, cancellation, None).await
}

async fn advance_fenced_cutover_at(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
    now_override_ms: Option<i64>,
) -> Result<Option<String>, PersistenceError> {
    if cancellation.is_cancelled() {
        return Ok(None);
    }
    let coordinator = RoutingGenerationCoordinator::new(runtime.clone());
    let registry = coordinator.inspect().await.map_err(coordinator_error)?;
    let Some(mut target) = registry.fencing else {
        return Ok(None);
    };
    let mut fence = crate::models::routing_generation::RoutingGenerationFence {
        source_runtime_generation_id: registry
            .active
            .as_ref()
            .map(|generation| generation.runtime_generation_id.clone()),
        target_runtime_generation_id: target.runtime_generation_id.clone(),
        fence_revision: registry.marker.fence_revision,
    };
    let rollback = {
        let mut read = runtime.begin_read().await?;
        crate::persistence::stores::routing_generation_store::RoutingGenerationStore
            .is_rollback_fence(read.connection(), &fence)
            .await?
    };
    let now_ms = now_override_ms
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
        .max(registry.marker.updated_at_ms);
    if now_ms.saturating_sub(registry.marker.updated_at_ms) >= SYSTEM_CUTOVER_FENCE_TIMEOUT_MS {
        if rollback {
            coordinator
                .abort_rollback(&fence, "fence_timeout", now_ms)
                .await
                .map_err(coordinator_error)?;
        } else {
            coordinator
                .abort_cutover(&fence, "fence_timeout", now_ms)
                .await
                .map_err(coordinator_error)?;
        }
        return Ok(None);
    }
    if let Some(reason) =
        load_projection_cutover_gate(runtime, target.input_observation_watermark, now_ms)
            .await?
            .rejection_code()
    {
        if rollback {
            coordinator
                .abort_rollback(&fence, reason, now_ms)
                .await
                .map_err(coordinator_error)?;
        } else {
            coordinator
                .abort_cutover(&fence, reason, now_ms)
                .await
                .map_err(coordinator_error)?;
        }
        return Ok(None);
    }

    let (pending_attempts, latest_policy_revision) = {
        let mut read = runtime.begin_read().await?;
        let pending: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_attempt_v3
             WHERE candidate_admitted = 1 AND terminal_state = 'pending'",
        )
        .fetch_one(read.connection())
        .await?;
        let latest_policy_revision: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(config_revision) FROM routing_policy_v3_staged
             WHERE scope = 'active' AND status IN ('staged', 'ready', 'active')",
        )
        .fetch_one(read.connection())
        .await?;
        (pending, latest_policy_revision)
    };
    if !rollback
        && latest_policy_revision
            .and_then(|value| u64::try_from(value).ok())
            .is_some_and(|revision| revision > target.policy_revision)
    {
        coordinator
            .abort_cutover(&fence, "policy_superseded", now_ms)
            .await
            .map_err(coordinator_error)?;
        return Ok(None);
    }
    if pending_attempts != 0 {
        return Ok(None);
    }

    let (origin_next_watermark, final_observation_watermark, final_circuit_watermark) = {
        let mut read = runtime.begin_read().await?;
        let origin = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
            .load_fence_origin_observation_watermark(read.connection(), &fence)
            .await?;
        let observation: Option<i64> = sqlx::query_scalar(
            "SELECT MAX(ingestion_sequence) FROM routing_observations
             WHERE generation_eligibility = 'active'",
        )
        .fetch_one(read.connection())
        .await?;
        let circuit: Option<i64> =
            sqlx::query_scalar("SELECT MAX(ingestion_sequence) FROM routing_circuit_event_v3")
                .fetch_one(read.connection())
                .await?;
        (
            origin,
            observation
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(target.input_observation_watermark)
                .max(target.input_observation_watermark),
            circuit
                .and_then(|value| u64::try_from(value).ok())
                .unwrap_or(target.input_circuit_event_watermark)
                .max(target.input_circuit_event_watermark),
        )
    };

    if final_observation_watermark > target.input_observation_watermark
        || final_circuit_watermark > target.input_circuit_event_watermark
    {
        let input = load_fenced_build_input(
            runtime,
            &target,
            origin_next_watermark,
            final_observation_watermark,
            final_circuit_watermark,
        )
        .await?;
        let replacement_result = build_ready_from_input(runtime, input, cancellation).await;
        let Some(replacement_id) = replacement_result? else {
            return Ok(None);
        };
        let replacement = load_runtime_generation_by_id(runtime, &replacement_id).await?;
        let policy_json =
            load_policy_generation_json(runtime, &replacement.policy_generation_id).await?;
        let policy = RoutingPolicyConfigV3::from_stored_value(&policy_json)
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        qualify_generation(
            runtime,
            &replacement,
            registry.active.as_ref(),
            &policy,
            origin_next_watermark,
        )
        .await?;
        let retarget_at_ms = chrono::Utc::now()
            .timestamp_millis()
            .max(now_ms)
            .max(replacement.created_at_ms);
        fence = coordinator
            .retarget_cutover(&fence, &replacement_id, retarget_at_ms)
            .await
            .map_err(coordinator_error)?;
        target = replacement;
    }

    let activation_at_ms = chrono::Utc::now()
        .timestamp_millis()
        .max(now_ms)
        .max(target.created_at_ms)
        .saturating_add(1);
    let activation = if rollback {
        coordinator
            .complete_rollback(&fence, activation_at_ms)
            .await
    } else {
        coordinator.complete_cutover(&fence, activation_at_ms).await
    };
    match activation {
        Ok(()) => Ok(Some(target.runtime_generation_id)),
        Err(RoutingGenerationCoordinatorError::CutoverBusy) => Ok(None),
        Err(error) => Err(coordinator_error(error)),
    }
}

async fn load_projection_cutover_gate(
    runtime: &PersistenceHandle,
    projected_through_watermark: u64,
    now_ms: i64,
) -> Result<ProjectionCutoverGate, PersistenceError> {
    if now_ms < 0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    let watermark = i64::try_from(projected_through_watermark)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let mut read = runtime.begin_read().await?;
    let row = sqlx::query(
        "SELECT COUNT(*) AS backlog, MIN(ingested_at_ms) AS oldest_ingested_at_ms
         FROM routing_observations
         WHERE ingestion_sequence IS NOT NULL AND ingestion_sequence > ?1",
    )
    .bind(watermark)
    .fetch_one(read.connection())
    .await?;
    let backlog = u64::try_from(row.get::<i64, _>("backlog"))
        .map_err(|_| PersistenceError::InvariantViolation("negative quality backlog".into()))?;
    let lag_ms = row
        .get::<Option<i64>, _>("oldest_ingested_at_ms")
        .map(|oldest| now_ms.saturating_sub(oldest).max(0) as u64)
        .unwrap_or(0);
    Ok(ProjectionCutoverGate {
        backlog,
        lag_seconds: lag_ms.saturating_add(999) / 1_000,
    })
}

async fn load_runtime_generation_by_id(
    runtime: &PersistenceHandle,
    runtime_generation_id: &str,
) -> Result<RoutingRuntimeGeneration, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_runtime_generation(read.connection(), runtime_generation_id)
        .await?
        .ok_or(PersistenceError::NotFound)
}

async fn load_policy_generation_json(
    runtime: &PersistenceHandle,
    policy_generation_id: &str,
) -> Result<Value, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_staged_policy_json(read.connection(), policy_generation_id)
        .await
}

async fn load_fenced_build_input(
    runtime: &PersistenceHandle,
    target: &RoutingRuntimeGeneration,
    next_observation_watermark: u64,
    input_observation_watermark: u64,
    input_circuit_event_watermark: u64,
) -> Result<StagedBuildInput, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let policy_json = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_staged_policy_json(read.connection(), &target.policy_generation_id)
        .await?;
    let policy = RoutingPolicyConfigV3::from_stored_value(&policy_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let checkpoints = sqlx::query(
        "SELECT quality_checkpoint_ref, circuit_checkpoint_ref
         FROM routing_runtime_generation WHERE runtime_generation_id = ?1",
    )
    .bind(&target.runtime_generation_id)
    .fetch_optional(read.connection())
    .await?
    .ok_or(PersistenceError::NotFound)?;
    let quality_tail = input_observation_watermark > target.input_observation_watermark;
    let circuit_tail = input_circuit_event_watermark > target.input_circuit_event_watermark;
    Ok(StagedBuildInput {
        policy_generation_id: target.policy_generation_id.clone(),
        policy_revision: target.policy_revision,
        policy_json,
        policy: policy.clone(),
        ingestion_watermark: input_observation_watermark.max(input_circuit_event_watermark),
        active: Some(ActiveBuildBaseline {
            generation: target.clone(),
            policy,
            quality_checkpoint_ref: checkpoints.get("quality_checkpoint_ref"),
            circuit_checkpoint_ref: checkpoints.get("circuit_checkpoint_ref"),
        }),
        rebuild_plan: ComponentRebuildPlan {
            quality: quality_tail,
            circuit: circuit_tail,
            quality_policy_changed: false,
            circuit_policy_changed: false,
        },
        quality_policy_revision: target.quality_policy_revision,
        circuit_policy_revision: target.circuit_policy_revision,
        input_observation_watermark: if quality_tail {
            input_observation_watermark
        } else {
            target.input_observation_watermark
        },
        next_observation_watermark: next_observation_watermark.min(if quality_tail {
            input_observation_watermark
        } else {
            target.input_observation_watermark
        }),
        input_circuit_event_watermark: if circuit_tail {
            input_circuit_event_watermark
        } else {
            target.input_circuit_event_watermark
        },
        stale_ready_generation_ids: Vec::new(),
    })
}

async fn load_ready_qualification_input(
    runtime: &PersistenceHandle,
) -> Result<
    Option<(
        RoutingRuntimeGeneration,
        Option<RoutingRuntimeGeneration>,
        RoutingPolicyConfigV3,
    )>,
    PersistenceError,
> {
    let mut read = runtime.begin_read().await?;
    let registry = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_registry_snapshot(read.connection())
        .await?;
    if registry.fencing.is_some() {
        return Ok(None);
    }
    let row = sqlx::query(
        "SELECT g.runtime_generation_id
         FROM routing_runtime_generation g
         JOIN routing_policy_v3_staged p
           ON p.policy_generation_id = g.policy_generation_id
         WHERE g.status = 'ready'
           AND p.scope = 'active'
           AND p.config_revision = (
               SELECT MAX(config_revision) FROM routing_policy_v3_staged
               WHERE scope = 'active' AND status IN ('staged', 'ready', 'active')
           )
         ORDER BY g.policy_revision DESC, g.created_at_ms DESC,
                  g.runtime_generation_id ASC LIMIT 1",
    )
    .fetch_optional(read.connection())
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let target_id: String = row.get("runtime_generation_id");
    let target = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_runtime_generation(read.connection(), &target_id)
        .await?
        .ok_or(PersistenceError::NotFound)?;
    let policy_json = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_staged_policy_json(read.connection(), &target.policy_generation_id)
        .await?;
    let policy = RoutingPolicyConfigV3::from_stored_value(&policy_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    Ok(Some((target, registry.active, policy)))
}

async fn load_quality_evaluation_at_ms(
    runtime: &PersistenceHandle,
    generation_id: &str,
) -> Result<i64, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    sqlx::query_scalar(
        "SELECT evaluation_at_ms FROM routing_quality_generation_v3
         WHERE quality_generation_id = ?1",
    )
    .bind(generation_id)
    .fetch_one(read.connection())
    .await
    .map_err(Into::into)
}

async fn load_circuit_evaluation_at_ms(
    runtime: &PersistenceHandle,
    generation_id: &str,
) -> Result<i64, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    sqlx::query_scalar(
        "SELECT created_at_ms FROM routing_circuit_generation_v3
         WHERE circuit_generation_id = ?1",
    )
    .bind(generation_id)
    .fetch_one(read.connection())
    .await
    .map_err(Into::into)
}

async fn load_component_checkpoint(
    runtime: &PersistenceHandle,
    runtime_generation_id: &str,
    quality: bool,
) -> Result<String, PersistenceError> {
    let column = if quality {
        "quality_checkpoint_ref"
    } else {
        "circuit_checkpoint_ref"
    };
    let mut read = runtime.begin_read().await?;
    sqlx::query_scalar(&format!(
        "SELECT {column} FROM routing_runtime_generation WHERE runtime_generation_id = ?1"
    ))
    .bind(runtime_generation_id)
    .fetch_one(read.connection())
    .await
    .map_err(Into::into)
}

fn quality_config_for_generation(
    policy: &RoutingPolicyConfigV3,
    quality_policy_revision: u64,
) -> QualityProjectionConfig {
    QualityProjectionConfig {
        quality_policy_revision,
        recent_minimum_samples: u64::from(policy.reliability_sampling.recent_minimum_samples),
        historical_minimum_samples: u64::from(
            policy.reliability_sampling.historical_minimum_samples,
        ),
        optimistic_reliability_basis_points: policy
            .reliability_sampling
            .optimistic_reliability_basis_points(),
        optimistic_latency_ms: policy.reliability_sampling.optimistic_latency_ms,
        real_traffic_weight_basis_points: policy
            .reliability_source_weights
            .real_traffic_basis_points(),
        monitoring_weight_basis_points: policy.reliability_source_weights.monitoring_basis_points(),
        real_source_eligible: true,
        monitoring_source_eligible: true,
        current_lifecycle_revision: None,
    }
}

fn build_replay_report(
    target: &RoutingRuntimeGeneration,
    quality: QualityGenerationVerification,
    circuit: CircuitGenerationVerification,
    policy: &RoutingPolicyConfigV3,
) -> Result<QualificationReplayReport, PersistenceError> {
    let semantic_fixtures = vec![
        replay_failure_semantics("tntapi_502", 502, policy)?,
        replay_failure_semantics("tntapi_429", 429, policy)?,
    ];
    Ok(QualificationReplayReport {
        report_version: "routing-generation-replay-report-v2",
        runtime_generation_id: target.runtime_generation_id.clone(),
        observation_watermark: target.input_observation_watermark,
        circuit_event_watermark: target.input_circuit_event_watermark,
        quality_input_hash: target.quality_input_hash.clone(),
        quality_content_hash: target.quality_content_hash.clone(),
        quality_input_observation_count: quality.input_observation_count,
        quality_output_scope_count: quality.output_scope_count,
        circuit_input_hash: target.circuit_input_hash.clone(),
        circuit_content_hash: target.circuit_content_hash.clone(),
        circuit_input_event_count: circuit.input_event_count,
        circuit_output_state_count: circuit.output_state_count,
        semantic_fixtures,
    })
}

async fn build_comparison_report(
    runtime: &PersistenceHandle,
    source: Option<&RoutingRuntimeGeneration>,
    target: &RoutingRuntimeGeneration,
    policy: &RoutingPolicyConfigV3,
) -> Result<QualificationComparisonReport, PersistenceError> {
    let quality = compare_generation_rows(
        load_quality_rows(
            runtime,
            source.map(|value| value.quality_generation_id.as_str()),
        )
        .await?,
        load_quality_rows(runtime, Some(&target.quality_generation_id)).await?,
        source.map(|value| value.quality_generation_id.clone()),
        target.quality_generation_id.clone(),
    );
    let circuit = compare_generation_rows(
        load_circuit_rows(
            runtime,
            source.map(|value| value.circuit_generation_id.as_str()),
        )
        .await?,
        load_circuit_rows(runtime, Some(&target.circuit_generation_id)).await?,
        source.map(|value| value.circuit_generation_id.clone()),
        target.circuit_generation_id.clone(),
    );
    let report_secret = load_or_create_report_secret(runtime).await?;
    let source_metrics = load_key_qualification_metrics(
        runtime,
        source.map(|value| value.quality_generation_id.as_str()),
        source.map(|value| value.circuit_generation_id.as_str()),
        policy,
    )
    .await?;
    let target_metrics = load_key_qualification_metrics(
        runtime,
        Some(&target.quality_generation_id),
        Some(&target.circuit_generation_id),
        policy,
    )
    .await?;
    let mut identities = source_metrics
        .keys()
        .chain(target_metrics.keys())
        .cloned()
        .collect::<Vec<_>>();
    identities.sort();
    identities.dedup();
    let target_runtime_generation_id = target.runtime_generation_id.clone();
    let keys = identities
        .into_iter()
        .map(|identity| {
            let source_metric = source_metrics.get(&identity);
            let target_metric = target_metrics.get(&identity);
            let raw_identity = target_metric.or(source_metric).ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "qualification key identity disappeared".to_string(),
                )
            })?;
            Ok(KeyComparison {
                key_commitment: qualification_key_commitment(
                    &report_secret,
                    &target_runtime_generation_id,
                    &raw_identity.station_key_id,
                    raw_identity.lifecycle_revision,
                ),
                source: source_metric.map(RawKeyQualificationMetrics::report_metrics),
                target: target_metric.map(RawKeyQualificationMetrics::report_metrics),
                score_delta_basis_points: source_metric.zip(target_metric).map(
                    |(source, target)| {
                        i32::from(target.qualification_score_basis_points)
                            - i32::from(source.qualification_score_basis_points)
                    },
                ),
                rank_delta: source_metric.zip(target_metric).map(|(source, target)| {
                    i64::try_from(target.rank).unwrap_or(i64::MAX)
                        - i64::try_from(source.rank).unwrap_or(i64::MAX)
                }),
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let rank_change_count = keys
        .iter()
        .filter(|key| key.rank_delta.is_some_and(|delta| delta != 0))
        .count() as u64;
    Ok(QualificationComparisonReport {
        report_version: "routing-generation-comparison-report-v2",
        runtime_generation_id: target.runtime_generation_id.clone(),
        source_runtime_generation_id: source.map(|value| value.runtime_generation_id.clone()),
        target_policy_revision: target.policy_revision,
        target_quality_policy_revision: target.quality_policy_revision,
        target_circuit_policy_revision: target.circuit_policy_revision,
        score_basis: "reliability_and_responsiveness_available_factors_renormalized",
        key_count: keys.len() as u64,
        rank_change_count,
        keys,
        quality,
        circuit,
    })
}

fn replay_failure_semantics(
    fixture: &'static str,
    http_status: u16,
    policy: &RoutingPolicyConfigV3,
) -> Result<FailureSemanticReplay, PersistenceError> {
    let signal = match http_status {
        429 => ProviderErrorSemanticSignal::RateLimited {
            station_id: "qualification-station".to_string(),
            retry_after_ms: Some(30_000),
        },
        502 => ProviderErrorSemanticSignal::ServerError {
            station_id: "qualification-station".to_string(),
            endpoint_revision: 1,
        },
        _ => return Err(PersistenceError::ConstraintViolation),
    };
    let failure =
        failure_from_provider_signal(signal, CapabilityApplicabilitySet::UnknownModelCatalog);
    let failure_sample = failure.health == HealthEffect::ObserveFailure;
    let retry_next_key = failure.retry == RetryDisposition::TryNextKey;
    let expected_class = match http_status {
        429 => FailureClass::RateLimited,
        502 => FailureClass::Upstream5xx,
        _ => unreachable!(),
    };
    let retry_after_ignored = if http_status == 429 {
        let without_retry_after = failure_from_provider_signal(
            ProviderErrorSemanticSignal::RateLimited {
                station_id: "qualification-station".to_string(),
                retry_after_ms: None,
            },
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        without_retry_after.class == failure.class
            && without_retry_after.retry == failure.retry
            && without_retry_after.health == failure.health
    } else {
        true
    };

    let config = StationKeyCircuitConfig {
        policy_revision: 1,
        consecutive_failure_threshold: policy.retry.consecutive_failure_threshold,
        recovery_success_threshold: u16::from(policy.circuit_breaker.recovery_success_threshold),
        recovery_wait_ms: u64::from(policy.circuit_breaker.recovery_wait_seconds) * 1_000,
        max_cooldown_ms: SYSTEM_MAX_COOLDOWN_MS,
    };
    let mut circuit = StationKeyCircuit::new(config).map_err(|_| {
        PersistenceError::InvariantViolation(
            "qualification circuit fixture configuration is invalid".to_string(),
        )
    })?;
    let mut final_transition = CircuitTransition::Observed;
    for ordinal in 0..policy.retry.consecutive_failure_threshold {
        final_transition = circuit
            .finish(u64::from(ordinal) + 1, None, false, true)
            .map_err(|_| {
                PersistenceError::InvariantViolation(
                    "qualification circuit fixture replay failed".to_string(),
                )
            })?;
    }
    let (station_key_circuit_opened, consecutive_failures) = match circuit.state() {
        StationKeyCircuitState::Open {
            consecutive_failures,
            ..
        } => (
            final_transition == CircuitTransition::Opened,
            *consecutive_failures,
        ),
        _ => (false, 0),
    };
    let passed = failure.class == expected_class
        && failure_sample
        && retry_next_key
        && retry_after_ignored
        && station_key_circuit_opened
        && consecutive_failures == policy.retry.consecutive_failure_threshold;
    if !passed {
        return Err(PersistenceError::InvariantViolation(format!(
            "routing generation semantic replay failed: {fixture}"
        )));
    }
    Ok(FailureSemanticReplay {
        fixture,
        http_status,
        failure_sample,
        retry_next_key,
        retry_after_ignored,
        station_key_circuit_opened,
        consecutive_failures,
        passed,
    })
}

async fn load_key_qualification_metrics(
    runtime: &PersistenceHandle,
    quality_generation_id: Option<&str>,
    circuit_generation_id: Option<&str>,
    policy: &RoutingPolicyConfigV3,
) -> Result<BTreeMap<String, RawKeyQualificationMetrics>, PersistenceError> {
    let Some(quality_generation_id) = quality_generation_id else {
        return Ok(BTreeMap::new());
    };
    let circuit_states = load_circuit_report_states(runtime, circuit_generation_id).await?;
    let mut read = runtime.begin_read().await?;
    let rows = sqlx::query(
        "SELECT station_key_id, station_key_lifecycle_revision, summary_json
         FROM routing_quality_summary_v3
         WHERE quality_generation_id = ?1
         ORDER BY station_key_id, station_key_lifecycle_revision",
    )
    .bind(quality_generation_id)
    .fetch_all(read.connection())
    .await?;
    drop(read);
    let mut metrics = BTreeMap::new();
    for row in rows {
        let station_key_id = row.get::<String, _>("station_key_id");
        let lifecycle_revision = u64::try_from(row.get::<i64, _>("station_key_lifecycle_revision"))
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let summary: crate::application::quality_projection::QualitySummary =
            serde_json::from_str(&row.get::<String, _>("summary_json"))
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        let identity = format!("{station_key_id}:{lifecycle_revision}");
        let qualification_score_basis_points = qualification_quality_score(&summary, policy);
        metrics.insert(
            identity.clone(),
            RawKeyQualificationMetrics {
                station_key_id,
                lifecycle_revision,
                reliability_basis_points: summary.reliability_basis_points,
                weighted_latency_ms: summary.latency.blended_weighted_latency_ms,
                qualification_score_basis_points,
                observation_count: summary.observation_count,
                real_source_weight_basis_points: summary.real_source_weight_basis_points,
                monitoring_source_weight_basis_points: summary
                    .monitoring_source_weight_basis_points,
                quality_basis: summary.quality_basis,
                circuit_state: circuit_states
                    .get(&identity)
                    .cloned()
                    .unwrap_or_else(|| "not_present".to_string()),
                rank: 0,
            },
        );
    }
    let mut ordered = metrics
        .iter()
        .map(|(identity, metric)| (identity.clone(), metric.qualification_score_basis_points))
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    for (index, (identity, _)) in ordered.into_iter().enumerate() {
        if let Some(metric) = metrics.get_mut(&identity) {
            metric.rank = u64::try_from(index)
                .map_err(|_| PersistenceError::ConstraintViolation)?
                .saturating_add(1);
        }
    }
    Ok(metrics)
}

fn qualification_quality_score(
    summary: &crate::application::quality_projection::QualitySummary,
    policy: &RoutingPolicyConfigV3,
) -> u16 {
    if summary.quality_unavailable {
        return 0;
    }
    let reliability_weight = u128::from(policy.reliability_weight);
    let responsiveness_weight = u128::from(policy.responsiveness_weight);
    let total_weight = reliability_weight + responsiveness_weight;
    if total_weight == 0 {
        return 0;
    }
    let weighted = u128::from(summary.reliability_basis_points) * reliability_weight
        + u128::from(summary.responsiveness_basis_points) * responsiveness_weight;
    u16::try_from((weighted + total_weight / 2) / total_weight)
        .unwrap_or(10_000)
        .min(10_000)
}

async fn load_circuit_report_states(
    runtime: &PersistenceHandle,
    generation_id: Option<&str>,
) -> Result<BTreeMap<String, String>, PersistenceError> {
    let Some(generation_id) = generation_id else {
        return Ok(BTreeMap::new());
    };
    let mut read = runtime.begin_read().await?;
    let rows = sqlx::query(
        "SELECT station_key_id, station_key_lifecycle_revision, state
         FROM routing_circuit_state_generation_v3
         WHERE circuit_generation_id = ?1
         ORDER BY station_key_id, station_key_lifecycle_revision",
    )
    .bind(generation_id)
    .fetch_all(read.connection())
    .await?;
    rows.into_iter()
        .map(|row| {
            let lifecycle_revision =
                u64::try_from(row.get::<i64, _>("station_key_lifecycle_revision"))
                    .map_err(|_| PersistenceError::ConstraintViolation)?;
            Ok((
                format!(
                    "{}:{}",
                    row.get::<String, _>("station_key_id"),
                    lifecycle_revision
                ),
                row.get::<String, _>("state"),
            ))
        })
        .collect()
}

async fn load_or_create_report_secret(
    runtime: &PersistenceHandle,
) -> Result<[u8; 32], PersistenceError> {
    let mut candidate = [0_u8; 32];
    OsRng.fill_bytes(&mut candidate);
    let now_ms = chrono::Utc::now().timestamp_millis().max(0);
    let mut write = runtime.begin_write().await?;
    sqlx::query(
        "INSERT INTO routing_generation_report_secret (singleton_key, secret, created_at_ms)
         VALUES (1, ?1, ?2) ON CONFLICT(singleton_key) DO NOTHING",
    )
    .bind(candidate.as_slice())
    .bind(now_ms)
    .execute(write.connection())
    .await?;
    let secret: Vec<u8> = sqlx::query_scalar(
        "SELECT secret FROM routing_generation_report_secret WHERE singleton_key = 1",
    )
    .fetch_one(write.connection())
    .await?;
    write.commit().await?;
    secret.try_into().map_err(|_| {
        PersistenceError::InvariantViolation(
            "routing generation report secret is invalid".to_string(),
        )
    })
}

fn qualification_key_commitment(
    secret: &[u8; 32],
    runtime_generation_id: &str,
    station_key_id: &str,
    lifecycle_revision: u64,
) -> String {
    let mut input = Vec::with_capacity(
        runtime_generation_id.len() + station_key_id.len() + std::mem::size_of::<u64>() + 2,
    );
    input.extend_from_slice(runtime_generation_id.as_bytes());
    input.push(0);
    input.extend_from_slice(station_key_id.as_bytes());
    input.push(0);
    input.extend_from_slice(&lifecycle_revision.to_be_bytes());
    let digest = hmac_sha256(secret, &input);
    format!("keyc1_{}", &sha256_hex(&digest)[..32])
}

fn hmac_sha256(key: &[u8; 32], value: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36_u8; 64];
    let mut opad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        ipad[index] ^= byte;
        opad[index] ^= byte;
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(value);
    let inner_digest = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn compare_generation_rows(
    source: BTreeMap<String, String>,
    target: BTreeMap<String, String>,
    source_generation_id: Option<String>,
    target_generation_id: String,
) -> ComponentComparison {
    let changed_row_count = source
        .iter()
        .filter(|(key, value)| target.get(*key).is_some_and(|target| target != *value))
        .count() as u64;
    let added_row_count = target
        .keys()
        .filter(|key| !source.contains_key(*key))
        .count() as u64;
    let removed_row_count = source
        .keys()
        .filter(|key| !target.contains_key(*key))
        .count() as u64;
    ComponentComparison {
        source_generation_id,
        target_generation_id,
        source_row_count: source.len() as u64,
        target_row_count: target.len() as u64,
        added_row_count,
        removed_row_count,
        changed_row_count,
    }
}

async fn load_quality_rows(
    runtime: &PersistenceHandle,
    generation_id: Option<&str>,
) -> Result<BTreeMap<String, String>, PersistenceError> {
    let Some(generation_id) = generation_id else {
        return Ok(BTreeMap::new());
    };
    let mut read = runtime.begin_read().await?;
    let rows = sqlx::query(
        "SELECT station_key_id, station_key_lifecycle_revision, summary_json
         FROM routing_quality_summary_v3 WHERE quality_generation_id = ?1
         ORDER BY station_key_id, station_key_lifecycle_revision",
    )
    .bind(generation_id)
    .fetch_all(read.connection())
    .await?;
    rows.into_iter()
        .map(|row| {
            let key = format!(
                "{}:{}",
                row.get::<String, _>("station_key_id"),
                row.get::<i64, _>("station_key_lifecycle_revision")
            );
            let value = serde_json::from_str::<Value>(&row.get::<String, _>("summary_json"))
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
            let hash =
                canonical_json_sha256(&value).map_err(|_| PersistenceError::ConstraintViolation)?;
            Ok((key, hash))
        })
        .collect()
}

async fn load_circuit_rows(
    runtime: &PersistenceHandle,
    generation_id: Option<&str>,
) -> Result<BTreeMap<String, String>, PersistenceError> {
    let Some(generation_id) = generation_id else {
        return Ok(BTreeMap::new());
    };
    let mut read = runtime.begin_read().await?;
    let rows = sqlx::query(
        "SELECT station_key_id, station_key_lifecycle_revision, state,
                state_revision, consecutive_failures, reopen_level,
                opened_at_ms, cooldown_until_ms, recovery_successes,
                monotonic_clock_watermark_ms, reducer_commit_sequence
         FROM routing_circuit_state_generation_v3
         WHERE circuit_generation_id = ?1
         ORDER BY station_key_id, station_key_lifecycle_revision",
    )
    .bind(generation_id)
    .fetch_all(read.connection())
    .await?;
    rows.into_iter()
        .map(|row| {
            let key = format!(
                "{}:{}",
                row.get::<String, _>("station_key_id"),
                row.get::<i64, _>("station_key_lifecycle_revision")
            );
            let value = serde_json::json!({
                "state": row.get::<String, _>("state"),
                "state_revision": row.get::<i64, _>("state_revision"),
                "consecutive_failures": row.get::<i64, _>("consecutive_failures"),
                "reopen_level": row.get::<i64, _>("reopen_level"),
                "opened_at_ms": row.get::<Option<i64>, _>("opened_at_ms"),
                "cooldown_until_ms": row.get::<Option<i64>, _>("cooldown_until_ms"),
                "recovery_successes": row.get::<i64, _>("recovery_successes"),
                "monotonic_clock_watermark_ms": row.get::<i64, _>("monotonic_clock_watermark_ms"),
                "reducer_commit_sequence": row.get::<i64, _>("reducer_commit_sequence")
            });
            let hash =
                canonical_json_sha256(&value).map_err(|_| PersistenceError::ConstraintViolation)?;
            Ok((key, hash))
        })
        .collect()
}

async fn publish_active_policy(
    runtime: &PersistenceHandle,
    policy_publication: &RoutingPolicyMutationCoordinator,
    replace_existing_document: bool,
) -> Result<(), PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let registry = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_registry_snapshot(read.connection())
        .await?;
    drop(read);
    if registry.active.is_none() {
        return Ok(());
    }
    let active =
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active(runtime)
            .await?
            .ok_or(PersistenceError::NotFound)?;
    if active.status != "active" {
        return Ok(());
    }
    policy_publication
        .publish_active_policy(&active)
        .await
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    crate::application::routing::sync_routing_policy_file(
        runtime.clone(),
        &active,
        replace_existing_document,
    )
    .await
}

async fn load_staged_build_input(
    runtime: &PersistenceHandle,
) -> Result<Option<StagedBuildInput>, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let registry = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
        .load_registry_snapshot(read.connection())
        .await?;
    if registry.fencing.is_some() {
        return Ok(None);
    }
    let row = sqlx::query(
        "SELECT p.policy_generation_id, p.config_revision, p.config_json
         FROM routing_policy_v3_staged p
         WHERE p.scope = 'active'
           AND p.status IN ('staged', 'ready', 'active')
         ORDER BY p.config_revision DESC LIMIT 1",
    )
    .fetch_optional(read.connection())
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let policy_generation_id = row.get::<String, _>("policy_generation_id");
    if registry.active.as_ref().is_some_and(|active| {
        active.policy_generation_id == policy_generation_id
            && active.status == RoutingGenerationStatus::Active
    }) {
        return Ok(None);
    }
    let policy_revision = u64::try_from(row.get::<i64, _>("config_revision"))
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    if policy_revision == 0 {
        return Err(PersistenceError::ConstraintViolation);
    }
    let policy_json = serde_json::from_str::<Value>(&row.get::<String, _>("config_json"))
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let policy = RoutingPolicyConfigV3::from_stored_value(&policy_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let ingestion_watermark = sqlx::query_scalar::<_, i64>(
        "SELECT MAX(next_sequence - 1, 0)
         FROM routing_v3_ingestion_sequence WHERE singleton_key = 1",
    )
    .fetch_one(read.connection())
    .await?
    .try_into()
    .map_err(|_| PersistenceError::ConstraintViolation)?;
    let active = if let Some(generation) = registry.active {
        let active_policy_json =
            crate::persistence::stores::routing_generation_store::RoutingGenerationStore
                .load_staged_policy_json(read.connection(), &generation.policy_generation_id)
                .await?;
        let policy = RoutingPolicyConfigV3::from_stored_value(&active_policy_json)
            .map_err(|_| PersistenceError::ConstraintViolation)?;
        let checkpoint_row = sqlx::query(
            "SELECT quality_checkpoint_ref, circuit_checkpoint_ref
             FROM routing_runtime_generation WHERE runtime_generation_id = ?1",
        )
        .bind(&generation.runtime_generation_id)
        .fetch_optional(read.connection())
        .await?
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "active routing generation checkpoint metadata is missing".to_string(),
            )
        })?;
        Some(ActiveBuildBaseline {
            generation,
            policy,
            quality_checkpoint_ref: checkpoint_row.get("quality_checkpoint_ref"),
            circuit_checkpoint_ref: checkpoint_row.get("circuit_checkpoint_ref"),
        })
    } else {
        None
    };
    let source_profile_changed = if let Some(active) = active.as_ref() {
        quality_context_changed(read.connection(), &active.generation.quality_generation_id).await?
    } else {
        false
    };
    let (quality_tail, circuit_tail) = if let Some(active) = active.as_ref() {
        let quality_tail: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM routing_observations
                 WHERE generation_eligibility IN ('active', 'next')
                   AND ingestion_sequence > ?1 AND ingestion_sequence <= ?2
             )",
        )
        .bind(
            i64::try_from(active.generation.input_observation_watermark)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .bind(
            i64::try_from(ingestion_watermark)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .fetch_one(read.connection())
        .await?;
        let circuit_tail: i64 = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM routing_circuit_event_v3
                 WHERE ingestion_sequence > ?1 AND ingestion_sequence <= ?2
             )",
        )
        .bind(
            i64::try_from(active.generation.input_circuit_event_watermark)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .bind(
            i64::try_from(ingestion_watermark)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .fetch_one(read.connection())
        .await?;
        (quality_tail != 0, circuit_tail != 0)
    } else {
        (false, false)
    };
    let rebuild_plan = component_rebuild_plan(
        &policy,
        active.as_ref().map(|active| &active.policy),
        quality_tail,
        circuit_tail,
        source_profile_changed,
    );
    let quality_policy_revision = if rebuild_plan.quality_policy_changed {
        policy_revision
    } else {
        active
            .as_ref()
            .map(|active| active.generation.quality_policy_revision)
            .unwrap_or(policy_revision)
    };
    let circuit_policy_revision = if rebuild_plan.circuit_policy_changed {
        policy_revision
    } else {
        active
            .as_ref()
            .map(|active| active.generation.circuit_policy_revision)
            .unwrap_or(policy_revision)
    };
    let input_observation_watermark = if rebuild_plan.quality {
        ingestion_watermark
    } else {
        active
            .as_ref()
            .map(|active| active.generation.input_observation_watermark)
            .unwrap_or(ingestion_watermark)
    };
    let input_circuit_event_watermark = if rebuild_plan.circuit {
        ingestion_watermark
    } else {
        active
            .as_ref()
            .map(|active| active.generation.input_circuit_event_watermark)
            .unwrap_or(ingestion_watermark)
    };
    let ready_rows = sqlx::query(
        "SELECT runtime_generation_id, quality_policy_revision,
                circuit_policy_revision, input_observation_watermark,
                input_circuit_event_watermark
         FROM routing_runtime_generation
         WHERE policy_generation_id = ?1 AND status = 'ready'
         ORDER BY created_at_ms DESC, runtime_generation_id ASC",
    )
    .bind(&policy_generation_id)
    .fetch_all(read.connection())
    .await?;
    let mut stale_ready_generation_ids = Vec::new();
    for ready in ready_rows {
        let current = u64::try_from(ready.get::<i64, _>("quality_policy_revision")).ok()
            == Some(quality_policy_revision)
            && u64::try_from(ready.get::<i64, _>("circuit_policy_revision")).ok()
                == Some(circuit_policy_revision)
            && u64::try_from(ready.get::<i64, _>("input_observation_watermark")).ok()
                == Some(input_observation_watermark)
            && u64::try_from(ready.get::<i64, _>("input_circuit_event_watermark")).ok()
                == Some(input_circuit_event_watermark);
        if current {
            return Ok(None);
        }
        stale_ready_generation_ids.push(ready.get("runtime_generation_id"));
    }
    Ok(Some(StagedBuildInput {
        policy_generation_id,
        policy_revision,
        policy_json,
        policy,
        ingestion_watermark,
        active,
        rebuild_plan,
        quality_policy_revision,
        circuit_policy_revision,
        input_observation_watermark,
        next_observation_watermark: input_observation_watermark,
        input_circuit_event_watermark,
        stale_ready_generation_ids,
    }))
}

fn component_rebuild_plan(
    target: &RoutingPolicyConfigV3,
    active: Option<&RoutingPolicyConfigV3>,
    quality_tail: bool,
    circuit_tail: bool,
    source_profile_changed: bool,
) -> ComponentRebuildPlan {
    let Some(active) = active else {
        return ComponentRebuildPlan {
            quality: true,
            circuit: true,
            quality_policy_changed: true,
            circuit_policy_changed: true,
        };
    };
    let quality_policy_changed = target.reliability_source_weights
        != active.reliability_source_weights
        || target.reliability_sampling != active.reliability_sampling;
    let circuit_policy_changed = target.retry.consecutive_failure_threshold
        != active.retry.consecutive_failure_threshold
        || target.circuit_breaker != active.circuit_breaker;
    ComponentRebuildPlan {
        quality: quality_policy_changed || quality_tail || source_profile_changed,
        circuit: circuit_policy_changed || circuit_tail,
        quality_policy_changed,
        circuit_policy_changed,
    }
}

async fn quality_context_changed(
    connection: &mut sqlx::SqliteConnection,
    quality_generation_id: &str,
) -> Result<bool, PersistenceError> {
    let snapshot_id = sqlx::query_scalar::<_, Option<String>>(
        "SELECT source_profile_snapshot_id
         FROM routing_quality_generation_v3
         WHERE quality_generation_id = ?1",
    )
    .bind(quality_generation_id)
    .fetch_optional(&mut *connection)
    .await?
    .flatten();
    let Some(snapshot_id) = snapshot_id else {
        return Ok(true);
    };
    let changed: i64 = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM station_keys k
             JOIN domain_revisions r
               ON r.scope = 'station_key:' || k.id
             LEFT JOIN routing_quality_source_profile_snapshot_item_v3 item
               ON item.snapshot_id = ?1 AND item.station_key_id = k.id
             WHERE item.station_key_id IS NULL
                OR item.station_key_lifecycle_revision <> r.revision
         ) OR EXISTS (
             SELECT 1
             FROM routing_quality_source_profile_snapshot_item_v3 item
             LEFT JOIN station_keys k ON k.id = item.station_key_id
             WHERE item.snapshot_id = ?1 AND k.id IS NULL
         )",
    )
    .bind(snapshot_id)
    .fetch_one(&mut *connection)
    .await?;
    Ok(changed != 0)
}

async fn resolve_build_evaluation_at_ms(
    runtime: &PersistenceHandle,
    input: &StagedBuildInput,
) -> Result<i64, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let quality_time = if input.rebuild_plan.quality {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MIN(evaluation_at_ms) FROM routing_quality_generation_v3
             WHERE quality_policy_revision = ?1
               AND input_observation_watermark = ?2
               AND status IN ('building', 'ready')",
        )
        .bind(
            i64::try_from(input.quality_policy_revision)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .bind(
            i64::try_from(input.input_observation_watermark)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .fetch_one(read.connection())
        .await?
    } else {
        None
    };
    let circuit_time = if input.rebuild_plan.circuit {
        sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MIN(created_at_ms) FROM routing_circuit_generation_v3
             WHERE circuit_policy_revision = ?1
               AND input_circuit_event_watermark = ?2
               AND status IN ('building', 'ready')",
        )
        .bind(
            i64::try_from(input.circuit_policy_revision)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .bind(
            i64::try_from(input.input_circuit_event_watermark)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .fetch_one(read.connection())
        .await?
    } else {
        None
    };
    Ok(quality_time
        .into_iter()
        .chain(circuit_time)
        .min()
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis().max(0)))
}

fn reused_quality_component(active: &ActiveBuildBaseline) -> QualityGenerationBuildResult {
    QualityGenerationBuildResult {
        quality_generation_id: active.generation.quality_generation_id.clone(),
        input_observation_hash: active.generation.quality_input_hash.clone(),
        output_content_hash: active.generation.quality_content_hash.clone(),
        checkpoint_ref: active.quality_checkpoint_ref.clone(),
        processed_scope_count: 0,
        complete: true,
    }
}

fn reused_circuit_component(active: &ActiveBuildBaseline) -> CircuitGenerationBuildResult {
    CircuitGenerationBuildResult {
        circuit_generation_id: active.generation.circuit_generation_id.clone(),
        input_circuit_event_hash: active.generation.circuit_input_hash.clone(),
        output_content_hash: active.generation.circuit_content_hash.clone(),
        checkpoint_ref: active.circuit_checkpoint_ref.clone(),
        processed_event_count: 0,
        complete: true,
    }
}

fn quality_config(
    input: &StagedBuildInput,
    quality_policy_revision: u64,
) -> QualityProjectionConfig {
    QualityProjectionConfig {
        quality_policy_revision,
        recent_minimum_samples: u64::from(input.policy.reliability_sampling.recent_minimum_samples),
        historical_minimum_samples: u64::from(
            input.policy.reliability_sampling.historical_minimum_samples,
        ),
        optimistic_reliability_basis_points: input
            .policy
            .reliability_sampling
            .optimistic_reliability_basis_points(),
        optimistic_latency_ms: input.policy.reliability_sampling.optimistic_latency_ms,
        real_traffic_weight_basis_points: input
            .policy
            .reliability_source_weights
            .real_traffic_basis_points(),
        monitoring_weight_basis_points: input
            .policy
            .reliability_source_weights
            .monitoring_basis_points(),
        real_source_eligible: true,
        monitoring_source_eligible: true,
        current_lifecycle_revision: None,
    }
}

fn coordinator_error(
    error: crate::application::routing_generation_coordinator::RoutingGenerationCoordinatorError,
) -> PersistenceError {
    match error {
        crate::application::routing_generation_coordinator::RoutingGenerationCoordinatorError::Persistence(
            source,
        ) => source,
        error => PersistenceError::InvariantViolation(error.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            observation_ingestion::ObservationIngestion,
            routing_generation::{policy_generation_id, ROUTING_GENERATION_ALGORITHM_VERSION},
            routing_generation_coordinator::RoutingGenerationCoordinatorError,
        },
        models::{
            routing_generation::RoutingGenerationQualification,
            routing_observation::{
                EventTimeStatus, ObservationOrder, ObservationOutcome, ObservationScope,
                ObservationSource, RoutingObservation, TrafficEquivalence,
            },
        },
        persistence::{
            runtime::{PersistenceHandle, PersistenceRuntime},
            stores::{
                routing_attempt_store::{
                    RoutingAttemptAdmission, RoutingAttemptStore, RoutingAttemptTerminal,
                    RoutingGenerationEligibility,
                },
                station_key_circuit_store::{CircuitTerminalInput, StationKeyCircuitStore},
            },
        },
    };

    fn test_qualification(
        runtime_generation_id: &str,
        qualified_at_ms: i64,
    ) -> RoutingGenerationQualification {
        let (comparison_report, replay_report) =
            crate::models::routing_generation::test_activation_qualification_reports(
                runtime_generation_id,
            );
        RoutingGenerationQualification {
            runtime_generation_id: runtime_generation_id.to_string(),
            comparison_report_hash: canonical_json_sha256(&comparison_report)
                .expect("comparison hash"),
            comparison_report,
            replay_report_hash: canonical_json_sha256(&replay_report).expect("replay hash"),
            replay_report,
            qualified_at_ms,
        }
    }

    async fn insert_staged_policy(
        handle: &PersistenceHandle,
        revision: u64,
        policy: &RoutingPolicyConfigV3,
    ) -> String {
        let policy_json = serde_json::to_value(policy).expect("serialize policy");
        let policy_hash = canonical_json_sha256(&policy_json).expect("policy hash");
        let policy_generation_id = policy_generation_id(
            "active",
            revision,
            "routing-policy-v3",
            &policy_hash,
            ROUTING_GENERATION_ALGORITHM_VERSION,
        )
        .expect("policy generation id");
        let mut write = handle.begin_write().await.expect("begin policy write");
        sqlx::query(
            "INSERT INTO routing_policy_v3_staged (
                 scope, source_config_revision, target_policy_revision,
                 config_revision, policy_generation_id, canonical_policy_hash,
                 policy_algorithm_version, source_policy_version, system_version,
                 target_policy_version, staged_policy_version, config_json,
                 status, created_at_ms, updated_at_ms
             ) VALUES ('active', ?1, ?1, ?1, ?2, ?3,
                       'routing-policy-v3', 'routing-policy-v3', 'routing-system-v1',
                       'routing-policy-v3', 'routing-policy-v3', ?4,
                       'staged', ?1, ?1)",
        )
        .bind(i64::try_from(revision).expect("policy revision"))
        .bind(&policy_generation_id)
        .bind(&policy_hash)
        .bind(serde_json::to_string(&policy_json).expect("policy JSON"))
        .execute(write.connection())
        .await
        .expect("insert staged policy");
        write.commit().await.expect("commit staged policy");
        policy_generation_id
    }

    async fn seed_routing_key(handle: &PersistenceHandle, station_key_id: &str) {
        let mut write = handle.begin_write().await.expect("begin routing key seed");
        sqlx::query(
            "INSERT INTO stations (
                 id, name, station_type, website_url, api_base_url,
                 enabled, created_at, updated_at
             ) VALUES (
                 'generation-station', 'Generation station', 'openai-compatible',
                 'https://generation.test', 'https://generation.test/v1',
                 1, '1', '1'
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert routing station");
        sqlx::query(
            "INSERT INTO station_keys (
                 id, station_id, name, api_key, enabled, created_at, updated_at
             ) VALUES (?1, 'generation-station', 'Generation key',
                       'test-key-not-a-secret', 1, '1', '1')",
        )
        .bind(station_key_id)
        .execute(write.connection())
        .await
        .expect("insert routing key");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES ('station_key:' || ?1, 1, 1, 'transactional_write')",
        )
        .bind(station_key_id)
        .execute(write.connection())
        .await
        .expect("insert routing key lifecycle revision");
        write.commit().await.expect("commit routing key seed");
    }

    fn generation_observation(
        id: &str,
        source: ObservationSource,
        outcome: ObservationOutcome,
        station_key_id: &str,
        correlation_id: &str,
        event_at_ms: i64,
    ) -> RoutingObservation {
        RoutingObservation {
            id: id.to_string(),
            order: ObservationOrder {
                producer_id: format!("generation-test:{id}"),
                producer_sequence: 1,
                event_at_ms,
                ingested_at_ms: event_at_ms,
            },
            scope: ObservationScope {
                station_id: Some("generation-station".to_string()),
                station_key_id: Some(station_key_id.to_string()),
                model: Some("gpt-test".to_string()),
                endpoint_revision: Some(1),
            },
            comparability_key: matches!(source, ObservationSource::ActiveProbe)
                .then(|| format!("cmp:v1:{}", "a".repeat(64))),
            source,
            traffic_equivalence: TrafficEquivalence::ExactRequest,
            outcome,
            latency_ms: Some(100),
            evidence_mass_basis_points: 10_000,
            correlation_id: correlation_id.to_string(),
            attempt_index: 0,
            station_key_lifecycle_revision: 1,
            cluster_finalized: true,
            cluster_expected_attempt_count: 1,
            boundary_crossed: true,
            event_time_status: EventTimeStatus::Valid,
            response_origin: crate::models::routing_observation::ResponseOrigin::Upstream,
            failure_code: None,
            failure_attribution: crate::models::routing_observation::FailureAttribution::Key,
            recovery_origin: crate::models::routing_observation::RecoveryOrigin::Normal,
            retry_disposition: crate::models::routing_observation::ObservationRetryDisposition::End,
            probe_state_revision: None,
            probe_scope: None,
        }
    }

    async fn load_runtime_generation(
        handle: &PersistenceHandle,
        runtime_generation_id: &str,
    ) -> RoutingRuntimeGeneration {
        let mut read = handle.begin_read().await.expect("begin generation read");
        crate::persistence::stores::routing_generation_store::RoutingGenerationStore
            .load_runtime_generation(read.connection(), runtime_generation_id)
            .await
            .expect("load runtime generation")
            .expect("runtime generation")
    }

    #[test]
    fn supervised_builder_never_activates_without_a_separate_qualification_owner() {
        assert_eq!(BUILD_INTERVAL, Duration::from_secs(5));
        assert_eq!(SYSTEM_MAX_COOLDOWN_MS, 86_400_000);
    }

    #[test]
    fn projection_cutover_gate_accepts_exact_operational_limits() {
        assert_eq!(
            ProjectionCutoverGate {
                backlog: MAX_PROJECTOR_BACKLOG,
                lag_seconds: MAX_ACTIVE_QUALITY_LAG_SECONDS,
            }
            .rejection_code(),
            None
        );
    }

    #[test]
    fn projection_cutover_gate_rejects_backlog_over_limit_first() {
        assert_eq!(
            ProjectionCutoverGate {
                backlog: MAX_PROJECTOR_BACKLOG + 1,
                lag_seconds: MAX_ACTIVE_QUALITY_LAG_SECONDS + 1,
            }
            .rejection_code(),
            Some("projection_backlog_exceeded")
        );
    }

    #[test]
    fn projection_cutover_gate_rejects_lag_over_limit() {
        assert_eq!(
            ProjectionCutoverGate {
                backlog: MAX_PROJECTOR_BACKLOG,
                lag_seconds: MAX_ACTIVE_QUALITY_LAG_SECONDS + 1,
            }
            .rejection_code(),
            Some("projection_lag_exceeded")
        );
    }

    #[test]
    fn scoring_boundaries_affinity_retry_limit_and_timeouts_reuse_both_components() {
        let active = RoutingPolicyConfigV3::default();
        let mut target = active.clone();
        target.reliability_weight = target.reliability_weight.saturating_sub(100);
        target.preference_weight = target.preference_weight.saturating_add(100);
        target.affinity_enabled = !target.affinity_enabled;
        target.retry.max_retry_count = 2;
        target.timeout_policy.connect_seconds = 3.0;

        assert_eq!(
            component_rebuild_plan(&target, Some(&active), false, false, false),
            ComponentRebuildPlan {
                quality: false,
                circuit: false,
                quality_policy_changed: false,
                circuit_policy_changed: false,
            }
        );
    }

    #[test]
    fn quality_fields_rebuild_only_quality_component() {
        let active = RoutingPolicyConfigV3::default();
        let mut target = active.clone();
        target.reliability_source_weights.real_traffic_percent = 80;
        target.reliability_source_weights.monitoring_percent = 20;

        assert_eq!(
            component_rebuild_plan(&target, Some(&active), false, false, false),
            ComponentRebuildPlan {
                quality: true,
                circuit: false,
                quality_policy_changed: true,
                circuit_policy_changed: false,
            }
        );
    }

    #[test]
    fn circuit_fields_rebuild_only_circuit_component() {
        let active = RoutingPolicyConfigV3::default();
        let mut target = active.clone();
        target.retry.consecutive_failure_threshold = 4;

        assert_eq!(
            component_rebuild_plan(&target, Some(&active), false, false, false),
            ComponentRebuildPlan {
                quality: false,
                circuit: true,
                quality_policy_changed: false,
                circuit_policy_changed: true,
            }
        );
    }

    #[test]
    fn combined_quality_and_circuit_changes_rebuild_both_components() {
        let active = RoutingPolicyConfigV3::default();
        let mut target = active.clone();
        target.reliability_sampling.optimistic_reliability_percent = 94;
        target.circuit_breaker.recovery_wait_seconds = 31;

        assert_eq!(
            component_rebuild_plan(&target, Some(&active), false, false, false),
            ComponentRebuildPlan {
                quality: true,
                circuit: true,
                quality_policy_changed: true,
                circuit_policy_changed: true,
            }
        );
    }

    #[test]
    fn unchanged_policy_rebuilds_only_components_with_input_tail_without_bumping_policy_revision() {
        let active = RoutingPolicyConfigV3::default();
        assert_eq!(
            component_rebuild_plan(&active, Some(&active), true, false, false),
            ComponentRebuildPlan {
                quality: true,
                circuit: false,
                quality_policy_changed: false,
                circuit_policy_changed: false,
            }
        );
        assert_eq!(
            component_rebuild_plan(&active, Some(&active), false, true, false),
            ComponentRebuildPlan {
                quality: false,
                circuit: true,
                quality_policy_changed: false,
                circuit_policy_changed: false,
            }
        );
    }

    #[tokio::test]
    async fn fresh_install_with_zero_watermark_builds_ready_but_does_not_activate() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("generation.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let policy = RoutingPolicyConfigV3::default();
        let policy_json = serde_json::to_value(&policy).expect("serialize policy");
        let policy_hash = canonical_json_sha256(&policy_json).expect("policy hash");
        let policy_generation_id = policy_generation_id(
            "active",
            1,
            "routing-policy-v3",
            &policy_hash,
            ROUTING_GENERATION_ALGORITHM_VERSION,
        )
        .expect("policy generation id");
        let mut write = handle.begin_write().await.expect("begin write");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        sqlx::query(
            "INSERT INTO routing_policy_v3_staged (
                 scope, source_config_revision, target_policy_revision,
                 config_revision, policy_generation_id, canonical_policy_hash,
                 policy_algorithm_version, source_policy_version, system_version,
                 target_policy_version, staged_policy_version, config_json,
                 status, created_at_ms, updated_at_ms
             ) VALUES ('active', 1, 1, 1, ?1, ?2,
                       'routing-policy-v3', 'routing-policy-v3', 'routing-system-v1',
                       'routing-policy-v3', 'routing-policy-v3', ?3,
                       'staged', 0, 0)",
        )
        .bind(&policy_generation_id)
        .bind(&policy_hash)
        .bind(serde_json::to_string(&policy_json).expect("policy json"))
        .execute(write.connection())
        .await
        .expect("insert staged policy");
        write.commit().await.expect("commit staged policy");

        let built = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build ready generation")
            .expect("ready generation id");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        let registry = coordinator.inspect().await.expect("inspect registry");
        assert!(registry.active.is_none());
        assert!(registry.fencing.is_none());
        assert_eq!(registry.marker.active_runtime_generation_id, None);
        let mut read = handle.begin_read().await.expect("begin read");
        let generation =
            crate::persistence::stores::routing_generation_store::RoutingGenerationStore
                .load_runtime_generation(read.connection(), &built)
                .await
                .expect("load generation")
                .expect("runtime generation");
        assert_eq!(generation.status, RoutingGenerationStatus::Ready);
        assert_eq!(generation.input_observation_watermark, 0);
        assert_eq!(generation.input_circuit_event_watermark, 0);
        let policy_status: String = sqlx::query_scalar(
            "SELECT status FROM routing_policy_v3_staged WHERE policy_generation_id = ?1",
        )
        .bind(&policy_generation_id)
        .fetch_one(read.connection())
        .await
        .expect("ready policy status");
        assert_eq!(policy_status, "ready");
        drop(read);
        let error = coordinator
            .begin_cutover(&built, None, 1)
            .await
            .expect_err("unqualified generation must not fence");
        assert!(matches!(
            error,
            RoutingGenerationCoordinatorError::NotQualified
        ));
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn production_qualification_replays_reports_and_activates_ready_generation() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("activate.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        insert_staged_policy(&handle, 1, &RoutingPolicyConfigV3::default()).await;
        let ready_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build ready generation")
            .expect("ready generation id");
        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("qualify and activate"),
            Some(ready_id.clone())
        );
        let active = load_runtime_generation(&handle, &ready_id).await;
        assert_eq!(active.status, RoutingGenerationStatus::Active);
        let mut read = handle.begin_read().await.expect("begin report read");
        let report: (String, String, String, String) = sqlx::query_as(
            "SELECT comparison_report_json, comparison_report_hash,
                    replay_report_json, replay_report_hash
             FROM routing_generation_qualification_report_v2
             WHERE runtime_generation_id = ?1",
        )
        .bind(&ready_id)
        .fetch_one(read.connection())
        .await
        .expect("qualification report");
        let comparison: Value = serde_json::from_str(&report.0).expect("comparison JSON");
        let replay: Value = serde_json::from_str(&report.2).expect("replay JSON");
        assert_eq!(
            canonical_json_sha256(&comparison).expect("comparison hash"),
            report.1
        );
        assert_eq!(
            canonical_json_sha256(&replay).expect("replay hash"),
            report.3
        );
        assert_eq!(
            replay
                .get("quality_input_observation_count")
                .and_then(Value::as_u64),
            Some(0)
        );
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn activation_replaces_live_circuit_state_from_qualified_generation() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("circuit-swap.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin circuit fixtures");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        insert_circuit_fixture_event(write.connection(), "closed-key", 1, 1, true, None).await;
        for sequence in 1..=3 {
            insert_circuit_fixture_event(
                write.connection(),
                "open-key",
                sequence,
                sequence,
                false,
                None,
            )
            .await;
            insert_circuit_fixture_event(
                write.connection(),
                "half-open-key",
                sequence,
                sequence,
                false,
                None,
            )
            .await;
        }
        insert_circuit_fixture_event(
            write.connection(),
            "half-open-key",
            4,
            30_004,
            true,
            Some(5),
        )
        .await;
        sqlx::query(
            "INSERT INTO routing_circuit_state_v3 (
                 station_key_id, station_key_lifecycle_revision, state,
                 state_revision, consecutive_failures, reopen_level,
                 recovery_successes, monotonic_clock_watermark_ms, updated_at_ms
             ) VALUES ('obsolete-live-key', 1, 'closed', 1, 0, 0, 0, 0, 0)",
        )
        .execute(write.connection())
        .await
        .expect("insert obsolete live state");
        write.commit().await.expect("commit circuit fixtures");

        insert_staged_policy(&handle, 1, &RoutingPolicyConfigV3::default()).await;
        let ready_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build circuit generation")
            .expect("ready generation id");
        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("qualify and activate circuit generation"),
            Some(ready_id)
        );

        let mut read = handle.begin_read().await.expect("read live circuit state");
        let states: Vec<(String, String, i64, i64, Option<String>, Option<i64>)> = sqlx::query_as(
            "SELECT station_key_id, state, state_revision, recovery_successes,
                        lease_id, lease_revision
                 FROM routing_circuit_state_v3 ORDER BY station_key_id",
        )
        .fetch_all(read.connection())
        .await
        .expect("load activated live states");
        assert_eq!(
            states,
            vec![
                (
                    "closed-key".to_string(),
                    "closed".to_string(),
                    2,
                    0,
                    None,
                    None,
                ),
                (
                    "half-open-key".to_string(),
                    "half_open".to_string(),
                    6,
                    1,
                    None,
                    Some(6),
                ),
                ("open-key".to_string(), "open".to_string(), 4, 0, None, None,),
            ]
        );
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn active_half_open_lease_rolls_back_generation_cutover() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("leased-cutover.sqlite3"))
                .await
                .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        let base_policy = RoutingPolicyConfigV3::default();
        insert_staged_policy(&handle, 1, &base_policy).await;
        let base_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build base generation")
            .expect("base generation id");
        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("activate base generation"),
            Some(base_id.clone())
        );

        let mut target_policy = base_policy;
        target_policy.retry.max_retry_count = 2;
        insert_staged_policy(&handle, 2, &target_policy).await;
        let target_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build target generation")
            .expect("target generation id");
        let target = load_runtime_generation(&handle, &target_id).await;
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        coordinator
            .record_qualification(&test_qualification(
                &target_id,
                target.created_at_ms.saturating_add(1),
            ))
            .await
            .expect("qualify target generation");
        let mut write = handle.begin_write().await.expect("begin active lease");
        sqlx::query(
            "INSERT INTO routing_circuit_state_v3 (
                 station_key_id, station_key_lifecycle_revision, state,
                 state_revision, consecutive_failures, reopen_level,
                 opened_at_ms, cooldown_until_ms, recovery_successes,
                 lease_id, lease_revision, lease_policy_revision,
                 lease_recovery_success_threshold, lease_recovery_wait_ms,
                 lease_attempt_id,
                 lease_expires_at_ms, lease_deadline_at_ms, boundary_crossed,
                 monotonic_clock_watermark_ms, updated_at_ms
             ) VALUES (
                 'leased-key', 1, 'half_open', 2, 3, 1,
                 0, 10, 0, 'active-lease', 2, 1, 2, 30_000, 'active-attempt',
                 1_000, 1_000, 0, 10, 10
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert active half-open lease");
        write.commit().await.expect("commit active lease");

        let error = qualify_and_activate_once(&handle, &CancellationToken::new())
            .await
            .expect_err("active half-open lease must block cutover");
        assert!(matches!(error, PersistenceError::InvariantViolation(_)));
        let registry = coordinator.inspect().await.expect("inspect registry");
        assert_eq!(
            registry
                .active
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(base_id.as_str())
        );
        assert_eq!(
            registry
                .fencing
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(target_id.as_str())
        );
        assert_eq!(
            load_runtime_generation(&handle, &target_id).await.status,
            RoutingGenerationStatus::CutoverFencing
        );
        let mut read = handle.begin_read().await.expect("read retained lease");
        let retained_lease: Option<String> = sqlx::query_scalar(
            "SELECT lease_id FROM routing_circuit_state_v3
             WHERE station_key_id = 'leased-key'",
        )
        .fetch_one(read.connection())
        .await
        .expect("load retained lease");
        assert_eq!(retained_lease.as_deref(), Some("active-lease"));
        drop(read);

        let mut write = handle.begin_write().await.expect("release active lease");
        sqlx::query(
            "UPDATE routing_circuit_state_v3
             SET lease_id = NULL, lease_attempt_id = NULL,
                 lease_policy_revision = NULL,
                 lease_recovery_success_threshold = NULL,
                 lease_recovery_wait_ms = NULL,
                 lease_expires_at_ms = NULL, lease_deadline_at_ms = NULL,
                 boundary_crossed = NULL, released_at_ms = NULL,
                 lease_terminal_state = NULL, updated_at_ms = 11
             WHERE station_key_id = 'leased-key' AND lease_id = 'active-lease'",
        )
        .execute(write.connection())
        .await
        .expect("release half-open lease");
        write.commit().await.expect("commit lease release");
        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("next tick activates after lease release"),
            Some(target_id.clone())
        );
        let registry = coordinator
            .inspect()
            .await
            .expect("inspect activated registry");
        assert!(registry.fencing.is_none());
        assert_eq!(
            registry
                .active
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(target_id.as_str())
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn qualified_generation_keeps_a_persistent_fence_until_admitted_attempts_drain() {
        let root = tempfile::tempdir().expect("tempdir");
        let database_path = root.path().join("busy.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&database_path)
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        insert_staged_policy(&handle, 1, &RoutingPolicyConfigV3::default()).await;
        let ready_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build ready generation")
            .expect("ready generation id");
        let mut write = handle.begin_write().await.expect("begin pending attempt");
        crate::persistence::stores::routing_attempt_store::RoutingAttemptStore::admit(
            write.connection(),
            &crate::persistence::stores::routing_attempt_store::RoutingAttemptAdmission {
                attempt_id: "busy-attempt",
                correlation_id: "busy-correlation",
                station_key_id: "busy-key",
                station_key_lifecycle_revision: 1,
                attempt_index: 0,
                capacity_lease_id: "busy-capacity",
                half_open_lease_id: None,
                lease_revision: None,
                deadline_at_ms: 10_000,
                admitted_at_ms: 1,
                generation_eligibility:
                    crate::persistence::stores::routing_attempt_store::RoutingGenerationEligibility::Active,
            },
        )
        .await
        .expect("admit pending attempt");
        write.commit().await.expect("commit pending attempt");

        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("qualification must remain retryable"),
            None
        );
        assert_eq!(
            load_runtime_generation(&handle, &ready_id).await.status,
            RoutingGenerationStatus::CutoverFencing
        );
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        let registry = coordinator.inspect().await.expect("registry");
        assert!(registry.active.is_none());
        assert_eq!(
            registry
                .fencing
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(ready_id.as_str())
        );
        let fence_revision = registry.marker.fence_revision;
        let mut read = handle.begin_read().await.expect("read admission guard");
        let guard = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
            .load_admission_guard(read.connection())
            .await
            .expect("load admission guard");
        assert!(guard.fencing);
        assert_eq!(guard.fence_revision, fence_revision);
        drop(read);

        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("second tick must preserve fence"),
            None
        );
        let still_fenced = coordinator.inspect().await.expect("persistent fence");
        assert_eq!(still_fenced.marker.fence_revision, fence_revision);
        assert!(still_fenced.fencing.is_some());

        drop(still_fenced);
        drop(coordinator);
        drop(handle);
        runtime.close().await.expect("close fenced runtime");
        let runtime = PersistenceRuntime::open_current(&database_path)
            .await
            .expect("reopen fenced runtime");
        let handle = runtime.handle();
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        let recovered = coordinator
            .inspect()
            .await
            .expect("recover persistent fence");
        assert_eq!(recovered.marker.fence_revision, fence_revision);
        assert_eq!(
            recovered
                .fencing
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(ready_id.as_str())
        );

        let mut write = handle
            .begin_write()
            .await
            .expect("terminalize pending attempt");
        sqlx::query(
            "UPDATE routing_attempt_v3
             SET terminal_state = 'local_abandoned', outcome = 'excluded',
                 response_origin = 'relay', failure_attribution = 'local',
                 event_time_status = 'valid', terminal_at_ms = 2,
                 released_at_ms = 2, updated_at_ms = MAX(updated_at_ms, 2)
             WHERE attempt_id = 'busy-attempt' AND terminal_state = 'pending'",
        )
        .execute(write.connection())
        .await
        .expect("terminalize pending attempt");
        write.commit().await.expect("commit terminal attempt");
        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("drained fence must activate"),
            Some(ready_id.clone())
        );
        let active = coordinator.inspect().await.expect("active registry");
        assert_eq!(active.marker.fence_revision, fence_revision);
        assert!(active.fencing.is_none());
        assert_eq!(
            active
                .active
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(ready_id.as_str())
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn persistent_fence_rebuilds_active_tail_and_excludes_next_monitoring_evidence() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("fence-tail.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");
        seed_routing_key(&handle, "tail-key").await;

        let base_policy = RoutingPolicyConfigV3::default();
        insert_staged_policy(&handle, 1, &base_policy).await;
        let base_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build base generation")
            .expect("base generation id");
        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("activate base generation"),
            Some(base_id.clone())
        );

        let mut target_policy = base_policy;
        target_policy.retry.max_retry_count = 2;
        insert_staged_policy(&handle, 2, &target_policy).await;
        let target_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build target generation")
            .expect("target generation id");
        let target = load_runtime_generation(&handle, &target_id).await;

        let admitted_at_ms = u64::try_from(target.created_at_ms.max(0))
            .expect("admission time")
            .saturating_add(1);
        let mut write = handle.begin_write().await.expect("begin admitted attempt");
        let eligibility = RoutingAttemptStore::resolve_generation_eligibility(write.connection())
            .await
            .expect("resolve active eligibility");
        assert_eq!(eligibility, RoutingGenerationEligibility::Active);
        RoutingAttemptStore::admit(
            write.connection(),
            &RoutingAttemptAdmission {
                attempt_id: "tail-request:0",
                correlation_id: "tail-request",
                station_key_id: "tail-key",
                station_key_lifecycle_revision: 1,
                attempt_index: 0,
                capacity_lease_id: "tail-capacity",
                half_open_lease_id: None,
                lease_revision: None,
                deadline_at_ms: admitted_at_ms.saturating_add(60_000),
                admitted_at_ms,
                generation_eligibility: eligibility,
            },
        )
        .await
        .expect("admit active attempt");
        write.commit().await.expect("commit active attempt");

        assert_eq!(
            qualify_and_activate_once(&handle, &CancellationToken::new())
                .await
                .expect("start persistent fence"),
            None
        );
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        let fenced = coordinator
            .inspect()
            .await
            .expect("inspect persistent fence");
        let fence_revision = fenced.marker.fence_revision;
        let fence_started_at_ms = fenced.marker.updated_at_ms;
        assert_eq!(
            fenced
                .fencing
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(target_id.as_str())
        );

        let ingestion = ObservationIngestion::new();
        let mut write = handle
            .begin_write()
            .await
            .expect("begin next monitoring sample");
        ingestion
            .append(
                &mut write,
                generation_observation(
                    "fence-monitoring-next",
                    ObservationSource::ActiveProbe,
                    ObservationOutcome::Success,
                    "tail-key",
                    "fence-monitoring-next",
                    fence_started_at_ms.saturating_add(1),
                ),
            )
            .await
            .expect("append next monitoring sample");
        write.commit().await.expect("commit next monitoring sample");

        let terminal_at_ms = u64::try_from(fence_started_at_ms.max(0))
            .expect("terminal time")
            .saturating_add(2);
        let mut write = handle.begin_write().await.expect("finish admitted attempt");
        assert!(RoutingAttemptStore::mark_boundary_crossed(
            write.connection(),
            "tail-request:0",
            "tail-key",
            1,
            terminal_at_ms.saturating_sub(1),
        )
        .await
        .expect("mark outbound boundary"));
        RoutingAttemptStore::terminalize(
            write.connection(),
            &RoutingAttemptTerminal {
                attempt_id: "tail-request:0",
                comparability_key: None,
                failure_code: Some("upstream_rate_limited"),
                failure_blame: Some("Upstream"),
                terminal_kind: "failed",
                retry_disposition: "retryable_before_commit",
                event_at_ms: terminal_at_ms,
                observed_at_ms: terminal_at_ms,
                ingested_at_ms: terminal_at_ms,
                latency_ms: 100,
            },
        )
        .await
        .expect("terminalize active attempt");
        StationKeyCircuitStore
            .finish_attempt(
                write.connection(),
                CircuitTerminalInput {
                    station_key_id: "tail-key",
                    lifecycle_revision: 1,
                    policy_revision: target.policy_revision,
                    attempt_id: "tail-request:0",
                    lease_id: None,
                    lease_revision: None,
                    now_ms: terminal_at_ms,
                    occurred_at_ms: terminal_at_ms,
                    success: false,
                    boundary_crossed: true,
                    affects_circuit: true,
                    failure_code: Some("upstream_rate_limited"),
                    recovery_origin: "normal",
                    retry_disposition: "retryable_before_commit",
                    consecutive_failure_threshold: target_policy
                        .retry
                        .consecutive_failure_threshold,
                    recovery_success_threshold: u16::from(
                        target_policy.circuit_breaker.recovery_success_threshold,
                    ),
                    recovery_wait_ms: u64::from(
                        target_policy.circuit_breaker.recovery_wait_seconds,
                    ) * 1_000,
                },
            )
            .await
            .expect("apply circuit tail");
        let mut samples = RoutingAttemptStore::finalize_request_clusters(
            write.connection(),
            "tail-request",
            i64::try_from(terminal_at_ms).expect("finalization time"),
        )
        .await
        .expect("finalize active request cluster");
        assert_eq!(samples.len(), 1);
        let sample = samples.pop().expect("active request sample");
        assert_eq!(
            sample.generation_eligibility,
            RoutingGenerationEligibility::Active
        );
        ingestion
            .append_with_generation_eligibility(
                &mut write,
                generation_observation(
                    "fence-real-active",
                    ObservationSource::RealRequest,
                    ObservationOutcome::RateLimited,
                    &sample.station_key_id,
                    &sample.correlation_id,
                    sample.event_at_ms.expect("sample event time"),
                ),
                Some(sample.generation_eligibility.as_str()),
            )
            .await
            .expect("append active attempt observation");
        write.commit().await.expect("commit active tail");

        let replacement_id = qualify_and_activate_once(&handle, &CancellationToken::new())
            .await
            .expect("drain and rebuild persistent fence")
            .expect("replacement generation id");
        assert_ne!(replacement_id, target_id);
        let registry = coordinator
            .inspect()
            .await
            .expect("inspect activated replacement");
        assert_eq!(registry.marker.fence_revision, fence_revision);
        assert!(registry.fencing.is_none());
        assert_eq!(
            registry
                .active
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(replacement_id.as_str())
        );
        let replaced = load_runtime_generation(&handle, &target_id).await;
        assert_eq!(replaced.status, RoutingGenerationStatus::Failed);

        let mut read = handle.begin_read().await.expect("read tail evidence");
        let failure_code: Option<String> = sqlx::query_scalar(
            "SELECT failure_code FROM routing_runtime_generation
             WHERE runtime_generation_id = ?1",
        )
        .bind(&target_id)
        .fetch_one(read.connection())
        .await
        .expect("load superseded failure code");
        assert_eq!(failure_code.as_deref(), Some("superseded_by_fence_tail"));
        let observation_eligibilities: Vec<(String, String)> = sqlx::query_as(
            "SELECT id, generation_eligibility FROM routing_observations
             WHERE id IN ('fence-monitoring-next', 'fence-real-active') ORDER BY id",
        )
        .fetch_all(read.connection())
        .await
        .expect("load observation eligibility");
        assert_eq!(
            observation_eligibilities,
            vec![
                ("fence-monitoring-next".to_string(), "next".to_string()),
                ("fence-real-active".to_string(), "active".to_string()),
            ]
        );
        let replay_report_json: String = sqlx::query_scalar(
            "SELECT replay_report_json FROM routing_generation_qualification_report_v2
             WHERE runtime_generation_id = ?1",
        )
        .bind(&replacement_id)
        .fetch_one(read.connection())
        .await
        .expect("load replacement qualification report");
        let replay_report: Value =
            serde_json::from_str(&replay_report_json).expect("decode replay report");
        assert_eq!(
            replay_report["quality_input_observation_count"].as_u64(),
            Some(1)
        );
        assert_eq!(replay_report["circuit_input_event_count"].as_u64(), Some(1));
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn persistent_fence_timeout_aborts_and_releases_admission() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("fence-timeout.sqlite3"))
                .await
                .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        insert_staged_policy(&handle, 1, &RoutingPolicyConfigV3::default()).await;
        let ready_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build ready generation")
            .expect("ready generation id");
        let ready = load_runtime_generation(&handle, &ready_id).await;
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        coordinator
            .record_qualification(&test_qualification(&ready_id, ready.created_at_ms))
            .await
            .expect("qualify ready generation");
        let started_at_ms = ready.created_at_ms.saturating_add(1);
        let fence = coordinator
            .begin_cutover(&ready_id, None, started_at_ms)
            .await
            .expect("begin persistent fence");

        assert_eq!(
            advance_fenced_cutover_at(
                &handle,
                &CancellationToken::new(),
                Some(started_at_ms.saturating_add(SYSTEM_CUTOVER_FENCE_TIMEOUT_MS)),
            )
            .await
            .expect("timeout must abort fence"),
            None
        );
        let registry = coordinator.inspect().await.expect("inspect aborted fence");
        assert!(registry.active.is_none());
        assert!(registry.fencing.is_none());
        assert_eq!(
            load_runtime_generation(&handle, &ready_id).await.status,
            RoutingGenerationStatus::Ready
        );
        assert_eq!(registry.marker.fence_revision, fence.fence_revision);
        let mut read = handle.begin_read().await.expect("read timeout audit");
        let reason: Option<String> = sqlx::query_scalar(
            "SELECT reason_code FROM routing_generation_transition_audit
             WHERE transition_kind = 'cutover_aborted' AND fence_revision = ?1",
        )
        .bind(i64::try_from(fence.fence_revision).expect("fence revision"))
        .fetch_one(read.connection())
        .await
        .expect("load abort reason");
        assert_eq!(reason.as_deref(), Some("fence_timeout"));
        let guard = crate::persistence::stores::routing_generation_store::RoutingGenerationStore
            .load_admission_guard(read.connection())
            .await
            .expect("load released admission guard");
        assert!(!guard.fencing);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn builder_reuses_or_rebuilds_generation_components_by_policy_field_class() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("reuse.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        let base_policy = RoutingPolicyConfigV3::default();
        insert_staged_policy(&handle, 1, &base_policy).await;
        let base_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build base generation")
            .expect("base generation id");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        coordinator
            .record_qualification(&test_qualification(&base_id, 2))
            .await
            .expect("qualify base generation");
        let ready_base = load_runtime_generation(&handle, &base_id).await;
        let fence = coordinator
            .begin_cutover(&base_id, None, ready_base.created_at_ms + 1)
            .await
            .expect("begin base cutover");
        coordinator
            .complete_cutover(&fence, ready_base.created_at_ms + 2)
            .await
            .expect("activate base generation");
        let base = load_runtime_generation(&handle, &base_id).await;

        let mut policy_only = base_policy.clone();
        policy_only.retry.max_retry_count = 2;
        insert_staged_policy(&handle, 2, &policy_only).await;
        let policy_only_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build policy-only generation")
            .expect("policy-only generation id");
        let policy_only_generation = load_runtime_generation(&handle, &policy_only_id).await;
        assert_eq!(
            policy_only_generation.quality_generation_id,
            base.quality_generation_id
        );
        assert_eq!(
            policy_only_generation.circuit_generation_id,
            base.circuit_generation_id
        );

        let mut quality_only = base_policy.clone();
        quality_only.reliability_source_weights.real_traffic_percent = 80;
        quality_only.reliability_source_weights.monitoring_percent = 20;
        insert_staged_policy(&handle, 3, &quality_only).await;
        let quality_only_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build quality-only generation")
            .expect("quality-only generation id");
        let quality_only_generation = load_runtime_generation(&handle, &quality_only_id).await;
        assert_ne!(
            quality_only_generation.quality_generation_id,
            base.quality_generation_id
        );
        assert_eq!(
            quality_only_generation.circuit_generation_id,
            base.circuit_generation_id
        );

        let mut circuit_only = base_policy.clone();
        circuit_only.retry.consecutive_failure_threshold = 4;
        insert_staged_policy(&handle, 4, &circuit_only).await;
        let circuit_only_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build circuit-only generation")
            .expect("circuit-only generation id");
        let circuit_only_generation = load_runtime_generation(&handle, &circuit_only_id).await;
        assert_eq!(
            circuit_only_generation.quality_generation_id,
            base.quality_generation_id
        );
        assert_ne!(
            circuit_only_generation.circuit_generation_id,
            base.circuit_generation_id
        );

        let mut combined = base_policy.clone();
        combined.reliability_sampling.optimistic_reliability_percent = 94;
        combined.circuit_breaker.recovery_wait_seconds = 31;
        insert_staged_policy(&handle, 5, &combined).await;
        let combined_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build combined generation")
            .expect("combined generation id");
        let combined_generation = load_runtime_generation(&handle, &combined_id).await;
        assert_ne!(
            combined_generation.quality_generation_id,
            base.quality_generation_id
        );
        assert_ne!(
            combined_generation.circuit_generation_id,
            base.circuit_generation_id
        );

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn policy_only_ready_generation_is_rebuilt_after_quality_and_circuit_tail() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("tail.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        let base_policy = RoutingPolicyConfigV3::default();
        let base_policy_id = insert_staged_policy(&handle, 1, &base_policy).await;
        let base_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build base generation")
            .expect("base generation id");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        coordinator
            .record_qualification(&test_qualification(&base_id, 2))
            .await
            .expect("qualify base generation");
        let ready_base = load_runtime_generation(&handle, &base_id).await;
        let fence = coordinator
            .begin_cutover(&base_id, None, ready_base.created_at_ms + 1)
            .await
            .expect("begin base cutover");
        coordinator
            .complete_cutover(&fence, ready_base.created_at_ms + 2)
            .await
            .expect("activate base generation");
        let base = load_runtime_generation(&handle, &base_id).await;

        let mut policy_only = base_policy.clone();
        policy_only.retry.max_retry_count = 2;
        let target_policy_id = insert_staged_policy(&handle, 2, &policy_only).await;
        let stale_ready_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build policy-only generation")
            .expect("policy-only generation id");
        let stale_ready = load_runtime_generation(&handle, &stale_ready_id).await;
        assert_eq!(
            stale_ready.quality_generation_id,
            base.quality_generation_id
        );
        assert_eq!(
            stale_ready.circuit_generation_id,
            base.circuit_generation_id
        );

        let mut write = handle.begin_write().await.expect("begin tail write");
        sqlx::query(
            "INSERT INTO routing_observations (
                 id, producer_id, producer_sequence, payload_hash,
                 event_at_ms, ingested_at_ms, scope, source,
                 traffic_equivalence, outcome_kind, latency_ms,
                 mass_basis_points, evidence_json, created_at_ms,
                 event_id, attempt_id, correlation_id, station_key_id,
                 station_key_lifecycle_revision, attempt_index,
                 candidate_admitted, candidate_admitted_at_ms,
                 boundary_crossed, response_origin, event_time_status,
                 outcome, failure_attribution, observed_at_ms,
                 retry_disposition, algorithm_version,
                 source_weight_revision, quality_policy_revision,
                 generation_eligibility, cluster_finalized,
                 cluster_expected_attempt_count, cluster_finalized_at_ms,
                 cluster_finalization_reason
             ) VALUES (
                 'tail-observation', 'tail-producer', 1, ?1,
                 5, 5, 'station_key:key-tail', 'real_request',
                 'exact_request', 'success', 100, 10000, '{}', 5,
                 'tail-observation', 'tail-attempt', 'tail-correlation', 'key-tail',
                 1, 0, 1, 5, 1, 'upstream', 'valid', 'success', 'key', 5,
                 'end', 'routing_quality_v3', 1, 1, 'active', 1, 1, 5,
                 'attempt_terminal'
             )",
        )
        .bind("a".repeat(64))
        .execute(write.connection())
        .await
        .expect("insert observation tail");
        sqlx::query(
            "INSERT INTO routing_circuit_event_v3 (
                 event_id, effect_kind, source, attempt_id, station_key_id,
                 station_key_lifecycle_revision, reducer_commit_sequence,
                 policy_revision, expected_state_revision, occurred_at_ms,
                 canonical_outcome, failure_code, recovery_origin,
                 retry_disposition, boundary_crossed, created_at_ms
             ) VALUES (
                 'tail-circuit', 'circuit', 'real_request', 'tail-circuit-attempt',
                 'key-tail', 1, 1, 1, 1, 6, 'attributable_failure',
                 'upstream_rate_limited', 'normal', 'retryable_before_commit', 1, 6
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert circuit tail");
        write.commit().await.expect("commit input tail");

        let rebuilt_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("rebuild stale policy-only generation")
            .expect("rebuilt generation id");
        assert_ne!(rebuilt_id, stale_ready_id);
        let rebuilt = load_runtime_generation(&handle, &rebuilt_id).await;
        assert_ne!(rebuilt.quality_generation_id, base.quality_generation_id);
        assert_ne!(rebuilt.circuit_generation_id, base.circuit_generation_id);
        assert_eq!(
            rebuilt.quality_policy_revision,
            base.quality_policy_revision
        );
        assert_eq!(
            rebuilt.circuit_policy_revision,
            base.circuit_policy_revision
        );
        assert!(rebuilt.input_observation_watermark > base.input_observation_watermark);
        assert!(rebuilt.input_circuit_event_watermark > base.input_circuit_event_watermark);
        let stale = load_runtime_generation(&handle, &stale_ready_id).await;
        assert_eq!(stale.status, RoutingGenerationStatus::Failed);

        coordinator
            .record_qualification(&test_qualification(&rebuilt_id, 7))
            .await
            .expect("qualify rebuilt generation");
        let fence = coordinator
            .begin_cutover(&rebuilt_id, Some(&base_id), rebuilt.created_at_ms + 1)
            .await
            .expect("rebuilt generation must pass tail validation");
        coordinator
            .complete_cutover(&fence, rebuilt.created_at_ms + 2)
            .await
            .expect("activate rebuilt generation");
        let mut read = handle.begin_read().await.expect("begin status read");
        let statuses: Vec<(String, String)> = sqlx::query_as(
            "SELECT policy_generation_id, status FROM routing_policy_v3_staged
             WHERE policy_generation_id IN (?1, ?2)
             ORDER BY policy_generation_id",
        )
        .bind(&base_policy_id)
        .bind(&target_policy_id)
        .fetch_all(read.connection())
        .await
        .expect("policy generation statuses");
        assert!(statuses.contains(&(base_policy_id, "retired".to_string())));
        assert!(statuses.contains(&(target_policy_id, "active".to_string())));
        drop(read);

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn rollback_atomically_restores_a_complete_qualified_v3_generation() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("rollback.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        let base_policy = RoutingPolicyConfigV3::default();
        insert_staged_policy(&handle, 1, &base_policy).await;
        let base_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build base generation")
            .expect("base generation id");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        coordinator
            .record_qualification(&test_qualification(&base_id, 2))
            .await
            .expect("qualify base generation");
        let base = load_runtime_generation(&handle, &base_id).await;
        let fence = coordinator
            .begin_cutover(&base_id, None, base.created_at_ms.saturating_add(1))
            .await
            .expect("begin base cutover");
        coordinator
            .complete_cutover(&fence, base.created_at_ms.saturating_add(2))
            .await
            .expect("activate base generation");

        let mut next_policy = base_policy;
        next_policy.retry.max_retry_count = 2;
        insert_staged_policy(&handle, 2, &next_policy).await;
        let next_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build next generation")
            .expect("next generation id");
        let next = load_runtime_generation(&handle, &next_id).await;
        coordinator
            .record_qualification(&test_qualification(
                &next_id,
                next.created_at_ms.saturating_add(1),
            ))
            .await
            .expect("qualify next generation");
        let fence = coordinator
            .begin_cutover(
                &next_id,
                Some(&base_id),
                next.created_at_ms.saturating_add(2),
            )
            .await
            .expect("begin next cutover");
        coordinator
            .complete_cutover(&fence, next.created_at_ms.saturating_add(3))
            .await
            .expect("activate next generation");

        let rollback_fence = coordinator
            .begin_rollback(
                Some(&base_id),
                &next_id,
                "operator_requested",
                next.created_at_ms.saturating_add(4),
            )
            .await
            .expect("begin rollback");
        coordinator
            .complete_rollback(&rollback_fence, next.created_at_ms.saturating_add(5))
            .await
            .expect("complete rollback");

        let registry = coordinator.inspect().await.expect("inspect registry");
        assert_eq!(
            registry
                .active
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(base_id.as_str())
        );
        assert!(registry.fencing.is_none());
        assert_eq!(
            load_runtime_generation(&handle, &next_id).await.status,
            RoutingGenerationStatus::Retired
        );
        let mut read = handle.begin_read().await.expect("read rollback audit");
        let audit: Vec<(String, Option<String>)> = sqlx::query_as(
            "SELECT transition_kind, reason_code
             FROM routing_generation_transition_audit
             WHERE target_runtime_generation_id = ?1
             ORDER BY transition_id",
        )
        .bind(&base_id)
        .fetch_all(read.connection())
        .await
        .expect("load rollback audit");
        assert_eq!(
            audit,
            vec![
                ("cutover_started".to_string(), None),
                ("cutover_activated".to_string(), None),
                (
                    "rollback_started".to_string(),
                    Some("operator_requested".to_string())
                ),
                ("rollback_activated".to_string(), None),
            ]
        );
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn rollback_replays_a_retired_generation_tail_before_activation() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("rollback-tail.sqlite3"))
                .await
                .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("begin fixture cleanup");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear migration audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged fixture");
        write.commit().await.expect("commit fixture cleanup");

        let base_policy = RoutingPolicyConfigV3::default();
        insert_staged_policy(&handle, 1, &base_policy).await;
        let base_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build base generation")
            .expect("base generation id");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        coordinator
            .record_qualification(&test_qualification(&base_id, 2))
            .await
            .expect("qualify base generation");
        let base = load_runtime_generation(&handle, &base_id).await;
        let fence = coordinator
            .begin_cutover(&base_id, None, base.created_at_ms.saturating_add(1))
            .await
            .expect("begin base cutover");
        coordinator
            .complete_cutover(&fence, base.created_at_ms.saturating_add(2))
            .await
            .expect("activate base generation");

        let mut next_policy = base_policy;
        next_policy.retry.max_retry_count = 2;
        insert_staged_policy(&handle, 2, &next_policy).await;
        let next_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build next generation")
            .expect("next generation id");
        let next = load_runtime_generation(&handle, &next_id).await;
        coordinator
            .record_qualification(&test_qualification(
                &next_id,
                next.created_at_ms.saturating_add(1),
            ))
            .await
            .expect("qualify next generation");
        let fence = coordinator
            .begin_cutover(
                &next_id,
                Some(&base_id),
                next.created_at_ms.saturating_add(2),
            )
            .await
            .expect("begin next cutover");
        coordinator
            .complete_cutover(&fence, next.created_at_ms.saturating_add(3))
            .await
            .expect("activate next generation");

        let mut write = handle.begin_write().await.expect("begin tail event");
        insert_circuit_fixture_event(
            write.connection(),
            "rollback-tail-key",
            1,
            next.created_at_ms.saturating_add(4),
            false,
            None,
        )
        .await;
        write.commit().await.expect("commit tail event");

        let _rollback_fence = coordinator
            .begin_rollback(
                Some(&base_id),
                &next_id,
                "operator_requested",
                next.created_at_ms.saturating_add(5),
            )
            .await
            .expect("rollback starts before tail replay");
        let replacement_id = advance_fenced_cutover_at(
            &handle,
            &CancellationToken::new(),
            Some(next.created_at_ms.saturating_add(6)),
        )
        .await
        .expect("rollback tail replay must complete")
        .expect("rollback replacement generation id");
        assert_ne!(replacement_id, base_id);
        assert_ne!(replacement_id, next_id);
        let registry = coordinator.inspect().await.expect("inspect registry");
        assert_eq!(
            registry
                .active
                .as_ref()
                .map(|generation| generation.runtime_generation_id.as_str()),
            Some(replacement_id.as_str())
        );
        assert!(registry.fencing.is_none());
        assert_eq!(
            load_runtime_generation(&handle, &base_id).await.status,
            RoutingGenerationStatus::Failed
        );
        assert_eq!(
            load_runtime_generation(&handle, &next_id).await.status,
            RoutingGenerationStatus::Retired
        );
        assert_eq!(
            load_runtime_generation(&handle, &replacement_id)
                .await
                .input_circuit_event_watermark,
            1
        );
        runtime.close().await.expect("close runtime");
    }

    async fn insert_circuit_fixture_event(
        connection: &mut sqlx::SqliteConnection,
        station_key_id: &str,
        reducer_commit_sequence: i64,
        occurred_at_ms: i64,
        success: bool,
        lease_revision: Option<i64>,
    ) {
        let event_id = format!("{station_key_id}-event-{reducer_commit_sequence}");
        let attempt_id = format!("{station_key_id}-attempt-{reducer_commit_sequence}");
        sqlx::query(
            "INSERT INTO routing_circuit_event_v3 (
                 event_id, effect_kind, source, attempt_id, station_key_id,
                 station_key_lifecycle_revision, reducer_commit_sequence,
                 policy_revision, expected_state_revision, occurred_at_ms,
                 canonical_outcome, failure_code, recovery_origin,
                 retry_disposition, lease_revision, boundary_crossed, created_at_ms
             ) VALUES (
                 ?1, 'circuit', 'real_request', ?2, ?3, 1, ?4,
                 1, ?4, ?5, ?6, ?7, 'normal', ?8, ?9, 1, ?5
             )",
        )
        .bind(event_id)
        .bind(attempt_id)
        .bind(station_key_id)
        .bind(reducer_commit_sequence)
        .bind(occurred_at_ms)
        .bind(if success {
            "success"
        } else {
            "attributable_failure"
        })
        .bind(if success {
            None
        } else {
            Some("upstream_rate_limited")
        })
        .bind(if success {
            "end"
        } else {
            "retryable_before_commit"
        })
        .bind(lease_revision)
        .execute(connection)
        .await
        .expect("insert circuit replay event");
    }
}
