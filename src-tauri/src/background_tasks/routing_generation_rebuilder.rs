use std::collections::BTreeMap;

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::Row;
use tokio_util::sync::CancellationToken;

#[cfg(test)]
use std::sync::{Mutex, OnceLock};

use crate::{
    application::{
        quality_projection::{
            rebuild_quality_summary_v3_at, QualityProjectionConfig, QualitySummary,
            QUALITY_PROJECTOR_VERSION,
        },
        routing_generation::{circuit_generation_id, quality_generation_id, sha256_hex},
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::{
            routing_observation_store::RoutingObservationStore,
            routing_quality_store::RoutingQualityStore,
        },
    },
};

pub(crate) const ROUTING_GENERATION_REBUILD_BATCH_SIZE: usize = 64;
const QUALITY_GENERATION_SCOPE: &str = "station_key";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QualityGenerationBuildRequest {
    pub(crate) input_observation_watermark: u64,
    pub(crate) next_observation_watermark: u64,
    pub(crate) evaluation_at_ms: i64,
    pub(crate) config: QualityProjectionConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QualityGenerationBuildResult {
    pub(crate) quality_generation_id: String,
    pub(crate) input_observation_hash: String,
    pub(crate) output_content_hash: String,
    pub(crate) checkpoint_ref: String,
    pub(crate) processed_scope_count: u64,
    pub(crate) complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct QualityGenerationVerification {
    pub(crate) input_observation_count: u64,
    pub(crate) output_scope_count: u64,
}

pub(crate) const CIRCUIT_REBUILD_ALGORITHM_VERSION: &str = "station-key-circuit-v3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CircuitGenerationBuildRequest {
    pub(crate) input_circuit_event_watermark: u64,
    pub(crate) circuit_policy_revision: u64,
    pub(crate) consecutive_failure_threshold: u16,
    pub(crate) recovery_success_threshold: u16,
    pub(crate) recovery_wait_ms: u64,
    pub(crate) max_cooldown_ms: u64,
    pub(crate) evaluation_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CircuitGenerationBuildResult {
    pub(crate) circuit_generation_id: String,
    pub(crate) input_circuit_event_hash: String,
    pub(crate) output_content_hash: String,
    pub(crate) checkpoint_ref: String,
    pub(crate) processed_event_count: u64,
    pub(crate) complete: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CircuitGenerationVerification {
    pub(crate) input_event_count: u64,
    pub(crate) output_state_count: u64,
}

#[derive(Debug, Clone)]
struct PreparedQualityOutput {
    quality_generation_id: String,
    source_profile_snapshot_id: String,
    input_observation_hash: String,
    output_content_hash: String,
    checkpoint_ref: String,
    input_observation_count: u64,
    output_scope_count: u64,
    build_request_hash: String,
}

#[derive(Debug, Clone)]
struct PreparedCircuitOutput {
    circuit_generation_id: String,
    input_circuit_event_hash: String,
    output_content_hash: String,
    checkpoint_ref: String,
    input_event_count: u64,
    output_state_count: u64,
}

#[derive(Debug, Clone, Serialize)]
struct CircuitReplayEvent {
    event_id: String,
    effect_kind: String,
    station_key_id: String,
    station_key_lifecycle_revision: u64,
    reducer_commit_sequence: u64,
    ingestion_sequence: u64,
    occurred_at_ms: u64,
    canonical_outcome: String,
    failure_code: Option<String>,
    lease_revision: Option<u64>,
    boundary_crossed: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
struct CircuitGenerationState {
    station_key_id: String,
    station_key_lifecycle_revision: u64,
    state: String,
    state_revision: u64,
    consecutive_failures: u16,
    reopen_level: u32,
    opened_at_ms: Option<u64>,
    cooldown_until_ms: Option<u64>,
    recovery_successes: u16,
    monotonic_clock_watermark_ms: u64,
    reducer_commit_sequence: u64,
}

#[derive(Debug, Clone, Serialize)]
struct GenerationQualitySummary {
    station_key_id: String,
    station_key_lifecycle_revision: u64,
    #[serde(skip)]
    input_observation_count: u64,
    #[serde(skip)]
    last_observation_id: Option<String>,
    summary: QualitySummary,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CircuitEventCursor {
    station_key_id: String,
    station_key_lifecycle_revision: u64,
    reducer_commit_sequence: u64,
    event_id: String,
    effect_kind: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct QualityCheckpoint {
    cursor_station_key_id: Option<String>,
    cursor_observation_id: Option<String>,
    processed_scope_count: u64,
    processed_observation_count: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct CircuitCheckpoint {
    cursor: Option<CircuitEventCursor>,
    processed_event_count: u64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingGenerationRebuilder;

impl RoutingGenerationRebuilder {
    pub(crate) async fn verify_quality_generation(
        &self,
        runtime: &PersistenceHandle,
        request: &QualityGenerationBuildRequest,
        expected: &QualityGenerationBuildResult,
    ) -> Result<QualityGenerationVerification, PersistenceError> {
        validate_quality_request(request)?;
        let prepared = prepare_quality_output(runtime, request).await?;
        let matches = prepared.quality_generation_id == expected.quality_generation_id
            && prepared.input_observation_hash == expected.input_observation_hash
            && prepared.output_content_hash == expected.output_content_hash
            && prepared.checkpoint_ref == expected.checkpoint_ref;
        if !matches {
            return Err(PersistenceError::InvariantViolation(
                "quality generation deterministic replay did not match".into(),
            ));
        }
        finalize_quality_generation(runtime, request, &prepared).await?;
        Ok(QualityGenerationVerification {
            input_observation_count: prepared.input_observation_count,
            output_scope_count: prepared.output_scope_count,
        })
    }

    pub(crate) async fn verify_circuit_generation(
        &self,
        runtime: &PersistenceHandle,
        request: &CircuitGenerationBuildRequest,
        expected: &CircuitGenerationBuildResult,
    ) -> Result<CircuitGenerationVerification, PersistenceError> {
        validate_circuit_request(request)?;
        let prepared = prepare_circuit_output(runtime, request).await?;
        let matches = prepared.circuit_generation_id == expected.circuit_generation_id
            && prepared.input_circuit_event_hash == expected.input_circuit_event_hash
            && prepared.output_content_hash == expected.output_content_hash
            && prepared.checkpoint_ref == expected.checkpoint_ref;
        if !matches {
            return Err(PersistenceError::InvariantViolation(
                "circuit generation deterministic replay did not match".into(),
            ));
        }
        finalize_circuit_generation(runtime, request, &prepared).await?;
        Ok(CircuitGenerationVerification {
            input_event_count: prepared.input_event_count,
            output_state_count: prepared.output_state_count,
        })
    }

    pub(crate) async fn rebuild_quality_generation(
        &self,
        runtime: &PersistenceHandle,
        request: QualityGenerationBuildRequest,
        cancellation: &CancellationToken,
    ) -> Result<QualityGenerationBuildResult, PersistenceError> {
        validate_quality_request(&request)?;
        let build_request_hash = quality_build_request_hash(&request)?;
        let prepared =
            match load_prepared_quality_output(runtime, &request, &build_request_hash).await? {
                Some(prepared) => prepared,
                None => prepare_quality_output(runtime, &request).await?,
            };
        ensure_quality_generation(runtime, &request, &prepared).await?;

        loop {
            let checkpoint =
                load_quality_progress(runtime, &prepared.quality_generation_id).await?;
            if cancellation.is_cancelled() {
                return Ok(quality_result(
                    &prepared,
                    checkpoint.processed_scope_count,
                    false,
                ));
            }
            let batch = load_quality_summary_batch(
                runtime,
                &request,
                &prepared.source_profile_snapshot_id,
                checkpoint.cursor_station_key_id.as_deref(),
                ROUTING_GENERATION_REBUILD_BATCH_SIZE,
            )
            .await?;
            if batch.is_empty() {
                if checkpoint.processed_scope_count != prepared.output_scope_count
                    || checkpoint.processed_observation_count != prepared.input_observation_count
                {
                    return Err(PersistenceError::InvariantViolation(
                        "quality generation checkpoint is not contiguous".into(),
                    ));
                }
                finalize_quality_generation(runtime, &request, &prepared).await?;
                return Ok(quality_result(&prepared, prepared.output_scope_count, true));
            }
            let next_observation_count = checkpoint.processed_observation_count.saturating_add(
                batch
                    .iter()
                    .map(|output| output.input_observation_count)
                    .sum::<u64>(),
            );
            let mut write = runtime.begin_write().await?;
            let quality_store = RoutingQualityStore;
            for output in &batch {
                let summary_json = serde_json::to_value(&output.summary)
                    .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
                quality_store
                    .save_generation_summary(
                        write.connection(),
                        &prepared.quality_generation_id,
                        &output.summary.scope,
                        &output.station_key_id,
                        output.station_key_lifecycle_revision,
                        request.input_observation_watermark.max(1),
                        &summary_json,
                        request.evaluation_at_ms,
                    )
                    .await?;
                for (axis, value) in [
                    ("reliability", output.summary.reliability_basis_points),
                    ("latency", output.summary.responsiveness_basis_points),
                ] {
                    quality_store
                        .save_generation_health_axis(
                            write.connection(),
                            &prepared.quality_generation_id,
                            &output.summary.scope,
                            &output.station_key_id,
                            output.station_key_lifecycle_revision,
                            axis,
                            request.input_observation_watermark.max(1),
                            value,
                            request.evaluation_at_ms,
                        )
                        .await?;
                }
            }
            let cursor = batch.last().ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "quality generation batch cursor is missing".into(),
                )
            })?;
            let checkpoint_update = sqlx::query(
                "UPDATE routing_quality_generation_v3_checkpoint
                 SET cursor_station_key_id = ?2, cursor_observation_id = ?3,
                     processed_observation_count = ?4, updated_at_ms = ?5
                 WHERE quality_generation_id = ?1 AND status = 'building'
                   AND processed_observation_count = ?6
                   AND cursor_station_key_id IS ?7
                   AND cursor_observation_id IS ?8",
            )
            .bind(&prepared.quality_generation_id)
            .bind(&cursor.station_key_id)
            .bind(cursor.last_observation_id.as_deref())
            .bind(to_i64(next_observation_count)?)
            .bind(request.evaluation_at_ms)
            .bind(to_i64(checkpoint.processed_observation_count)?)
            .bind(checkpoint.cursor_station_key_id.as_deref())
            .bind(checkpoint.cursor_observation_id.as_deref())
            .execute(write.connection())
            .await?
            .rows_affected();
            let metadata_update = sqlx::query(
                "UPDATE routing_quality_generation_v3
                 SET cursor_station_key_id = ?2, cursor_observation_id = ?3,
                     processed_observation_count = ?4, updated_at_ms = ?5
                 WHERE quality_generation_id = ?1 AND status = 'building'
                   AND processed_observation_count = ?6
                   AND cursor_station_key_id IS ?7
                   AND cursor_observation_id IS ?8",
            )
            .bind(&prepared.quality_generation_id)
            .bind(&cursor.station_key_id)
            .bind(cursor.last_observation_id.as_deref())
            .bind(to_i64(next_observation_count)?)
            .bind(request.evaluation_at_ms)
            .bind(to_i64(checkpoint.processed_observation_count)?)
            .bind(checkpoint.cursor_station_key_id.as_deref())
            .bind(checkpoint.cursor_observation_id.as_deref())
            .execute(write.connection())
            .await?
            .rows_affected();
            if checkpoint_update != 1 || metadata_update != 1 {
                return Err(PersistenceError::RevisionConflict(
                    "routing_quality_generation_v3".into(),
                ));
            }
            write.commit().await?;
        }
    }

    pub(crate) async fn rebuild_circuit_generation(
        &self,
        runtime: &PersistenceHandle,
        request: CircuitGenerationBuildRequest,
        cancellation: &CancellationToken,
    ) -> Result<CircuitGenerationBuildResult, PersistenceError> {
        validate_circuit_request(&request)?;
        let prepared = prepare_circuit_output(runtime, &request).await?;
        ensure_circuit_generation(runtime, &request, &prepared).await?;

        loop {
            let checkpoint =
                load_circuit_progress(runtime, &prepared.circuit_generation_id).await?;
            if cancellation.is_cancelled() {
                return Ok(circuit_result(
                    &prepared,
                    checkpoint.processed_event_count,
                    false,
                ));
            }
            let batch = load_circuit_event_batch(
                runtime,
                request.input_circuit_event_watermark,
                checkpoint.cursor.as_ref(),
                ROUTING_GENERATION_REBUILD_BATCH_SIZE,
            )
            .await?;
            if batch.is_empty() {
                if checkpoint.processed_event_count != prepared.input_event_count {
                    return Err(PersistenceError::InvariantViolation(
                        "circuit generation checkpoint is not contiguous".into(),
                    ));
                }
                finalize_circuit_generation(runtime, &request, &prepared).await?;
                return Ok(circuit_result(&prepared, prepared.input_event_count, true));
            }

            let next_event_count = checkpoint
                .processed_event_count
                .saturating_add(batch.len() as u64);
            let mut write = runtime.begin_write().await?;
            apply_circuit_event_batch(
                write.connection(),
                &prepared.circuit_generation_id,
                &batch,
                &request,
            )
            .await?;
            let cursor = batch.last().ok_or_else(|| {
                PersistenceError::InvariantViolation(
                    "circuit generation batch cursor is missing".into(),
                )
            })?;
            let checkpoint_update = sqlx::query(
                "UPDATE routing_circuit_generation_v3_checkpoint
                 SET cursor_station_key_id = ?2,
                     cursor_station_key_lifecycle_revision = ?3,
                     cursor_reducer_commit_sequence = ?4, cursor_event_id = ?5,
                     processed_event_count = ?6, updated_at_ms = ?7
                 WHERE circuit_generation_id = ?1 AND status = 'building'
                   AND processed_event_count = ?8
                   AND cursor_station_key_id IS ?9
                   AND cursor_station_key_lifecycle_revision IS ?10
                   AND cursor_reducer_commit_sequence IS ?11
                   AND cursor_event_id IS ?12",
            )
            .bind(&prepared.circuit_generation_id)
            .bind(&cursor.station_key_id)
            .bind(to_i64(cursor.station_key_lifecycle_revision)?)
            .bind(to_i64(cursor.reducer_commit_sequence)?)
            .bind(&cursor.event_id)
            .bind(to_i64(next_event_count)?)
            .bind(request.evaluation_at_ms)
            .bind(to_i64(checkpoint.processed_event_count)?)
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| value.station_key_id.as_str()),
            )
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| to_i64(value.station_key_lifecycle_revision))
                    .transpose()?,
            )
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| to_i64(value.reducer_commit_sequence))
                    .transpose()?,
            )
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| value.event_id.as_str()),
            )
            .execute(write.connection())
            .await?
            .rows_affected();
            let metadata_update = sqlx::query(
                "UPDATE routing_circuit_generation_v3
                 SET cursor_station_key_id = ?2,
                     cursor_station_key_lifecycle_revision = ?3,
                     cursor_reducer_commit_sequence = ?4, cursor_event_id = ?5,
                     processed_event_count = ?6, updated_at_ms = ?7
                 WHERE circuit_generation_id = ?1 AND status = 'building'
                   AND processed_event_count = ?8
                   AND cursor_station_key_id IS ?9
                   AND cursor_station_key_lifecycle_revision IS ?10
                   AND cursor_reducer_commit_sequence IS ?11
                   AND cursor_event_id IS ?12",
            )
            .bind(&prepared.circuit_generation_id)
            .bind(&cursor.station_key_id)
            .bind(to_i64(cursor.station_key_lifecycle_revision)?)
            .bind(to_i64(cursor.reducer_commit_sequence)?)
            .bind(&cursor.event_id)
            .bind(to_i64(next_event_count)?)
            .bind(request.evaluation_at_ms)
            .bind(to_i64(checkpoint.processed_event_count)?)
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| value.station_key_id.as_str()),
            )
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| to_i64(value.station_key_lifecycle_revision))
                    .transpose()?,
            )
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| to_i64(value.reducer_commit_sequence))
                    .transpose()?,
            )
            .bind(
                checkpoint
                    .cursor
                    .as_ref()
                    .map(|value| value.event_id.as_str()),
            )
            .execute(write.connection())
            .await?
            .rows_affected();
            if checkpoint_update != 1 || metadata_update != 1 {
                return Err(PersistenceError::RevisionConflict(
                    "routing_circuit_generation_v3".into(),
                ));
            }
            write.commit().await?;
        }
    }
}

async fn prepare_quality_output(
    runtime: &PersistenceHandle,
    request: &QualityGenerationBuildRequest,
) -> Result<PreparedQualityOutput, PersistenceError> {
    #[cfg(test)]
    record_quality_preparation_scan(request)?;
    let source_profile_snapshot_id = ensure_source_profile_snapshot(runtime, request).await?;
    let mut input_hasher = Sha256::new();
    input_hasher.update(b"[[");
    let mut first_context = true;
    let mut context_cursor: Option<String> = None;
    loop {
        let mut read = runtime.begin_read().await?;
        let batch = load_snapshot_key_context_batch(
            read.connection(),
            &source_profile_snapshot_id,
            context_cursor.as_deref(),
            ROUTING_GENERATION_REBUILD_BATCH_SIZE,
        )
        .await?;
        drop(read);
        if batch.is_empty() {
            break;
        }
        for context in &batch {
            append_canonical_item(&mut input_hasher, context, &mut first_context)?;
        }
        context_cursor = batch.last().map(|context| context.station_key_id.clone());
    }
    input_hasher.update(b"],[");
    let mut first_observation = true;
    let mut observation_key_cursor: Option<String> = None;
    let mut observation_id_cursor: Option<String> = None;
    loop {
        let mut read = runtime.begin_read().await?;
        let batch = RoutingObservationStore
            .list_v3_generation_cursor(
                read.connection(),
                request.input_observation_watermark,
                request.next_observation_watermark,
                observation_key_cursor.as_deref(),
                observation_id_cursor.as_deref(),
                ROUTING_GENERATION_REBUILD_BATCH_SIZE,
            )
            .await?;
        drop(read);
        if batch.is_empty() {
            break;
        }
        for observation in &batch {
            append_canonical_item(&mut input_hasher, observation, &mut first_observation)?;
        }
        let last = batch.last().ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "quality input cursor batch unexpectedly empty".into(),
            )
        })?;
        observation_key_cursor = last.scope.station_key_id.clone();
        observation_id_cursor = Some(last.id.clone());
    }
    input_hasher.update(b"]]");
    let input_observation_hash = digest_hex(&input_hasher.finalize());

    let mut output_hasher = Sha256::new();
    output_hasher.update(b"[");
    let mut first_summary = true;
    let mut input_observation_count = 0_u64;
    let mut output_scope_count = 0_u64;
    let mut summary_cursor: Option<String> = None;
    loop {
        let batch = load_quality_summary_batch(
            runtime,
            request,
            &source_profile_snapshot_id,
            summary_cursor.as_deref(),
            ROUTING_GENERATION_REBUILD_BATCH_SIZE,
        )
        .await?;
        if batch.is_empty() {
            break;
        }
        for output in &batch {
            append_canonical_item(&mut output_hasher, output, &mut first_summary)?;
            input_observation_count =
                input_observation_count.saturating_add(output.input_observation_count);
            output_scope_count = output_scope_count.saturating_add(1);
        }
        summary_cursor = batch.last().map(|output| output.station_key_id.clone());
    }
    output_hasher.update(b"]");
    let output_content_hash = digest_hex(&output_hasher.finalize());
    let quality_generation_id = quality_generation_id(
        request.evaluation_at_ms,
        request.input_observation_watermark,
        request.config.quality_policy_revision,
        QUALITY_PROJECTOR_VERSION,
        &input_observation_hash,
    )
    .map_err(|_| PersistenceError::ConstraintViolation)?;
    Ok(PreparedQualityOutput {
        checkpoint_ref: format!("quality-checkpoint:{quality_generation_id}"),
        quality_generation_id,
        source_profile_snapshot_id,
        input_observation_hash,
        output_content_hash,
        input_observation_count,
        output_scope_count,
        build_request_hash: quality_build_request_hash(request)?,
    })
}

#[cfg(test)]
fn quality_preparation_scan_counts() -> &'static Mutex<std::collections::BTreeMap<String, usize>> {
    static COUNTS: OnceLock<Mutex<std::collections::BTreeMap<String, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(std::collections::BTreeMap::new()))
}

#[cfg(test)]
fn record_quality_preparation_scan(
    request: &QualityGenerationBuildRequest,
) -> Result<(), PersistenceError> {
    let identity = quality_build_request_hash(request)?;
    let mut counts = quality_preparation_scan_counts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    *counts.entry(identity).or_default() += 1;
    Ok(())
}

#[cfg(test)]
fn quality_preparation_scan_count(
    request: &QualityGenerationBuildRequest,
) -> Result<usize, PersistenceError> {
    let identity = quality_build_request_hash(request)?;
    Ok(quality_preparation_scan_counts()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&identity)
        .copied()
        .unwrap_or_default())
}

fn quality_build_request_hash(
    request: &QualityGenerationBuildRequest,
) -> Result<String, PersistenceError> {
    let value = serde_json::json!({
        "version": "routing-quality-build-request-v1",
        "input_observation_watermark": request.input_observation_watermark,
        "next_observation_watermark": request.next_observation_watermark,
        "evaluation_at_ms": request.evaluation_at_ms,
        "quality_policy_revision": request.config.quality_policy_revision,
        "recent_minimum_samples": request.config.recent_minimum_samples,
        "historical_minimum_samples": request.config.historical_minimum_samples,
        "optimistic_reliability_basis_points": request.config.optimistic_reliability_basis_points,
        "optimistic_latency_ms": request.config.optimistic_latency_ms,
        "real_traffic_weight_basis_points": request.config.real_traffic_weight_basis_points,
        "monitoring_weight_basis_points": request.config.monitoring_weight_basis_points,
        "real_source_eligible": request.config.real_source_eligible,
        "monitoring_source_eligible": request.config.monitoring_source_eligible,
        "current_lifecycle_revision": request.config.current_lifecycle_revision,
        "algorithm_version": QUALITY_PROJECTOR_VERSION,
    });
    crate::application::routing_generation::canonical_json_sha256(&value)
        .map_err(|_| PersistenceError::ConstraintViolation)
}

async fn load_prepared_quality_output(
    runtime: &PersistenceHandle,
    request: &QualityGenerationBuildRequest,
    build_request_hash: &str,
) -> Result<Option<PreparedQualityOutput>, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let row = sqlx::query(
        "SELECT quality_generation_id, quality_policy_revision,
                quality_algorithm_version, input_observation_watermark,
                input_observation_hash, output_content_hash, checkpoint_ref,
                expected_input_observation_count, expected_output_scope_count,
                source_profile_snapshot_id
         FROM routing_quality_generation_v3
         WHERE build_request_hash = ?1
           AND status IN ('building', 'ready', 'active', 'retired')",
    )
    .bind(build_request_hash)
    .fetch_optional(read.connection())
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    let expected_policy_revision = i64::try_from(request.config.quality_policy_revision)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    let expected_watermark = i64::try_from(request.input_observation_watermark)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    if row.get::<i64, _>("quality_policy_revision") != expected_policy_revision
        || row.get::<String, _>("quality_algorithm_version") != QUALITY_PROJECTOR_VERSION
        || row.get::<Option<i64>, _>("input_observation_watermark") != Some(expected_watermark)
    {
        return Err(PersistenceError::InvariantViolation(
            "quality build request identity collision".to_string(),
        ));
    }
    let quality_generation_id = row.get::<String, _>("quality_generation_id");
    let source_profile_snapshot_id = row
        .get::<Option<String>, _>("source_profile_snapshot_id")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "quality generation resume source-profile snapshot is missing".to_string(),
            )
        })?;
    let input_observation_hash = row
        .get::<Option<String>, _>("input_observation_hash")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "quality generation resume input hash is missing".to_string(),
            )
        })?;
    let output_content_hash = row
        .get::<Option<String>, _>("output_content_hash")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "quality generation resume output hash is missing".to_string(),
            )
        })?;
    let checkpoint_ref = row
        .get::<Option<String>, _>("checkpoint_ref")
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "quality generation resume checkpoint is missing".to_string(),
            )
        })?;
    let input_observation_count = nonnegative_u64(
        row.get::<i64, _>("expected_input_observation_count"),
        "quality expected input observation count",
    )?;
    let output_scope_count = nonnegative_u64(
        row.get::<i64, _>("expected_output_scope_count"),
        "quality expected output scope count",
    )?;
    Ok(Some(PreparedQualityOutput {
        quality_generation_id,
        source_profile_snapshot_id,
        input_observation_hash,
        output_content_hash,
        checkpoint_ref,
        input_observation_count,
        output_scope_count,
        build_request_hash: build_request_hash.to_string(),
    }))
}

#[derive(Debug, Serialize)]
struct QualityKeyContext {
    station_key_id: String,
    lifecycle_revision: u64,
    real_source_eligible: bool,
    monitoring_source_eligible: bool,
    monitoring_profile_commitment: Option<String>,
}

#[derive(Debug, Serialize)]
struct MonitoringProfileFact {
    monitor_id: String,
    target_type: String,
    station_key_id: Option<String>,
    template_id: String,
    protocol_kind: String,
    client_profile_id: String,
    client_profile_version: u64,
    primary_model: String,
    schedule_revision: u64,
}

async fn ensure_source_profile_snapshot(
    runtime: &PersistenceHandle,
    request: &QualityGenerationBuildRequest,
) -> Result<String, PersistenceError> {
    let build_request_hash = quality_build_request_hash(request)?;
    let snapshot_id = format!("quality-profile:{build_request_hash}");
    let mut write = runtime.begin_write().await?;
    let existing = sqlx::query(
        "SELECT evaluation_at_ms, input_observation_watermark,
                quality_policy_revision, profile_count
         FROM routing_quality_source_profile_snapshot_v3
         WHERE snapshot_id = ?1",
    )
    .bind(&snapshot_id)
    .fetch_optional(write.connection())
    .await?;
    if let Some(row) = existing {
        let item_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM routing_quality_source_profile_snapshot_item_v3
             WHERE snapshot_id = ?1",
        )
        .bind(&snapshot_id)
        .fetch_one(write.connection())
        .await?;
        if row.get::<i64, _>("evaluation_at_ms") != request.evaluation_at_ms
            || row.get::<i64, _>("input_observation_watermark")
                != to_i64(request.input_observation_watermark)?
            || row.get::<i64, _>("quality_policy_revision")
                != to_i64(request.config.quality_policy_revision)?
            || row.get::<i64, _>("profile_count") != item_count
        {
            return Err(PersistenceError::InvariantViolation(
                "quality source-profile snapshot identity collision".into(),
            ));
        }
        write.commit().await?;
        return Ok(snapshot_id);
    }

    let contexts = load_current_key_contexts(write.connection()).await?;
    let content_hash = canonical_hash(&contexts)?;
    for context in &contexts {
        sqlx::query(
            "INSERT INTO routing_quality_source_profile_snapshot_item_v3 (
                 snapshot_id, station_key_id, station_key_lifecycle_revision,
                 real_source_eligible, monitoring_source_eligible,
                 monitoring_profile_commitment, captured_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .bind(&snapshot_id)
        .bind(&context.station_key_id)
        .bind(to_i64(context.lifecycle_revision)?)
        .bind(i64::from(context.real_source_eligible))
        .bind(i64::from(context.monitoring_source_eligible))
        .bind(context.monitoring_profile_commitment.as_deref())
        .bind(request.evaluation_at_ms)
        .execute(write.connection())
        .await?;
    }
    sqlx::query(
        "INSERT INTO routing_quality_source_profile_snapshot_v3 (
             snapshot_id, evaluation_at_ms, input_observation_watermark,
             quality_policy_revision, profile_count, content_hash, created_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?2)",
    )
    .bind(&snapshot_id)
    .bind(request.evaluation_at_ms)
    .bind(to_i64(request.input_observation_watermark)?)
    .bind(to_i64(request.config.quality_policy_revision)?)
    .bind(to_i64(contexts.len() as u64)?)
    .bind(content_hash)
    .execute(write.connection())
    .await?;
    write.commit().await?;
    Ok(snapshot_id)
}

async fn load_current_key_contexts(
    connection: &mut sqlx::SqliteConnection,
) -> Result<Vec<QualityKeyContext>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT k.id AS station_key_id, r.revision AS station_key_lifecycle_revision,
                CASE WHEN k.enabled = 1 AND s.enabled = 1
                       AND (TRIM(k.api_key) <> '' OR k.api_key_secret_id IS NOT NULL)
                     THEN 1 ELSE 0 END AS real_source_eligible,
                m.id AS monitor_id, m.target_type,
                m.station_key_id AS monitor_station_key_id, m.template_id,
                m.protocol_kind, m.client_profile_id, m.client_profile_version,
                m.primary_model, m.schedule_revision
         FROM station_keys k
         JOIN stations s ON s.id = k.station_id
         JOIN domain_revisions r ON r.scope = 'station_key:' || k.id
         LEFT JOIN channel_monitors m
           ON m.enabled = 1 AND m.station_id = k.station_id
          AND (m.station_key_id = k.id OR m.station_key_id IS NULL)
          AND m.client_profile_id = 'standard_api'
          AND m.client_profile_version > 0
          AND m.protocol_kind IN (
              'open_ai_chat', 'open_ai_responses', 'anthropic_messages',
              'gemini_native', 'xai_grok', 'generic_open_ai'
          )
          AND TRIM(m.primary_model) <> ''
         WHERE r.revision > 0
         ORDER BY k.id, r.revision, m.id",
    )
    .fetch_all(&mut *connection)
    .await?;
    let mut grouped = BTreeMap::<String, (u64, bool, Vec<MonitoringProfileFact>)>::new();
    for row in rows {
        let station_key_id = row.get::<String, _>("station_key_id");
        let lifecycle_revision = u64::try_from(row.get::<i64, _>("station_key_lifecycle_revision"))
            .map_err(|_| {
                PersistenceError::InvariantViolation(
                    "quality rebuild lifecycle revision is negative".into(),
                )
            })?;
        let entry = grouped.entry(station_key_id).or_insert_with(|| {
            (
                lifecycle_revision,
                row.get::<i64, _>("real_source_eligible") != 0,
                Vec::new(),
            )
        });
        if entry.0 != lifecycle_revision {
            return Err(PersistenceError::InvariantViolation(
                "quality source-profile snapshot contains duplicate Key lifecycles".into(),
            ));
        }
        if let Some(monitor_id) = row.get::<Option<String>, _>("monitor_id") {
            entry.2.push(MonitoringProfileFact {
                monitor_id,
                target_type: row.get("target_type"),
                station_key_id: row.get("monitor_station_key_id"),
                template_id: row.get("template_id"),
                protocol_kind: row.get("protocol_kind"),
                client_profile_id: row.get("client_profile_id"),
                client_profile_version: nonnegative_u64(
                    row.get("client_profile_version"),
                    "monitor client-profile version",
                )?,
                primary_model: row.get("primary_model"),
                schedule_revision: nonnegative_u64(
                    row.get("schedule_revision"),
                    "monitor schedule revision",
                )?,
            });
        }
    }
    grouped
        .into_iter()
        .map(
            |(station_key_id, (lifecycle_revision, real_source_eligible, profiles))| {
                let monitoring_profile_commitment = (!profiles.is_empty())
                    .then(|| canonical_hash(&profiles))
                    .transpose()?;
                Ok(QualityKeyContext {
                    station_key_id,
                    lifecycle_revision,
                    real_source_eligible,
                    monitoring_source_eligible: monitoring_profile_commitment.is_some(),
                    monitoring_profile_commitment,
                })
            },
        )
        .collect()
}

async fn load_snapshot_key_context_batch(
    connection: &mut sqlx::SqliteConnection,
    snapshot_id: &str,
    after_station_key_id: Option<&str>,
    limit: usize,
) -> Result<Vec<QualityKeyContext>, PersistenceError> {
    let rows = sqlx::query(
        "SELECT item.station_key_id,
                CASE
                    WHEN alias.target_lifecycle_revision > item.station_key_lifecycle_revision
                    THEN alias.target_lifecycle_revision
                    ELSE item.station_key_lifecycle_revision
                END AS station_key_lifecycle_revision,
                item.real_source_eligible, item.monitoring_source_eligible,
                item.monitoring_profile_commitment
         FROM routing_quality_source_profile_snapshot_item_v3 item
         LEFT JOIN routing_quality_lifecycle_alias_v1 alias
           ON alias.station_key_id = item.station_key_id
         WHERE item.snapshot_id = ?1 AND (?2 IS NULL OR item.station_key_id > ?2)
         ORDER BY item.station_key_id, item.station_key_lifecycle_revision LIMIT ?3",
    )
    .bind(snapshot_id)
    .bind(after_station_key_id)
    .bind(i64::try_from(limit.clamp(1, 1_024)).map_err(|_| PersistenceError::ConstraintViolation)?)
    .fetch_all(&mut *connection)
    .await?;
    rows.into_iter()
        .map(|row| {
            Ok(QualityKeyContext {
                station_key_id: row.get("station_key_id"),
                lifecycle_revision: nonnegative_u64(
                    row.get("station_key_lifecycle_revision"),
                    "snapshot station-key lifecycle revision",
                )?,
                real_source_eligible: row.get::<i64, _>("real_source_eligible") != 0,
                monitoring_source_eligible: row.get::<i64, _>("monitoring_source_eligible") != 0,
                monitoring_profile_commitment: row.get("monitoring_profile_commitment"),
            })
        })
        .collect()
}

async fn load_quality_summary_batch(
    runtime: &PersistenceHandle,
    request: &QualityGenerationBuildRequest,
    source_profile_snapshot_id: &str,
    after_station_key_id: Option<&str>,
    limit: usize,
) -> Result<Vec<GenerationQualitySummary>, PersistenceError> {
    let contexts = {
        let mut read = runtime.begin_read().await?;
        load_snapshot_key_context_batch(
            read.connection(),
            source_profile_snapshot_id,
            after_station_key_id,
            limit,
        )
        .await?
    };
    if contexts.is_empty() {
        return Ok(Vec::new());
    }

    // Read the entire batch of Key histories with one bounded-scope query.
    // The previous implementation opened one query per Key, which made a
    // rebuild's query count proportional to the number of credentials and
    // caused a large station set to monopolize the SQLite read pool.
    let scopes = contexts
        .iter()
        .map(|context| format!("station_key:{}", context.station_key_id))
        .collect::<Vec<_>>();
    let mut observations = {
        let mut read = runtime.begin_read().await?;
        RoutingObservationStore
            .list_for_scopes_v3_for_generation(
                read.connection(),
                &scopes,
                request.input_observation_watermark,
                request.next_observation_watermark,
            )
            .await?
    };
    {
        let mut read = runtime.begin_read().await?;
        RoutingObservationStore
            .apply_quality_lifecycle_aliases(read.connection(), &mut observations)
            .await?;
    }
    let mut observations_by_key =
        BTreeMap::<String, Vec<crate::models::routing_observation::RoutingObservation>>::new();
    for observation in observations {
        if let Some(station_key_id) = observation.scope.station_key_id.clone() {
            observations_by_key
                .entry(station_key_id)
                .or_default()
                .push(observation);
        }
    }
    let mut summaries = Vec::with_capacity(contexts.len());
    for context in contexts {
        let observations = observations_by_key
            .remove(&context.station_key_id)
            .unwrap_or_default();
        let mut config = request.config;
        config.current_lifecycle_revision = Some(context.lifecycle_revision);
        config.real_source_eligible = context.real_source_eligible;
        config.monitoring_source_eligible = context.monitoring_source_eligible;
        let scope = format!("station_key:{}", context.station_key_id);
        let summary = rebuild_quality_summary_v3_at(
            &scope,
            &observations,
            config,
            request.input_observation_watermark.max(1),
            request.evaluation_at_ms,
        );
        let input_observation_count = observations
            .iter()
            .filter(|observation| {
                observation.station_key_lifecycle_revision == context.lifecycle_revision
            })
            .count() as u64;
        let last_observation_id = observations
            .iter()
            .rev()
            .find(|observation| {
                observation.station_key_lifecycle_revision == context.lifecycle_revision
            })
            .map(|observation| observation.id.clone());
        summaries.push(GenerationQualitySummary {
            station_key_id: context.station_key_id,
            station_key_lifecycle_revision: context.lifecycle_revision,
            input_observation_count,
            last_observation_id,
            summary,
        });
    }
    Ok(summaries)
}

async fn ensure_quality_generation(
    runtime: &PersistenceHandle,
    request: &QualityGenerationBuildRequest,
    prepared: &PreparedQualityOutput,
) -> Result<(), PersistenceError> {
    let mut write = runtime.begin_write().await?;
    let existing = sqlx::query(
        "SELECT quality_policy_revision, quality_algorithm_version,
                input_observation_watermark, input_observation_hash,
                output_content_hash, checkpoint_ref, build_request_hash,
                expected_input_observation_count, expected_output_scope_count,
                source_profile_snapshot_id
         FROM routing_quality_generation_v3 WHERE quality_generation_id = ?1",
    )
    .bind(&prepared.quality_generation_id)
    .fetch_optional(write.connection())
    .await?;
    if let Some(row) = existing {
        let matches = row.get::<i64, _>("quality_policy_revision")
            == i64::try_from(request.config.quality_policy_revision)
                .map_err(|_| PersistenceError::ConstraintViolation)?
            && row.get::<String, _>("quality_algorithm_version") == QUALITY_PROJECTOR_VERSION
            && row.get::<Option<i64>, _>("input_observation_watermark")
                == Some(
                    i64::try_from(request.input_observation_watermark)
                        .map_err(|_| PersistenceError::ConstraintViolation)?,
                )
            && row.get::<Option<String>, _>("input_observation_hash")
                == Some(prepared.input_observation_hash.clone())
            && row.get::<Option<String>, _>("output_content_hash")
                == Some(prepared.output_content_hash.clone())
            && row.get::<Option<String>, _>("checkpoint_ref")
                == Some(prepared.checkpoint_ref.clone())
            && row.get::<Option<String>, _>("build_request_hash")
                == Some(prepared.build_request_hash.clone())
            && row.get::<Option<i64>, _>("expected_input_observation_count")
                == Some(to_i64(prepared.input_observation_count)?)
            && row.get::<Option<i64>, _>("expected_output_scope_count")
                == Some(to_i64(prepared.output_scope_count)?)
            && row.get::<Option<String>, _>("source_profile_snapshot_id")
                == Some(prepared.source_profile_snapshot_id.clone());
        if !matches {
            return Err(PersistenceError::InvariantViolation(
                "quality generation identity collision".into(),
            ));
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO routing_quality_generation_v3 (
             quality_generation_id, scope, quality_policy_revision,
             quality_algorithm_version, status, evaluation_at_ms,
             input_observation_watermark, input_observation_hash,
             output_content_hash, checkpoint_ref, processed_observation_count,
             created_at_ms, updated_at_ms, build_request_hash,
             expected_input_observation_count, expected_output_scope_count,
             source_profile_snapshot_id
         ) VALUES (?1, ?2, ?3, ?4, 'building', ?5, ?6, ?7, ?8, ?9, 0, ?5, ?5,
                   ?10, ?11, ?12, ?13)",
    )
    .bind(&prepared.quality_generation_id)
    .bind(QUALITY_GENERATION_SCOPE)
    .bind(
        i64::try_from(request.config.quality_policy_revision)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(QUALITY_PROJECTOR_VERSION)
    .bind(request.evaluation_at_ms)
    .bind(
        i64::try_from(request.input_observation_watermark)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(&prepared.input_observation_hash)
    .bind(&prepared.output_content_hash)
    .bind(&prepared.checkpoint_ref)
    .bind(&prepared.build_request_hash)
    .bind(to_i64(prepared.input_observation_count)?)
    .bind(to_i64(prepared.output_scope_count)?)
    .bind(&prepared.source_profile_snapshot_id)
    .execute(write.connection())
    .await?;
    sqlx::query(
        "INSERT INTO routing_quality_generation_v3_checkpoint (
             quality_generation_id, input_observation_watermark,
             processed_observation_count, status, updated_at_ms
         ) VALUES (?1, ?2, 0, 'building', ?3)",
    )
    .bind(&prepared.quality_generation_id)
    .bind(
        i64::try_from(request.input_observation_watermark)
            .map_err(|_| PersistenceError::ConstraintViolation)?,
    )
    .bind(request.evaluation_at_ms)
    .execute(write.connection())
    .await?;
    write.commit().await
}

async fn load_quality_progress(
    runtime: &PersistenceHandle,
    quality_generation_id: &str,
) -> Result<QualityCheckpoint, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let scope_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_quality_summary_v3
         WHERE quality_generation_id = ?1",
    )
    .bind(quality_generation_id)
    .fetch_one(read.connection())
    .await?;
    let row = sqlx::query(
        "SELECT cursor_station_key_id, cursor_observation_id,
                processed_observation_count
         FROM routing_quality_generation_v3_checkpoint
         WHERE quality_generation_id = ?1",
    )
    .bind(quality_generation_id)
    .fetch_one(read.connection())
    .await?;
    Ok(QualityCheckpoint {
        cursor_station_key_id: row.get("cursor_station_key_id"),
        cursor_observation_id: row.get("cursor_observation_id"),
        processed_scope_count: nonnegative_u64(scope_count, "quality processed scope count")?,
        processed_observation_count: nonnegative_u64(
            row.get("processed_observation_count"),
            "quality processed observation count",
        )?,
    })
}

async fn finalize_quality_generation(
    runtime: &PersistenceHandle,
    request: &QualityGenerationBuildRequest,
    prepared: &PreparedQualityOutput,
) -> Result<(), PersistenceError> {
    let mut write = runtime.begin_write().await?;
    let rows = sqlx::query(
        "SELECT station_key_id, station_key_lifecycle_revision, summary_json
         FROM routing_quality_summary_v3 WHERE quality_generation_id = ?1
         ORDER BY station_key_id, station_key_lifecycle_revision",
    )
    .bind(&prepared.quality_generation_id)
    .fetch_all(write.connection())
    .await?;
    let mut persisted = Vec::with_capacity(rows.len());
    for row in rows {
        let summary: QualitySummary =
            serde_json::from_str(&row.get::<String, _>("summary_json"))
                .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
        persisted.push(GenerationQualitySummary {
            station_key_id: row.get("station_key_id"),
            station_key_lifecycle_revision: u64::try_from(
                row.get::<i64, _>("station_key_lifecycle_revision"),
            )
            .map_err(|_| PersistenceError::ConstraintViolation)?,
            // These fields are intentionally skipped by `Serialize`; the
            // durable checkpoint owns the aggregate count and the output hash
            // covers only the canonical station-key identity plus summary.
            input_observation_count: 0,
            last_observation_id: None,
            summary,
        });
    }
    if persisted.len() as u64 != prepared.output_scope_count
        || canonical_hash(&persisted)? != prepared.output_content_hash
    {
        return Err(PersistenceError::InvariantViolation(
            "quality generation output verification failed".into(),
        ));
    }
    // The checkpoint is advanced in the same transaction as each batch. Its
    // processed count is therefore the authoritative expected input count;
    // retaining every in-memory summary just to recompute this total is both
    // redundant and an avoidable memory spike on large installations.
    let expected_count: i64 = sqlx::query_scalar(
        "SELECT processed_observation_count
         FROM routing_quality_generation_v3_checkpoint
         WHERE quality_generation_id = ?1",
    )
    .bind(&prepared.quality_generation_id)
    .fetch_one(write.connection())
    .await?;
    let updated = sqlx::query(
        "UPDATE routing_quality_generation_v3
         SET status = 'ready', ready_at_ms = ?2, updated_at_ms = ?2
         WHERE quality_generation_id = ?1 AND status = 'building'
           AND processed_observation_count = ?3",
    )
    .bind(&prepared.quality_generation_id)
    .bind(request.evaluation_at_ms)
    .bind(expected_count)
    .execute(write.connection())
    .await?
    .rows_affected();
    if updated == 0 {
        let status: String = sqlx::query_scalar(
            "SELECT status FROM routing_quality_generation_v3
             WHERE quality_generation_id = ?1",
        )
        .bind(&prepared.quality_generation_id)
        .fetch_one(write.connection())
        .await?;
        if !matches!(status.as_str(), "ready" | "active" | "retired") {
            return Err(PersistenceError::RevisionConflict(
                "routing_quality_generation_v3".into(),
            ));
        }
    }
    sqlx::query(
        "UPDATE routing_quality_generation_v3_checkpoint
         SET status = 'ready', updated_at_ms = ?2
         WHERE quality_generation_id = ?1 AND processed_observation_count = ?3",
    )
    .bind(&prepared.quality_generation_id)
    .bind(request.evaluation_at_ms)
    .bind(expected_count)
    .execute(write.connection())
    .await?;
    write.commit().await
}

async fn load_circuit_event_batch(
    runtime: &PersistenceHandle,
    watermark: u64,
    cursor: Option<&CircuitEventCursor>,
    limit: usize,
) -> Result<Vec<CircuitReplayEvent>, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let rows = sqlx::query(
        "SELECT event_id, effect_kind, station_key_id,
                station_key_lifecycle_revision, reducer_commit_sequence,
                ingestion_sequence, occurred_at_ms, canonical_outcome,
                failure_code, lease_revision, boundary_crossed
         FROM routing_circuit_event_v3
         WHERE ingestion_sequence IS NOT NULL AND ingestion_sequence <= ?1
           AND applied = 1
           AND effect_kind IN ('circuit', 'lease')
           AND (
                ?2 IS NULL
                OR station_key_id > ?2
                OR (station_key_id = ?2
                    AND station_key_lifecycle_revision > ?3)
                OR (station_key_id = ?2
                    AND station_key_lifecycle_revision = ?3
                    AND reducer_commit_sequence > ?4)
           )
         ORDER BY station_key_id, station_key_lifecycle_revision,
                  reducer_commit_sequence, event_id, effect_kind
         LIMIT ?5",
    )
    .bind(to_i64(watermark)?)
    .bind(cursor.map(|value| value.station_key_id.as_str()))
    .bind(
        cursor
            .map(|value| to_i64(value.station_key_lifecycle_revision))
            .transpose()?,
    )
    .bind(
        cursor
            .map(|value| to_i64(value.reducer_commit_sequence))
            .transpose()?,
    )
    .bind(i64::try_from(limit.clamp(1, 1_024)).map_err(|_| PersistenceError::ConstraintViolation)?)
    .fetch_all(read.connection())
    .await?;
    rows.into_iter().map(circuit_event_from_row).collect()
}

fn circuit_event_from_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CircuitReplayEvent, PersistenceError> {
    Ok(CircuitReplayEvent {
        event_id: row.get("event_id"),
        effect_kind: row.get("effect_kind"),
        station_key_id: row.get("station_key_id"),
        station_key_lifecycle_revision: positive_u64(
            row.get("station_key_lifecycle_revision"),
            "station_key_lifecycle_revision",
        )?,
        reducer_commit_sequence: positive_u64(
            row.get("reducer_commit_sequence"),
            "reducer_commit_sequence",
        )?,
        ingestion_sequence: positive_u64(row.get("ingestion_sequence"), "ingestion_sequence")?,
        occurred_at_ms: nonnegative_u64(row.get("occurred_at_ms"), "occurred_at_ms")?,
        canonical_outcome: row.get("canonical_outcome"),
        failure_code: row.get("failure_code"),
        lease_revision: row
            .get::<Option<i64>, _>("lease_revision")
            .map(|value| positive_u64(value, "lease_revision"))
            .transpose()?,
        boundary_crossed: row.get::<i64, _>("boundary_crossed") != 0,
    })
}

async fn hash_replayed_circuit_states(
    runtime: &PersistenceHandle,
    request: &CircuitGenerationBuildRequest,
) -> Result<(String, u64), PersistenceError> {
    let mut hasher = Sha256::new();
    hasher.update(b"[");
    let mut first_state = true;
    let mut state_count = 0_u64;
    let mut cursor: Option<CircuitEventCursor> = None;
    let mut current: Option<CircuitGenerationState> = None;
    loop {
        let batch = load_circuit_event_batch(
            runtime,
            request.input_circuit_event_watermark,
            cursor.as_ref(),
            ROUTING_GENERATION_REBUILD_BATCH_SIZE,
        )
        .await?;
        if batch.is_empty() {
            break;
        }
        for event in &batch {
            if current
                .as_ref()
                .is_some_and(|state| !same_circuit_key(state, event))
            {
                let completed = current.take().expect("checked circuit state");
                append_canonical_item(&mut hasher, &completed, &mut first_state)?;
                state_count = state_count.saturating_add(1);
            }
            let state = current.get_or_insert_with(|| new_circuit_state(event));
            state.monotonic_clock_watermark_ms =
                state.monotonic_clock_watermark_ms.max(event.occurred_at_ms);
            state.reducer_commit_sequence = event.reducer_commit_sequence;
            apply_circuit_event(state, event, request)?;
        }
        let last = batch.last().ok_or_else(|| {
            PersistenceError::InvariantViolation("circuit hash cursor is missing".into())
        })?;
        cursor = Some(CircuitEventCursor {
            station_key_id: last.station_key_id.clone(),
            station_key_lifecycle_revision: last.station_key_lifecycle_revision,
            reducer_commit_sequence: last.reducer_commit_sequence,
            event_id: last.event_id.clone(),
            effect_kind: last.effect_kind.clone(),
        });
    }
    if let Some(completed) = current {
        append_canonical_item(&mut hasher, &completed, &mut first_state)?;
        state_count = state_count.saturating_add(1);
    }
    hasher.update(b"]");
    Ok((digest_hex(&hasher.finalize()), state_count))
}

async fn prepare_circuit_output(
    runtime: &PersistenceHandle,
    request: &CircuitGenerationBuildRequest,
) -> Result<PreparedCircuitOutput, PersistenceError> {
    let mut input_hasher = Sha256::new();
    input_hasher.update(b"[");
    let mut first_event = true;
    let mut input_event_count = 0_u64;
    let mut cursor: Option<CircuitEventCursor> = None;
    loop {
        let batch = load_circuit_event_batch(
            runtime,
            request.input_circuit_event_watermark,
            cursor.as_ref(),
            ROUTING_GENERATION_REBUILD_BATCH_SIZE,
        )
        .await?;
        if batch.is_empty() {
            break;
        }
        for event in &batch {
            append_canonical_item(&mut input_hasher, event, &mut first_event)?;
            input_event_count = input_event_count.saturating_add(1);
        }
        let Some(last) = batch.last() else {
            break;
        };
        cursor = Some(CircuitEventCursor {
            station_key_id: last.station_key_id.clone(),
            station_key_lifecycle_revision: last.station_key_lifecycle_revision,
            reducer_commit_sequence: last.reducer_commit_sequence,
            event_id: last.event_id.clone(),
            effect_kind: last.effect_kind.clone(),
        });
    }
    input_hasher.update(b"]");
    let input_circuit_event_hash = digest_hex(&input_hasher.finalize());

    let (output_content_hash, output_state_count) =
        hash_replayed_circuit_states(runtime, request).await?;
    let circuit_generation_id = circuit_generation_id(
        request.input_circuit_event_watermark,
        request.circuit_policy_revision,
        CIRCUIT_REBUILD_ALGORITHM_VERSION,
        &input_circuit_event_hash,
    )
    .map_err(|_| PersistenceError::ConstraintViolation)?;
    Ok(PreparedCircuitOutput {
        checkpoint_ref: format!("circuit-checkpoint:{circuit_generation_id}"),
        circuit_generation_id,
        input_circuit_event_hash,
        output_content_hash,
        input_event_count,
        output_state_count,
    })
}

fn new_circuit_state(event: &CircuitReplayEvent) -> CircuitGenerationState {
    CircuitGenerationState {
        station_key_id: event.station_key_id.clone(),
        station_key_lifecycle_revision: event.station_key_lifecycle_revision,
        state: "closed".to_string(),
        state_revision: 1,
        consecutive_failures: 0,
        reopen_level: 0,
        opened_at_ms: None,
        cooldown_until_ms: None,
        recovery_successes: 0,
        monotonic_clock_watermark_ms: 0,
        reducer_commit_sequence: 0,
    }
}

fn same_circuit_key(state: &CircuitGenerationState, event: &CircuitReplayEvent) -> bool {
    state.station_key_id == event.station_key_id
        && state.station_key_lifecycle_revision == event.station_key_lifecycle_revision
}

async fn apply_circuit_event_batch(
    connection: &mut sqlx::SqliteConnection,
    circuit_generation_id: &str,
    events: &[CircuitReplayEvent],
    request: &CircuitGenerationBuildRequest,
) -> Result<(), PersistenceError> {
    let mut current: Option<CircuitGenerationState> = None;
    for event in events {
        if current
            .as_ref()
            .is_some_and(|state| !same_circuit_key(state, event))
        {
            let completed = current.take().expect("checked circuit state");
            save_circuit_generation_state(
                connection,
                circuit_generation_id,
                &completed,
                request.evaluation_at_ms,
            )
            .await?;
        }
        if current.is_none() {
            current = load_circuit_generation_state(
                connection,
                circuit_generation_id,
                &event.station_key_id,
                event.station_key_lifecycle_revision,
            )
            .await?
            .or_else(|| Some(new_circuit_state(event)));
        }
        let state = current.as_mut().expect("circuit state initialized");
        if state.reducer_commit_sequence >= event.reducer_commit_sequence {
            return Err(PersistenceError::RevisionConflict(
                "routing_circuit_generation_v3".into(),
            ));
        }
        state.monotonic_clock_watermark_ms =
            state.monotonic_clock_watermark_ms.max(event.occurred_at_ms);
        state.reducer_commit_sequence = event.reducer_commit_sequence;
        apply_circuit_event(state, event, request)?;
        sqlx::query(
            "INSERT INTO routing_circuit_event_applied_generation_v3 (
                 circuit_generation_id, event_id, effect_kind, station_key_id,
                 station_key_lifecycle_revision, reducer_commit_sequence,
                 ingestion_sequence, applied_at_ms
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
             ON CONFLICT(circuit_generation_id, event_id, effect_kind) DO NOTHING",
        )
        .bind(circuit_generation_id)
        .bind(&event.event_id)
        .bind(&event.effect_kind)
        .bind(&event.station_key_id)
        .bind(to_i64(event.station_key_lifecycle_revision)?)
        .bind(to_i64(event.reducer_commit_sequence)?)
        .bind(to_i64(event.ingestion_sequence)?)
        .bind(request.evaluation_at_ms)
        .execute(&mut *connection)
        .await?;
    }
    if let Some(completed) = current {
        save_circuit_generation_state(
            connection,
            circuit_generation_id,
            &completed,
            request.evaluation_at_ms,
        )
        .await?;
    }
    Ok(())
}

fn apply_circuit_event(
    state: &mut CircuitGenerationState,
    event: &CircuitReplayEvent,
    request: &CircuitGenerationBuildRequest,
) -> Result<(), PersistenceError> {
    if event.effect_kind == "lease" {
        if event.failure_code.as_deref() == Some("lease_expired") {
            reopen_circuit(state, event.occurred_at_ms, request);
        }
        return Ok(());
    }
    let success = event.canonical_outcome == "success" && event.boundary_crossed;
    let failure = event.canonical_outcome == "attributable_failure" && event.boundary_crossed;
    let is_probe_result = event.lease_revision.is_some();
    let state_name = state.state.clone();
    match state_name.as_str() {
        "closed" => {
            if success {
                state.state_revision = state.state_revision.saturating_add(1);
                state.consecutive_failures = 0;
            } else if failure {
                state.state_revision = state.state_revision.saturating_add(1);
                state.consecutive_failures = state.consecutive_failures.saturating_add(1);
                if state.consecutive_failures >= request.consecutive_failure_threshold {
                    open_circuit(state, event.occurred_at_ms, request, false);
                }
            }
        }
        "open" if is_probe_result => {
            state.state = "half_open".to_string();
            state.state_revision = state.state_revision.saturating_add(1);
            apply_half_open_result(state, event, success, failure, request);
        }
        "open" => {}
        "half_open" if is_probe_result => {
            apply_half_open_result(state, event, success, failure, request);
        }
        "half_open" => {}
        _ => {
            return Err(PersistenceError::InvariantViolation(
                "circuit rebuild produced an unknown state".into(),
            ))
        }
    }
    Ok(())
}

fn apply_half_open_result(
    state: &mut CircuitGenerationState,
    event: &CircuitReplayEvent,
    success: bool,
    failure: bool,
    request: &CircuitGenerationBuildRequest,
) {
    if success {
        state.state_revision = state.state_revision.saturating_add(1);
        state.recovery_successes = state.recovery_successes.saturating_add(1);
        if state.recovery_successes >= request.recovery_success_threshold {
            state.state = "closed".to_string();
            state.consecutive_failures = 0;
            state.reopen_level = 0;
            state.opened_at_ms = None;
            state.cooldown_until_ms = None;
            state.recovery_successes = 0;
        }
    } else if failure {
        reopen_circuit(state, event.occurred_at_ms, request);
    }
}

fn reopen_circuit(
    state: &mut CircuitGenerationState,
    occurred_at_ms: u64,
    request: &CircuitGenerationBuildRequest,
) {
    open_circuit(state, occurred_at_ms, request, true);
}

fn open_circuit(
    state: &mut CircuitGenerationState,
    occurred_at_ms: u64,
    request: &CircuitGenerationBuildRequest,
    reopening: bool,
) {
    if reopening {
        state.state_revision = state.state_revision.saturating_add(1);
    }
    state.state = "open".to_string();
    if reopening {
        state.consecutive_failures = 0;
    }
    state.reopen_level = state.reopen_level.saturating_add(1).max(1);
    state.opened_at_ms = Some(occurred_at_ms);
    let exponent = state.reopen_level.saturating_sub(1).min(63);
    let multiplier = 1_u64.checked_shl(exponent).unwrap_or(u64::MAX);
    let cooldown = request
        .recovery_wait_ms
        .saturating_mul(multiplier)
        .min(request.max_cooldown_ms);
    state.cooldown_until_ms = Some(occurred_at_ms.saturating_add(cooldown));
    state.recovery_successes = 0;
}

async fn ensure_circuit_generation(
    runtime: &PersistenceHandle,
    request: &CircuitGenerationBuildRequest,
    prepared: &PreparedCircuitOutput,
) -> Result<(), PersistenceError> {
    let mut write = runtime.begin_write().await?;
    let existing = sqlx::query(
        "SELECT circuit_policy_revision, circuit_algorithm_version,
                input_circuit_event_watermark, input_circuit_event_hash,
                output_content_hash, checkpoint_ref
         FROM routing_circuit_generation_v3 WHERE circuit_generation_id = ?1",
    )
    .bind(&prepared.circuit_generation_id)
    .fetch_optional(write.connection())
    .await?;
    if let Some(row) = existing {
        let matches = row.get::<i64, _>("circuit_policy_revision")
            == to_i64(request.circuit_policy_revision)?
            && row.get::<String, _>("circuit_algorithm_version")
                == CIRCUIT_REBUILD_ALGORITHM_VERSION
            && row.get::<Option<i64>, _>("input_circuit_event_watermark")
                == Some(to_i64(request.input_circuit_event_watermark)?)
            && row.get::<Option<String>, _>("input_circuit_event_hash")
                == Some(prepared.input_circuit_event_hash.clone())
            && row.get::<Option<String>, _>("output_content_hash")
                == Some(prepared.output_content_hash.clone())
            && row.get::<Option<String>, _>("checkpoint_ref")
                == Some(prepared.checkpoint_ref.clone());
        if !matches {
            return Err(PersistenceError::InvariantViolation(
                "circuit generation identity collision".into(),
            ));
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO routing_circuit_generation_v3 (
             circuit_generation_id, scope, circuit_policy_revision,
             circuit_algorithm_version, status, input_circuit_event_watermark,
             input_circuit_event_hash, output_content_hash, checkpoint_ref,
             processed_event_count, created_at_ms, updated_at_ms
         ) VALUES (?1, 'station_key', ?2, ?3, 'building', ?4, ?5, ?6, ?7, 0, ?8, ?8)",
    )
    .bind(&prepared.circuit_generation_id)
    .bind(to_i64(request.circuit_policy_revision)?)
    .bind(CIRCUIT_REBUILD_ALGORITHM_VERSION)
    .bind(to_i64(request.input_circuit_event_watermark)?)
    .bind(&prepared.input_circuit_event_hash)
    .bind(&prepared.output_content_hash)
    .bind(&prepared.checkpoint_ref)
    .bind(request.evaluation_at_ms)
    .execute(write.connection())
    .await?;
    sqlx::query(
        "INSERT INTO routing_circuit_generation_v3_checkpoint (
             circuit_generation_id, input_circuit_event_watermark,
             processed_event_count, status, updated_at_ms
         ) VALUES (?1, ?2, 0, 'building', ?3)",
    )
    .bind(&prepared.circuit_generation_id)
    .bind(to_i64(request.input_circuit_event_watermark)?)
    .bind(request.evaluation_at_ms)
    .execute(write.connection())
    .await?;
    write.commit().await
}

async fn load_circuit_progress(
    runtime: &PersistenceHandle,
    circuit_generation_id: &str,
) -> Result<CircuitCheckpoint, PersistenceError> {
    let mut read = runtime.begin_read().await?;
    let row = sqlx::query(
        "SELECT cursor_station_key_id, cursor_station_key_lifecycle_revision,
                cursor_reducer_commit_sequence, cursor_event_id,
                processed_event_count
         FROM routing_circuit_generation_v3_checkpoint
         WHERE circuit_generation_id = ?1",
    )
    .bind(circuit_generation_id)
    .fetch_one(read.connection())
    .await?;
    let processed_event_count =
        nonnegative_u64(row.get("processed_event_count"), "processed event count")?;
    let station_key_id = row.get::<Option<String>, _>("cursor_station_key_id");
    let lifecycle_revision = row.get::<Option<i64>, _>("cursor_station_key_lifecycle_revision");
    let reducer_commit_sequence = row.get::<Option<i64>, _>("cursor_reducer_commit_sequence");
    let event_id = row.get::<Option<String>, _>("cursor_event_id");
    let cursor = match (
        station_key_id,
        lifecycle_revision,
        reducer_commit_sequence,
        event_id,
    ) {
        (None, None, None, None) if processed_event_count == 0 => None,
        (Some(station_key_id), Some(lifecycle_revision), Some(sequence), Some(event_id))
            if processed_event_count > 0 =>
        {
            Some(CircuitEventCursor {
                station_key_id,
                station_key_lifecycle_revision: positive_u64(
                    lifecycle_revision,
                    "checkpoint station_key_lifecycle_revision",
                )?,
                reducer_commit_sequence: positive_u64(
                    sequence,
                    "checkpoint reducer_commit_sequence",
                )?,
                event_id,
                effect_kind: String::new(),
            })
        }
        _ => {
            return Err(PersistenceError::InvariantViolation(
                "circuit generation checkpoint cursor is inconsistent".into(),
            ))
        }
    };
    Ok(CircuitCheckpoint {
        cursor,
        processed_event_count,
    })
}

async fn load_circuit_generation_state(
    connection: &mut sqlx::SqliteConnection,
    circuit_generation_id: &str,
    station_key_id: &str,
    station_key_lifecycle_revision: u64,
) -> Result<Option<CircuitGenerationState>, PersistenceError> {
    let row = sqlx::query(
        "SELECT station_key_id, station_key_lifecycle_revision, state,
                state_revision, consecutive_failures, reopen_level,
                opened_at_ms, cooldown_until_ms, recovery_successes,
                monotonic_clock_watermark_ms, reducer_commit_sequence
         FROM routing_circuit_state_generation_v3
         WHERE circuit_generation_id = ?1 AND station_key_id = ?2
           AND station_key_lifecycle_revision = ?3",
    )
    .bind(circuit_generation_id)
    .bind(station_key_id)
    .bind(to_i64(station_key_lifecycle_revision)?)
    .fetch_optional(&mut *connection)
    .await?;
    row.map(circuit_state_from_row).transpose()
}

async fn save_circuit_generation_state(
    connection: &mut sqlx::SqliteConnection,
    circuit_generation_id: &str,
    state: &CircuitGenerationState,
    now_ms: i64,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO routing_circuit_state_generation_v3 (
             circuit_generation_id, station_key_id,
             station_key_lifecycle_revision, state, state_revision,
             consecutive_failures, reopen_level, opened_at_ms,
             cooldown_until_ms, recovery_successes,
             monotonic_clock_watermark_ms, reducer_commit_sequence,
             updated_at_ms
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
         ON CONFLICT(
             circuit_generation_id, station_key_id,
             station_key_lifecycle_revision
         ) DO UPDATE SET state = excluded.state,
             state_revision = excluded.state_revision,
             consecutive_failures = excluded.consecutive_failures,
             reopen_level = excluded.reopen_level,
             opened_at_ms = excluded.opened_at_ms,
             cooldown_until_ms = excluded.cooldown_until_ms,
             recovery_successes = excluded.recovery_successes,
             monotonic_clock_watermark_ms = excluded.monotonic_clock_watermark_ms,
             reducer_commit_sequence = excluded.reducer_commit_sequence,
             updated_at_ms = excluded.updated_at_ms",
    )
    .bind(circuit_generation_id)
    .bind(&state.station_key_id)
    .bind(to_i64(state.station_key_lifecycle_revision)?)
    .bind(&state.state)
    .bind(to_i64(state.state_revision)?)
    .bind(i64::from(state.consecutive_failures))
    .bind(i64::from(state.reopen_level))
    .bind(state.opened_at_ms.map(to_i64).transpose()?)
    .bind(state.cooldown_until_ms.map(to_i64).transpose()?)
    .bind(i64::from(state.recovery_successes))
    .bind(to_i64(state.monotonic_clock_watermark_ms)?)
    .bind(to_i64(state.reducer_commit_sequence)?)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn finalize_circuit_generation(
    runtime: &PersistenceHandle,
    request: &CircuitGenerationBuildRequest,
    prepared: &PreparedCircuitOutput,
) -> Result<(), PersistenceError> {
    let mut write = runtime.begin_write().await?;
    let (persisted_hash, persisted_state_count) =
        hash_persisted_circuit_states(write.connection(), &prepared.circuit_generation_id).await?;
    let applied_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_circuit_event_applied_generation_v3
         WHERE circuit_generation_id = ?1",
    )
    .bind(&prepared.circuit_generation_id)
    .fetch_one(write.connection())
    .await?;
    if persisted_state_count != prepared.output_state_count
        || persisted_hash != prepared.output_content_hash
        || nonnegative_u64(applied_count, "applied event count")? != prepared.input_event_count
    {
        return Err(PersistenceError::InvariantViolation(
            "circuit generation output verification failed".into(),
        ));
    }
    let expected_count = to_i64(prepared.input_event_count)?;
    let updated = sqlx::query(
        "UPDATE routing_circuit_generation_v3
         SET status = 'ready', ready_at_ms = ?2, updated_at_ms = ?2
         WHERE circuit_generation_id = ?1 AND status = 'building'
           AND processed_event_count = ?3",
    )
    .bind(&prepared.circuit_generation_id)
    .bind(request.evaluation_at_ms)
    .bind(expected_count)
    .execute(write.connection())
    .await?
    .rows_affected();
    if updated == 0 {
        let status: String = sqlx::query_scalar(
            "SELECT status FROM routing_circuit_generation_v3
             WHERE circuit_generation_id = ?1",
        )
        .bind(&prepared.circuit_generation_id)
        .fetch_one(write.connection())
        .await?;
        if !matches!(status.as_str(), "ready" | "active" | "retired") {
            return Err(PersistenceError::RevisionConflict(
                "routing_circuit_generation_v3".into(),
            ));
        }
    }
    sqlx::query(
        "UPDATE routing_circuit_generation_v3_checkpoint
         SET status = 'ready', updated_at_ms = ?2
         WHERE circuit_generation_id = ?1 AND processed_event_count = ?3",
    )
    .bind(&prepared.circuit_generation_id)
    .bind(request.evaluation_at_ms)
    .bind(expected_count)
    .execute(write.connection())
    .await?;
    write.commit().await
}

async fn hash_persisted_circuit_states(
    connection: &mut sqlx::SqliteConnection,
    circuit_generation_id: &str,
) -> Result<(String, u64), PersistenceError> {
    let mut hasher = Sha256::new();
    hasher.update(b"[");
    let mut first = true;
    let mut count = 0_u64;
    let mut station_key_cursor: Option<String> = None;
    let mut lifecycle_cursor: Option<u64> = None;
    loop {
        let rows = sqlx::query(
            "SELECT station_key_id, station_key_lifecycle_revision, state,
                    state_revision, consecutive_failures, reopen_level,
                    opened_at_ms, cooldown_until_ms, recovery_successes,
                    monotonic_clock_watermark_ms, reducer_commit_sequence
             FROM routing_circuit_state_generation_v3
             WHERE circuit_generation_id = ?1
               AND (
                    ?2 IS NULL OR station_key_id > ?2
                    OR (station_key_id = ?2
                        AND station_key_lifecycle_revision > ?3)
               )
             ORDER BY station_key_id, station_key_lifecycle_revision
             LIMIT ?4",
        )
        .bind(circuit_generation_id)
        .bind(station_key_cursor.as_deref())
        .bind(lifecycle_cursor.map(to_i64).transpose()?)
        .bind(
            i64::try_from(ROUTING_GENERATION_REBUILD_BATCH_SIZE)
                .map_err(|_| PersistenceError::ConstraintViolation)?,
        )
        .fetch_all(&mut *connection)
        .await?;
        if rows.is_empty() {
            break;
        }
        let states = rows
            .into_iter()
            .map(circuit_state_from_row)
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        for state in &states {
            append_canonical_item(&mut hasher, state, &mut first)?;
            count = count.saturating_add(1);
        }
        let last = states.last().ok_or_else(|| {
            PersistenceError::InvariantViolation("persisted circuit cursor is missing".into())
        })?;
        station_key_cursor = Some(last.station_key_id.clone());
        lifecycle_cursor = Some(last.station_key_lifecycle_revision);
    }
    hasher.update(b"]");
    Ok((digest_hex(&hasher.finalize()), count))
}

fn circuit_state_from_row(
    row: sqlx::sqlite::SqliteRow,
) -> Result<CircuitGenerationState, PersistenceError> {
    Ok(CircuitGenerationState {
        station_key_id: row.get("station_key_id"),
        station_key_lifecycle_revision: positive_u64(
            row.get("station_key_lifecycle_revision"),
            "station_key_lifecycle_revision",
        )?,
        state: row.get("state"),
        state_revision: positive_u64(row.get("state_revision"), "state_revision")?,
        consecutive_failures: u16::try_from(row.get::<i64, _>("consecutive_failures"))
            .map_err(|_| PersistenceError::ConstraintViolation)?,
        reopen_level: u32::try_from(row.get::<i64, _>("reopen_level"))
            .map_err(|_| PersistenceError::ConstraintViolation)?,
        opened_at_ms: row
            .get::<Option<i64>, _>("opened_at_ms")
            .map(|value| nonnegative_u64(value, "opened_at_ms"))
            .transpose()?,
        cooldown_until_ms: row
            .get::<Option<i64>, _>("cooldown_until_ms")
            .map(|value| nonnegative_u64(value, "cooldown_until_ms"))
            .transpose()?,
        recovery_successes: u16::try_from(row.get::<i64, _>("recovery_successes"))
            .map_err(|_| PersistenceError::ConstraintViolation)?,
        monotonic_clock_watermark_ms: nonnegative_u64(
            row.get("monotonic_clock_watermark_ms"),
            "monotonic_clock_watermark_ms",
        )?,
        reducer_commit_sequence: nonnegative_u64(
            row.get("reducer_commit_sequence"),
            "reducer_commit_sequence",
        )?,
    })
}

fn validate_circuit_request(
    request: &CircuitGenerationBuildRequest,
) -> Result<(), PersistenceError> {
    if request.circuit_policy_revision == 0
        || request.consecutive_failure_threshold == 0
        || request.recovery_success_threshold == 0
        || request.recovery_wait_ms == 0
        || request.max_cooldown_ms < request.recovery_wait_ms
        || request.evaluation_at_ms < 0
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn positive_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    let value = nonnegative_u64(value, field)?;
    if value == 0 {
        return Err(PersistenceError::InvariantViolation(format!(
            "circuit rebuild {field} is zero"
        )));
    }
    Ok(value)
}

fn nonnegative_u64(value: i64, field: &str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| {
        PersistenceError::InvariantViolation(format!("circuit rebuild {field} is negative"))
    })
}

fn to_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| PersistenceError::ConstraintViolation)
}

fn validate_quality_request(
    request: &QualityGenerationBuildRequest,
) -> Result<(), PersistenceError> {
    if request.evaluation_at_ms < 0
        || request.config.quality_policy_revision == 0
        || request.next_observation_watermark > request.input_observation_watermark
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}

fn canonical_hash<T: Serialize + ?Sized>(value: &T) -> Result<String, PersistenceError> {
    let value = serde_json::to_value(value)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    let bytes = canonical_json_bytes(&value)?;
    Ok(sha256_hex(&bytes))
}

fn append_canonical_item<T: Serialize>(
    hasher: &mut Sha256,
    value: &T,
    first: &mut bool,
) -> Result<(), PersistenceError> {
    if !*first {
        hasher.update(b",");
    }
    let value = serde_json::to_value(value)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?;
    hasher.update(canonical_json_bytes(&value)?);
    *first = false;
    Ok(())
}

fn digest_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

fn canonical_json_bytes(value: &Value) -> Result<Vec<u8>, PersistenceError> {
    fn write(value: &Value, output: &mut Vec<u8>) -> Result<(), PersistenceError> {
        match value {
            Value::Object(object) => {
                output.push(b'{');
                let mut keys = object.keys().collect::<Vec<_>>();
                keys.sort();
                for (index, key) in keys.into_iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    output.extend(serde_json::to_vec(key).map_err(|error| {
                        PersistenceError::InvariantViolation(error.to_string())
                    })?);
                    output.push(b':');
                    write(&object[key], output)?;
                }
                output.push(b'}');
            }
            Value::Array(array) => {
                output.push(b'[');
                for (index, item) in array.iter().enumerate() {
                    if index > 0 {
                        output.push(b',');
                    }
                    write(item, output)?;
                }
                output.push(b']');
            }
            _ => output.extend(
                serde_json::to_vec(value)
                    .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))?,
            ),
        }
        Ok(())
    }
    let mut output = Vec::new();
    write(value, &mut output)?;
    Ok(output)
}

fn quality_result(
    prepared: &PreparedQualityOutput,
    processed_scope_count: u64,
    complete: bool,
) -> QualityGenerationBuildResult {
    QualityGenerationBuildResult {
        quality_generation_id: prepared.quality_generation_id.clone(),
        input_observation_hash: prepared.input_observation_hash.clone(),
        output_content_hash: prepared.output_content_hash.clone(),
        checkpoint_ref: prepared.checkpoint_ref.clone(),
        processed_scope_count,
        complete,
    }
}

fn circuit_result(
    prepared: &PreparedCircuitOutput,
    processed_event_count: u64,
    complete: bool,
) -> CircuitGenerationBuildResult {
    CircuitGenerationBuildResult {
        circuit_generation_id: prepared.circuit_generation_id.clone(),
        input_circuit_event_hash: prepared.input_circuit_event_hash.clone(),
        output_content_hash: prepared.output_content_hash.clone(),
        checkpoint_ref: prepared.checkpoint_ref.clone(),
        processed_event_count,
        complete,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[tokio::test]
    async fn quality_generation_resume_reuses_durable_preparation_without_rescan() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &root.path().join("quality-resume-without-rescan.sqlite3"),
        )
        .await
        .expect("initialize runtime");
        let handle = runtime.handle();
        let request = QualityGenerationBuildRequest {
            input_observation_watermark: 0,
            next_observation_watermark: 0,
            evaluation_at_ms: 987_654_321,
            config: QualityProjectionConfig::default(),
        };
        assert_eq!(
            quality_preparation_scan_count(&request).expect("scan count"),
            0
        );

        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let pending = RoutingGenerationRebuilder
            .rebuild_quality_generation(&handle, request.clone(), &cancelled)
            .await
            .expect("prepare cancelled quality build");
        assert!(!pending.complete);
        assert_eq!(
            quality_preparation_scan_count(&request).expect("scan count"),
            1
        );

        let completed = RoutingGenerationRebuilder
            .rebuild_quality_generation(&handle, request.clone(), &CancellationToken::new())
            .await
            .expect("resume quality build");
        assert!(completed.complete);
        assert_eq!(
            pending.quality_generation_id,
            completed.quality_generation_id
        );
        assert_eq!(
            pending.input_observation_hash,
            completed.input_observation_hash
        );
        assert_eq!(pending.output_content_hash, completed.output_content_hash);
        assert_eq!(
            quality_preparation_scan_count(&request).expect("scan count"),
            1
        );

        let build_request_hash = quality_build_request_hash(&request).expect("build request hash");
        let mut read = handle.begin_read().await.expect("generation read");
        let generation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_quality_generation_v3
             WHERE build_request_hash = ?1",
        )
        .bind(build_request_hash)
        .fetch_one(read.connection())
        .await
        .expect("generation count");
        assert_eq!(generation_count, 1);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn quality_generation_keeps_its_frozen_key_and_monitor_profile_snapshot() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(
            &root.path().join("quality-profile-snapshot.sqlite3"),
        )
        .await
        .expect("initialize runtime");
        let handle = runtime.handle();
        let mut write = handle.begin_write().await.expect("seed write");
        sqlx::query(
            "INSERT INTO stations (
                 id, name, station_type, website_url, api_base_url,
                 enabled, created_at, updated_at
             ) VALUES (
                 'snapshot-station', 'Snapshot station', 'openai-compatible',
                 'https://snapshot.invalid', 'https://snapshot.invalid/v1',
                 1, '1', '1'
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert station");
        sqlx::query(
            "INSERT INTO station_keys (
                 id, station_id, name, api_key, enabled, created_at, updated_at
             ) VALUES (
                 'snapshot-key', 'snapshot-station', 'Snapshot key',
                 'test-api-key-not-a-secret', 1, '1', '1'
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert key");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES ('station_key:snapshot-key', 1, 1, 'transactional_write')",
        )
        .execute(write.connection())
        .await
        .expect("insert lifecycle revision");
        sqlx::query(
            "INSERT INTO channel_monitor_request_templates (
                 id, name, endpoint_kind, method, path, request_body_json,
                 enabled, built_in, created_at, updated_at
             ) VALUES (
                 'snapshot-template', 'Snapshot template', 'chat', 'POST',
                 '/v1/chat/completions', '{}', 1, 0, '1', '1'
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert monitor template");
        sqlx::query(
            "INSERT INTO channel_monitors (
                 id, name, target_type, station_id, station_key_id,
                 template_id, enabled, interval_seconds, timeout_seconds,
                 created_at, updated_at
             ) VALUES (
                 'snapshot-monitor', 'Snapshot monitor', 'station_key',
                 'snapshot-station', 'snapshot-key', 'snapshot-template',
                 1, 60, 30, '1', '1'
             )",
        )
        .execute(write.connection())
        .await
        .expect("insert monitor");
        write.commit().await.expect("commit seed");

        let request = QualityGenerationBuildRequest {
            input_observation_watermark: 0,
            next_observation_watermark: 0,
            evaluation_at_ms: 10_000,
            config: QualityProjectionConfig::default(),
        };
        let cancellation = CancellationToken::new();
        cancellation.cancel();
        let pending = RoutingGenerationRebuilder
            .rebuild_quality_generation(&handle, request.clone(), &cancellation)
            .await
            .expect("create quality generation");
        assert!(!pending.complete);

        let mut write = handle.begin_write().await.expect("disable monitor write");
        let snapshot_id: String = sqlx::query_scalar(
            "SELECT source_profile_snapshot_id
             FROM routing_quality_generation_v3
             WHERE quality_generation_id = ?1",
        )
        .bind(&pending.quality_generation_id)
        .fetch_one(write.connection())
        .await
        .expect("generation snapshot id");
        let frozen: (i64, Option<String>) = sqlx::query_as(
            "SELECT monitoring_source_eligible, monitoring_profile_commitment
             FROM routing_quality_source_profile_snapshot_item_v3
             WHERE snapshot_id = ?1 AND station_key_id = 'snapshot-key'",
        )
        .bind(&snapshot_id)
        .fetch_one(write.connection())
        .await
        .expect("frozen snapshot item");
        assert_eq!(frozen.0, 1);
        assert_eq!(frozen.1.as_deref().map(str::len), Some(64));
        sqlx::query(
            "INSERT INTO routing_quality_lifecycle_alias_v1 (
                 station_key_id, target_lifecycle_revision, reason_code, created_at_ms
             ) VALUES ('snapshot-key', 2, 'group_rate_projection_lifecycle_drift', 2)",
        )
        .execute(write.connection())
        .await
        .expect("insert lifecycle alias");
        let effective_context =
            load_snapshot_key_context_batch(write.connection(), &snapshot_id, None, 10)
                .await
                .expect("load aliased snapshot context");
        assert_eq!(effective_context.len(), 1);
        assert_eq!(effective_context[0].lifecycle_revision, 2);
        sqlx::query(
            "UPDATE channel_monitors SET enabled = 0, updated_at = '2'
             WHERE id = 'snapshot-monitor'",
        )
        .execute(write.connection())
        .await
        .expect("disable monitor");
        write.commit().await.expect("commit monitor mutation");

        let replay_snapshot_id = ensure_source_profile_snapshot(&handle, &request)
            .await
            .expect("reload frozen snapshot");
        assert_eq!(replay_snapshot_id, snapshot_id);
        let mut read = handle.begin_read().await.expect("snapshot read");
        let after_mutation: (i64, Option<String>) = sqlx::query_as(
            "SELECT monitoring_source_eligible, monitoring_profile_commitment
             FROM routing_quality_source_profile_snapshot_item_v3
             WHERE snapshot_id = ?1 AND station_key_id = 'snapshot-key'",
        )
        .bind(&snapshot_id)
        .fetch_one(read.connection())
        .await
        .expect("snapshot after mutation");
        let generation_snapshot_id: String = sqlx::query_scalar(
            "SELECT source_profile_snapshot_id
             FROM routing_quality_generation_v3
             WHERE quality_generation_id = ?1",
        )
        .bind(&pending.quality_generation_id)
        .fetch_one(read.connection())
        .await
        .expect("generation snapshot after mutation");
        assert_eq!(after_mutation, frozen);
        assert_eq!(generation_snapshot_id, snapshot_id);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn circuit_rebuild_streams_multiple_batches_and_resumes_a_building_generation() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("circuit-rebuild.sqlite3"))
                .await
                .expect("initialize runtime");
        let handle = runtime.handle();
        let watermark = {
            let mut write = handle.begin_write().await.expect("begin event write");
            for sequence in 1_i64..=130 {
                sqlx::query(
                    "INSERT INTO routing_circuit_event_v3 (
                         event_id, effect_kind, source, attempt_id, station_key_id,
                         station_key_lifecycle_revision, reducer_commit_sequence,
                         policy_revision, expected_state_revision, occurred_at_ms,
                         canonical_outcome, failure_code, recovery_origin,
                         retry_disposition, boundary_crossed, created_at_ms
                     ) VALUES (
                         ?1, 'circuit', 'real_request', ?2, 'key-stream',
                         1, ?3, 1, ?3, ?3, 'attributable_failure',
                         'upstream_rate_limited', 'normal',
                         'retryable_before_commit', 1, ?3
                     )",
                )
                .bind(format!("stream-event-{sequence}"))
                .bind(format!("stream-attempt-{sequence}"))
                .bind(sequence)
                .execute(write.connection())
                .await
                .expect("insert circuit event");
            }
            let watermark: i64 =
                sqlx::query_scalar("SELECT MAX(ingestion_sequence) FROM routing_circuit_event_v3")
                    .fetch_one(write.connection())
                    .await
                    .expect("circuit watermark");
            write.commit().await.expect("commit circuit events");
            u64::try_from(watermark).expect("positive watermark")
        };
        let request = CircuitGenerationBuildRequest {
            input_circuit_event_watermark: watermark,
            circuit_policy_revision: 1,
            consecutive_failure_threshold: 3,
            recovery_success_threshold: 2,
            recovery_wait_ms: 1_000,
            max_cooldown_ms: 60_000,
            evaluation_at_ms: 1_000,
        };
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let pending = RoutingGenerationRebuilder
            .rebuild_circuit_generation(&handle, request.clone(), &cancelled)
            .await
            .expect("create resumable generation");
        assert!(!pending.complete);
        assert_eq!(pending.processed_event_count, 0);

        let completed = RoutingGenerationRebuilder
            .rebuild_circuit_generation(&handle, request.clone(), &CancellationToken::new())
            .await
            .expect("resume circuit generation");
        assert!(completed.complete);
        assert_eq!(completed.processed_event_count, 130);
        let verified = RoutingGenerationRebuilder
            .verify_circuit_generation(&handle, &request, &completed)
            .await
            .expect("verify deterministic replay");
        assert_eq!(verified.input_event_count, 130);
        assert_eq!(verified.output_state_count, 1);

        let mut read = handle.begin_read().await.expect("begin verification read");
        let checkpoint: i64 = sqlx::query_scalar(
            "SELECT processed_event_count
             FROM routing_circuit_generation_v3_checkpoint
             WHERE circuit_generation_id = ?1",
        )
        .bind(&completed.circuit_generation_id)
        .fetch_one(read.connection())
        .await
        .expect("checkpoint count");
        let applied: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_circuit_event_applied_generation_v3
             WHERE circuit_generation_id = ?1",
        )
        .bind(&completed.circuit_generation_id)
        .fetch_one(read.connection())
        .await
        .expect("applied count");
        let reducer_sequence: i64 = sqlx::query_scalar(
            "SELECT reducer_commit_sequence
             FROM routing_circuit_state_generation_v3
             WHERE circuit_generation_id = ?1 AND station_key_id = 'key-stream'",
        )
        .bind(&completed.circuit_generation_id)
        .fetch_one(read.connection())
        .await
        .expect("reducer sequence");
        drop(read);
        assert_eq!(checkpoint, 130);
        assert_eq!(applied, 130);
        assert_eq!(reducer_sequence, 130);
        runtime.close().await.expect("close runtime");
    }
}
