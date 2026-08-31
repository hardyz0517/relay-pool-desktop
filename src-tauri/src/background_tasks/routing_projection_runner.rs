use sqlx::Row;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor};

use crate::{
    application::quality_projection::{
        rebuild_quality_summary_v3_at, QualityProjectionConfig, QUALITY_PROJECTOR_VERSION,
    },
    models::routing_observation::RoutingObservation,
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            routing_generation_store::RoutingGenerationStore,
            routing_observation_store::RoutingObservationStore,
            routing_quality_store::{
                RoutingQualityStore, ROUTING_QUALITY_CURSOR_SCOPE, ROUTING_QUALITY_PROJECTOR_ID,
            },
        },
    },
};

#[cfg(test)]
use crate::application::quality_projection::QualitySummary;
#[cfg(test)]
use crate::application::quality_projection::{rebuild_quality_summary_with_checkpoint, BetaPrior};

pub(crate) const MAX_ROUTING_PROJECTION_BATCH: usize = 256;
pub const ROUTING_PROJECTION_TASK_ID: &str = ROUTING_QUALITY_PROJECTOR_ID;
const STALE_REFRESH_INTERVAL_MS: i64 = 60_000;
const MAX_PROJECTION_SCOPES: usize = 1_024;
const RAW_EVENT_RETENTION_WINDOW_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
const RAW_EVENT_RETENTION_INTERVAL_MS: i64 = 60 * 60 * 1_000;
const RAW_EVENT_RETENTION_BUSY_BUDGET_MS: u64 = 250;
const RAW_EVENT_RETENTION_BATCH_SIZE: i64 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RoutingRawEventRetentionReport {
    observation_safe_sequence: u64,
    circuit_safe_sequence: u64,
    observations_deleted: u64,
    circuit_events_deleted: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct QualityKeyContext {
    /// `Some(0)` is an explicit unavailable-context sentinel.  It prevents
    /// the projector's test-friendly `None` default from accidentally mixing
    /// observations from an unknown/deleted key lifecycle in production.
    lifecycle_revision: Option<u64>,
    real_source_eligible: bool,
    monitoring_source_eligible: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RoutingProjectionBatch {
    pub(crate) summaries: Vec<QualitySummary>,
    pub(crate) processed: usize,
    pub(crate) cancelled: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct RoutingProjectionRunner;

#[cfg(test)]
impl RoutingProjectionRunner {
    pub(crate) fn project_batch(
        &self,
        scopes: &[String],
        observations: &[RoutingObservation],
        prior: BetaPrior,
        cancellation: &CancellationToken,
    ) -> RoutingProjectionBatch {
        let checkpoint_sequence = observations
            .iter()
            .map(|observation| observation.order.ingested_at_ms)
            .max()
            .and_then(|value| u64::try_from(value).ok())
            .unwrap_or(0);
        let mut summaries = Vec::with_capacity(scopes.len());
        let mut processed = 0;
        for scope in scopes.iter().take(MAX_ROUTING_PROJECTION_BATCH) {
            if cancellation.is_cancelled() {
                return RoutingProjectionBatch {
                    summaries,
                    processed,
                    cancelled: true,
                };
            }
            summaries.push(rebuild_quality_summary_with_checkpoint(
                scope,
                observations,
                prior,
                checkpoint_sequence,
            ));
            processed += 1;
        }
        RoutingProjectionBatch {
            summaries,
            processed,
            cancelled: false,
        }
    }
}

/// Register the single ordered projector owner. The task is intentionally
/// cancellation-first: projection work may be resumed from durable
/// checkpoints on the next tick, while shutdown never leaves a worker behind.
pub fn register_routing_projection_task(
    supervisor: &TaskSupervisor,
    runtime: PersistenceHandle,
) -> Result<TaskId, String> {
    let task_id = TaskId::from(ROUTING_PROJECTION_TASK_ID);
    supervisor
        .register(
            TaskSpec::new(task_id.clone(), "routing_projection_v1", move |context: TaskRunContext| {
                let runtime = runtime.clone();
                Box::pin(async move {
                    let mut last_stale_refresh_ms = 0_i64;
                    let mut last_retention_ms = 0_i64;
                    loop {
                        tokio::select! {
                            _ = context.cancellation_token.cancelled() => return Err(TaskFailure::cancelled()),
                            _ = tokio::time::sleep(Duration::from_millis(1_000)) => {}
                        }
                        let now_ms = chrono::Utc::now().timestamp_millis().max(0);
                        let refresh_stale = now_ms.saturating_sub(last_stale_refresh_ms)
                            >= STALE_REFRESH_INTERVAL_MS;
                        if refresh_stale {
                            last_stale_refresh_ms = now_ms;
                        }
                        if let Err(_error) = project_once(
                            &runtime,
                            &context.cancellation_token,
                            refresh_stale,
                        )
                        .await
                        {
                            crate::observability::runtime::bootstrap::emit(
                                crate::services::proxy::runtime_events::routing_projection_tick_failed(),
                            );
                        }
                        let retention_due = now_ms.saturating_sub(last_retention_ms)
                            >= RAW_EVENT_RETENTION_INTERVAL_MS;
                        if retention_due {
                            last_retention_ms = now_ms;
                            let retention = tokio::time::timeout(
                                Duration::from_millis(RAW_EVENT_RETENTION_BUSY_BUDGET_MS),
                                retain_raw_events_once(
                                    &runtime,
                                    &context.cancellation_token,
                                    now_ms,
                                ),
                            )
                            .await;
                            let failure_code = match retention {
                                Ok(Ok(_)) => None,
                                Ok(Err(ref error)) => Some(retention_error_code(error)),
                                Err(_) => Some("busy_budget_exceeded"),
                            };
                            if let Some(failure_code) = failure_code {
                                let _ = tokio::time::timeout(
                                    Duration::from_millis(RAW_EVENT_RETENTION_BUSY_BUDGET_MS),
                                    record_retention_failure(&runtime, now_ms, failure_code),
                                )
                                .await;
                                crate::observability::runtime::bootstrap::emit(
                                    crate::services::proxy::runtime_events::routing_projection_tick_failed(),
                                );
                            }
                        }
                    }
                })
            })
            .with_concurrency_key("routing-projection-v1")
            .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

async fn record_retention_failure(
    runtime: &PersistenceHandle,
    now_ms: i64,
    failure_code: &'static str,
) -> Result<(), crate::persistence::error::PersistenceError> {
    let now_ms = now_ms.max(0);
    let cutoff_at_ms = now_ms.saturating_sub(RAW_EVENT_RETENTION_WINDOW_MS);
    let mut write = runtime.begin_write().await?;
    sqlx::query(
        "INSERT INTO routing_raw_event_retention_run (
             started_at_ms, finished_at_ms, cutoff_at_ms,
             observation_safe_sequence, circuit_safe_sequence,
             observations_deleted, circuit_events_deleted, status, error_code)
         VALUES (?1, ?1, ?2, 0, 0, 0, 0, 'failed', ?3)",
    )
    .bind(now_ms)
    .bind(cutoff_at_ms)
    .bind(failure_code)
    .execute(write.connection())
    .await?;
    write.commit().await
}

fn retention_error_code(error: &crate::persistence::error::PersistenceError) -> &'static str {
    use crate::persistence::error::PersistenceError;
    match error {
        PersistenceError::DatabaseBusy => "database_busy",
        PersistenceError::ConstraintViolation => "constraint_violation",
        PersistenceError::InvariantViolation(_) => "invariant_violation",
        PersistenceError::RuntimeUnavailable | PersistenceError::SessionClosed => {
            "runtime_unavailable"
        }
        _ => "database_failed",
    }
}

async fn retain_raw_events_once(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
    now_ms: i64,
) -> Result<RoutingRawEventRetentionReport, crate::persistence::error::PersistenceError> {
    if cancellation.is_cancelled() {
        return Ok(RoutingRawEventRetentionReport {
            observation_safe_sequence: 0,
            circuit_safe_sequence: 0,
            observations_deleted: 0,
            circuit_events_deleted: 0,
        });
    }
    let started_at_ms = now_ms.max(0);
    let cutoff_at_ms = started_at_ms.saturating_sub(RAW_EVENT_RETENTION_WINDOW_MS);
    let mut write = runtime.begin_write().await?;
    let observation_safe_sequence =
        load_observation_retention_safe_sequence(write.connection()).await?;
    let circuit_safe_sequence = load_circuit_retention_safe_sequence(write.connection()).await?;
    if cancellation.is_cancelled() {
        return Ok(RoutingRawEventRetentionReport {
            observation_safe_sequence,
            circuit_safe_sequence,
            observations_deleted: 0,
            circuit_events_deleted: 0,
        });
    }

    roll_up_observation_retention_batch(
        write.connection(),
        cutoff_at_ms,
        observation_safe_sequence,
        started_at_ms,
    )
    .await?;
    let observations_deleted = delete_observation_retention_batch(
        write.connection(),
        cutoff_at_ms,
        observation_safe_sequence,
    )
    .await?;
    roll_up_circuit_retention_batch(
        write.connection(),
        cutoff_at_ms,
        circuit_safe_sequence,
        started_at_ms,
    )
    .await?;
    let circuit_events_deleted =
        delete_circuit_retention_batch(write.connection(), cutoff_at_ms, circuit_safe_sequence)
            .await?;

    sqlx::query(
        "INSERT INTO routing_raw_event_retention_run (
             started_at_ms, finished_at_ms, cutoff_at_ms,
             observation_safe_sequence, circuit_safe_sequence,
             observations_deleted, circuit_events_deleted, status, error_code)
         VALUES (?1, ?1, ?2, ?3, ?4, ?5, ?6, 'succeeded', NULL)",
    )
    .bind(started_at_ms)
    .bind(cutoff_at_ms)
    .bind(to_i64_retention_sequence(observation_safe_sequence)?)
    .bind(to_i64_retention_sequence(circuit_safe_sequence)?)
    .bind(to_i64_retention_sequence(observations_deleted)?)
    .bind(to_i64_retention_sequence(circuit_events_deleted)?)
    .execute(write.connection())
    .await?;
    write.commit().await?;

    Ok(RoutingRawEventRetentionReport {
        observation_safe_sequence,
        circuit_safe_sequence,
        observations_deleted,
        circuit_events_deleted,
    })
}

async fn load_observation_retention_safe_sequence(
    connection: &mut sqlx::SqliteConnection,
) -> Result<u64, crate::persistence::error::PersistenceError> {
    let protected = sqlx::query_scalar::<_, Option<i64>>(
        "WITH latest_retired AS (
             SELECT quality_generation_id
             FROM routing_runtime_generation
             WHERE status = 'retired'
             ORDER BY retired_at_ms DESC, runtime_generation_id ASC
             LIMIT 1
         ), protected_runtime AS (
             SELECT quality_generation_id
             FROM routing_runtime_generation
             WHERE status IN ('building', 'ready', 'cutover_fencing', 'active')
             UNION
             SELECT quality_generation_id
             FROM latest_retired
         ), protected_sequences AS (
             SELECT COALESCE(incremental.checkpoint_sequence,
                             checkpoint.input_observation_watermark,
                             generation.input_observation_watermark, 0) AS safe_sequence
             FROM protected_runtime protected
             JOIN routing_quality_generation_v3 generation
               ON generation.quality_generation_id = protected.quality_generation_id
             LEFT JOIN routing_quality_generation_v3_checkpoint checkpoint
               ON checkpoint.quality_generation_id = generation.quality_generation_id
             LEFT JOIN routing_quality_incremental_checkpoint_v3 incremental
               ON incremental.quality_generation_id = generation.quality_generation_id
              AND incremental.projector = ?1
              AND incremental.projector_version = ?2
              AND incremental.scope = ?3
             UNION ALL
             SELECT COALESCE(incremental.checkpoint_sequence,
                             checkpoint.input_observation_watermark,
                             generation.input_observation_watermark, 0)
             FROM routing_quality_generation_v3 generation
             LEFT JOIN routing_quality_generation_v3_checkpoint checkpoint
               ON checkpoint.quality_generation_id = generation.quality_generation_id
             LEFT JOIN routing_quality_incremental_checkpoint_v3 incremental
               ON incremental.quality_generation_id = generation.quality_generation_id
              AND incremental.projector = ?1
              AND incremental.projector_version = ?2
              AND incremental.scope = ?3
             WHERE generation.status IN ('building', 'ready', 'active')
         )
         SELECT MIN(safe_sequence) FROM protected_sequences",
    )
    .bind(ROUTING_PROJECTION_TASK_ID)
    .bind(QUALITY_PROJECTOR_VERSION)
    .bind(ROUTING_QUALITY_CURSOR_SCOPE)
    .fetch_one(&mut *connection)
    .await?;
    retention_safe_sequence_or_max(connection, protected, "routing_observations").await
}

async fn load_circuit_retention_safe_sequence(
    connection: &mut sqlx::SqliteConnection,
) -> Result<u64, crate::persistence::error::PersistenceError> {
    let protected = sqlx::query_scalar::<_, Option<i64>>(
        "WITH latest_retired AS (
             SELECT circuit_generation_id
             FROM routing_runtime_generation
             WHERE status = 'retired'
             ORDER BY retired_at_ms DESC, runtime_generation_id ASC
             LIMIT 1
         ), protected_runtime AS (
             SELECT circuit_generation_id
             FROM routing_runtime_generation
             WHERE status IN ('building', 'ready', 'cutover_fencing', 'active')
             UNION
             SELECT circuit_generation_id
             FROM latest_retired
         ), protected_sequences AS (
             SELECT COALESCE(checkpoint.input_circuit_event_watermark,
                             generation.input_circuit_event_watermark, 0) AS safe_sequence
             FROM protected_runtime protected
             JOIN routing_circuit_generation_v3 generation
               ON generation.circuit_generation_id = protected.circuit_generation_id
             LEFT JOIN routing_circuit_generation_v3_checkpoint checkpoint
               ON checkpoint.circuit_generation_id = generation.circuit_generation_id
             UNION ALL
             SELECT COALESCE(checkpoint.input_circuit_event_watermark,
                             generation.input_circuit_event_watermark, 0)
             FROM routing_circuit_generation_v3 generation
             LEFT JOIN routing_circuit_generation_v3_checkpoint checkpoint
               ON checkpoint.circuit_generation_id = generation.circuit_generation_id
             WHERE generation.status IN ('building', 'ready', 'active')
         )
         SELECT MIN(safe_sequence) FROM protected_sequences",
    )
    .fetch_one(&mut *connection)
    .await?;
    retention_safe_sequence_or_max(connection, protected, "routing_circuit_event_v3").await
}

async fn retention_safe_sequence_or_max(
    connection: &mut sqlx::SqliteConnection,
    protected: Option<i64>,
    table: &'static str,
) -> Result<u64, crate::persistence::error::PersistenceError> {
    let value = match protected {
        Some(value) => value,
        None => {
            let sql = format!("SELECT COALESCE(MAX(ingestion_sequence), 0) FROM {table}");
            sqlx::query_scalar::<_, i64>(&sql)
                .fetch_one(&mut *connection)
                .await?
        }
    };
    u64::try_from(value).map_err(|_| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "routing retention safe sequence is negative".into(),
        )
    })
}

async fn roll_up_observation_retention_batch(
    connection: &mut sqlx::SqliteConnection,
    cutoff_at_ms: i64,
    safe_sequence: u64,
    now_ms: i64,
) -> Result<(), crate::persistence::error::PersistenceError> {
    sqlx::query(
        "WITH candidates AS (
             SELECT rowid
             FROM routing_observations
             WHERE ingestion_sequence IS NOT NULL
               AND ingestion_sequence <= ?1
               AND created_at_ms < ?2
               AND ingested_at_ms < ?2
               AND event_at_ms < ?2
             ORDER BY ingestion_sequence ASC
             LIMIT ?3
         )
         INSERT INTO routing_raw_event_retention_rollup (
             event_kind, source_kind, outcome_kind, bucket_start_ms,
             deleted_count, first_ingestion_sequence,
             last_ingestion_sequence, updated_at_ms)
         SELECT 'observation',
                COALESCE(NULLIF(source, ''), 'unknown'),
                COALESCE(NULLIF(outcome, ''), NULLIF(outcome_kind, ''), 'unknown'),
                (created_at_ms / 86400000) * 86400000,
                COUNT(*), MIN(ingestion_sequence), MAX(ingestion_sequence), ?4
         FROM routing_observations
         WHERE rowid IN (SELECT rowid FROM candidates)
         GROUP BY 2, 3, 4
         ON CONFLICT(event_kind, source_kind, outcome_kind, bucket_start_ms)
         DO UPDATE SET
             deleted_count = deleted_count + excluded.deleted_count,
             first_ingestion_sequence = MIN(first_ingestion_sequence,
                                            excluded.first_ingestion_sequence),
             last_ingestion_sequence = MAX(last_ingestion_sequence,
                                           excluded.last_ingestion_sequence),
             updated_at_ms = MAX(updated_at_ms, excluded.updated_at_ms)",
    )
    .bind(to_i64_retention_sequence(safe_sequence)?)
    .bind(cutoff_at_ms)
    .bind(RAW_EVENT_RETENTION_BATCH_SIZE)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn delete_observation_retention_batch(
    connection: &mut sqlx::SqliteConnection,
    cutoff_at_ms: i64,
    safe_sequence: u64,
) -> Result<u64, crate::persistence::error::PersistenceError> {
    Ok(sqlx::query(
        "DELETE FROM routing_observations
         WHERE rowid IN (
             SELECT rowid
             FROM routing_observations
             WHERE ingestion_sequence IS NOT NULL
               AND ingestion_sequence <= ?1
               AND created_at_ms < ?2
               AND ingested_at_ms < ?2
               AND event_at_ms < ?2
             ORDER BY ingestion_sequence ASC
             LIMIT ?3
         )",
    )
    .bind(to_i64_retention_sequence(safe_sequence)?)
    .bind(cutoff_at_ms)
    .bind(RAW_EVENT_RETENTION_BATCH_SIZE)
    .execute(&mut *connection)
    .await?
    .rows_affected())
}

async fn roll_up_circuit_retention_batch(
    connection: &mut sqlx::SqliteConnection,
    cutoff_at_ms: i64,
    safe_sequence: u64,
    now_ms: i64,
) -> Result<(), crate::persistence::error::PersistenceError> {
    sqlx::query(
        "WITH candidates AS (
             SELECT rowid
             FROM routing_circuit_event_v3
             WHERE ingestion_sequence IS NOT NULL
               AND ingestion_sequence <= ?1
               AND created_at_ms < ?2
               AND occurred_at_ms < ?2
             ORDER BY ingestion_sequence ASC
             LIMIT ?3
         )
         INSERT INTO routing_raw_event_retention_rollup (
             event_kind, source_kind, outcome_kind, bucket_start_ms,
             deleted_count, first_ingestion_sequence,
             last_ingestion_sequence, updated_at_ms)
         SELECT 'circuit', source, canonical_outcome,
                (created_at_ms / 86400000) * 86400000,
                COUNT(*), MIN(ingestion_sequence), MAX(ingestion_sequence), ?4
         FROM routing_circuit_event_v3
         WHERE rowid IN (SELECT rowid FROM candidates)
         GROUP BY 2, 3, 4
         ON CONFLICT(event_kind, source_kind, outcome_kind, bucket_start_ms)
         DO UPDATE SET
             deleted_count = deleted_count + excluded.deleted_count,
             first_ingestion_sequence = MIN(first_ingestion_sequence,
                                            excluded.first_ingestion_sequence),
             last_ingestion_sequence = MAX(last_ingestion_sequence,
                                           excluded.last_ingestion_sequence),
             updated_at_ms = MAX(updated_at_ms, excluded.updated_at_ms)",
    )
    .bind(to_i64_retention_sequence(safe_sequence)?)
    .bind(cutoff_at_ms)
    .bind(RAW_EVENT_RETENTION_BATCH_SIZE)
    .bind(now_ms)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn delete_circuit_retention_batch(
    connection: &mut sqlx::SqliteConnection,
    cutoff_at_ms: i64,
    safe_sequence: u64,
) -> Result<u64, crate::persistence::error::PersistenceError> {
    Ok(sqlx::query(
        "DELETE FROM routing_circuit_event_v3
         WHERE rowid IN (
             SELECT rowid
             FROM routing_circuit_event_v3
             WHERE ingestion_sequence IS NOT NULL
               AND ingestion_sequence <= ?1
               AND created_at_ms < ?2
               AND occurred_at_ms < ?2
             ORDER BY ingestion_sequence ASC
             LIMIT ?3
         )",
    )
    .bind(to_i64_retention_sequence(safe_sequence)?)
    .bind(cutoff_at_ms)
    .bind(RAW_EVENT_RETENTION_BATCH_SIZE)
    .execute(&mut *connection)
    .await?
    .rows_affected())
}

fn to_i64_retention_sequence(
    value: u64,
) -> Result<i64, crate::persistence::error::PersistenceError> {
    i64::try_from(value)
        .map_err(|_| crate::persistence::error::PersistenceError::ConstraintViolation)
}

async fn project_once(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
    refresh_stale: bool,
) -> Result<(), crate::persistence::error::PersistenceError> {
    if cancellation.is_cancelled() {
        return Ok(());
    }
    let observation_store = RoutingObservationStore;
    let quality_store = RoutingQualityStore;
    let now_ms = chrono::Utc::now().timestamp_millis().max(0);
    let (
        active_quality_generation_id,
        observations,
        scoped_histories,
        previous_checkpoint,
        quality_config,
        key_contexts,
    ) = {
        let mut read = runtime.begin_read().await?;
        let registry = RoutingGenerationStore
            .load_registry_snapshot(read.connection())
            .await?;
        let active_quality_generation_id = registry
            .active
            .as_ref()
            .map(|generation| generation.quality_generation_id.clone());
        let active_source_profile_snapshot_id = match active_quality_generation_id.as_deref() {
            Some(generation_id) => Some(
                sqlx::query_scalar::<_, Option<String>>(
                    "SELECT source_profile_snapshot_id
                     FROM routing_quality_generation_v3
                     WHERE quality_generation_id = ?1",
                )
                .bind(generation_id)
                .fetch_one(read.connection())
                .await?
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    crate::persistence::error::PersistenceError::InvariantViolation(
                        "active quality generation has no source-profile snapshot".into(),
                    )
                })?,
            ),
            None => None,
        };
        let active_quality_base_watermark = registry
            .active
            .as_ref()
            .map(|generation| generation.input_observation_watermark);
        let quality_config = load_quality_config(read.connection()).await?;
        let checkpoint = match active_quality_generation_id.as_deref() {
            Some(generation_id) => {
                quality_store
                    .load_generation_checkpoint_cursor(
                        read.connection(),
                        generation_id,
                        ROUTING_PROJECTION_TASK_ID,
                        QUALITY_PROJECTOR_VERSION,
                        ROUTING_QUALITY_CURSOR_SCOPE,
                    )
                    .await?
            }
            None => {
                quality_store
                    .load_checkpoint_cursor(
                        read.connection(),
                        ROUTING_PROJECTION_TASK_ID,
                        QUALITY_PROJECTOR_VERSION,
                        ROUTING_QUALITY_CURSOR_SCOPE,
                    )
                    .await?
            }
        }
        .map(
            |cursor| -> Result<_, crate::persistence::error::PersistenceError> {
                let sequence = i64::try_from(cursor.sequence).map_err(|_| {
                    crate::persistence::error::PersistenceError::InvariantViolation(
                        "routing projection checkpoint exceeds SQLite integer range".into(),
                    )
                })?;
                Ok((sequence, cursor.item_id.unwrap_or_default()))
            },
        )
        .transpose()?;
        let previous_checkpoint = checkpoint.clone();
        let observations = observation_store
            .list_after_v3(read.connection(), checkpoint, MAX_ROUTING_PROJECTION_BATCH)
            .await?;
        let mut scopes = observations
            .iter()
            .map(|row| observation_scope(&row.observation))
            .collect::<Vec<_>>();
        if refresh_stale {
            scopes.extend(match active_quality_generation_id.as_deref() {
                Some(generation_id) => {
                    quality_store
                        .list_generation_summary_scopes(read.connection(), generation_id)
                        .await?
                }
                None => quality_store.list_summary_scopes(read.connection()).await?,
            });
        }
        scopes.sort();
        scopes.dedup();
        scopes.truncate(MAX_PROJECTION_SCOPES);
        let key_contexts = load_quality_key_contexts(
            read.connection(),
            &scopes,
            active_source_profile_snapshot_id.as_deref(),
        )
        .await?;
        let mut histories = observation_store
            .list_for_scopes_v3(read.connection(), &scopes)
            .await?;
        if let Some(base_watermark) = active_quality_base_watermark {
            let base_history = observation_store
                .list_for_scopes_v3_through(read.connection(), &scopes, Some(base_watermark))
                .await?;
            let mut by_id = histories
                .into_iter()
                .map(|observation| (observation.id.clone(), observation))
                .collect::<std::collections::BTreeMap<_, _>>();
            for observation in base_history {
                by_id.entry(observation.id.clone()).or_insert(observation);
            }
            histories = by_id.into_values().collect();
            histories.sort_by(|left, right| {
                observation_scope(left)
                    .cmp(&observation_scope(right))
                    .then_with(|| left.order.ingested_at_ms.cmp(&right.order.ingested_at_ms))
                    .then_with(|| left.id.cmp(&right.id))
            });
        }
        let mut histories_by_scope =
            std::collections::BTreeMap::<String, Vec<RoutingObservation>>::new();
        for observation in histories {
            histories_by_scope
                .entry(observation_scope(&observation))
                .or_default()
                .push(observation);
        }
        for history in histories_by_scope.values_mut() {
            observation_store
                .apply_quality_lifecycle_aliases(read.connection(), history)
                .await?;
        }
        let scoped_histories = scopes
            .into_iter()
            .map(|scope| {
                (
                    scope.clone(),
                    histories_by_scope.remove(&scope).unwrap_or_default(),
                )
            })
            .collect::<Vec<_>>();
        (
            active_quality_generation_id,
            observations,
            scoped_histories,
            previous_checkpoint,
            quality_config,
            key_contexts,
        )
    };
    if scoped_histories.is_empty() {
        return Ok(());
    }
    let ingestion_cursor = observations
        .last()
        .map(|row| (row.ingestion_sequence, row.observation.id.clone()));
    let ingestion_checkpoint = ingestion_cursor
        .as_ref()
        .map(|cursor| cursor.0)
        .or(previous_checkpoint.and_then(|cursor| u64::try_from(cursor.0).ok()))
        .unwrap_or(1);
    let mut write = runtime.begin_write().await?;
    for (scope, history) in scoped_histories {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let context = key_contexts
            .get(&scope)
            .copied()
            .unwrap_or(QualityKeyContext {
                lifecycle_revision: Some(0),
                real_source_eligible: false,
                monitoring_source_eligible: false,
            });
        let mut key_config = quality_config;
        key_config.current_lifecycle_revision = context.lifecycle_revision;
        key_config.real_source_eligible = context.real_source_eligible;
        key_config.monitoring_source_eligible = context.monitoring_source_eligible;
        let summary = rebuild_quality_summary_v3_at(
            &scope,
            &history,
            key_config,
            ingestion_checkpoint,
            now_ms,
        );
        let json = serde_json::to_value(&summary).map_err(|error| {
            crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
        })?;
        if let Some(generation_id) = active_quality_generation_id.as_deref() {
            let Some(lifecycle_revision) = context.lifecycle_revision.filter(|value| *value > 0)
            else {
                continue;
            };
            let Some(station_key_id) = summary.scope.strip_prefix("station_key:") else {
                continue;
            };
            quality_store
                .save_generation_summary(
                    write.connection(),
                    generation_id,
                    &summary.scope,
                    station_key_id,
                    lifecycle_revision,
                    summary.checkpoint_sequence.max(1),
                    &json,
                    now_ms,
                )
                .await?;
            for (axis, value) in [
                ("reliability", summary.reliability_basis_points),
                ("latency", summary.responsiveness_basis_points),
            ] {
                quality_store
                    .save_generation_health_axis(
                        write.connection(),
                        generation_id,
                        &summary.scope,
                        station_key_id,
                        lifecycle_revision,
                        axis,
                        summary.checkpoint_sequence.max(1),
                        value,
                        now_ms,
                    )
                    .await?;
            }
            quality_store
                .save_generation_checkpoint(
                    write.connection(),
                    generation_id,
                    ROUTING_PROJECTION_TASK_ID,
                    QUALITY_PROJECTOR_VERSION,
                    &summary.scope,
                    summary.checkpoint_sequence.max(1),
                    "ready",
                    None,
                    None,
                    now_ms,
                )
                .await?;
        } else {
            quality_store
                .save_summary(
                    write.connection(),
                    &summary.scope,
                    summary.checkpoint_sequence.max(1),
                    &json,
                    now_ms,
                )
                .await?;
            quality_store
                .save_health_axis(
                    write.connection(),
                    &summary.scope,
                    "reliability",
                    summary.checkpoint_sequence.max(1),
                    summary.reliability_basis_points,
                    now_ms,
                )
                .await?;
            quality_store
                .save_health_axis(
                    write.connection(),
                    &summary.scope,
                    "latency",
                    summary.checkpoint_sequence.max(1),
                    summary.responsiveness_basis_points,
                    now_ms,
                )
                .await?;
            quality_store
                .save_checkpoint(
                    write.connection(),
                    ROUTING_PROJECTION_TASK_ID,
                    QUALITY_PROJECTOR_VERSION,
                    &summary.scope,
                    summary.checkpoint_sequence.max(1),
                    "ready",
                    None,
                    now_ms,
                )
                .await?;
        }
    }
    // The cursor is advanced in the same transaction as every derived row. A
    // failed projection therefore retries the exact uncommitted ingestion range
    // instead of starving all observations after the first batch.
    if let Some((sequence, item_id)) = ingestion_cursor {
        if let Some(generation_id) = active_quality_generation_id.as_deref() {
            quality_store
                .save_generation_checkpoint(
                    write.connection(),
                    generation_id,
                    ROUTING_PROJECTION_TASK_ID,
                    QUALITY_PROJECTOR_VERSION,
                    ROUTING_QUALITY_CURSOR_SCOPE,
                    sequence,
                    "ready",
                    Some(item_id.as_str()),
                    None,
                    now_ms,
                )
                .await?;
        } else {
            quality_store
                .save_checkpoint(
                    write.connection(),
                    ROUTING_PROJECTION_TASK_ID,
                    QUALITY_PROJECTOR_VERSION,
                    ROUTING_QUALITY_CURSOR_SCOPE,
                    sequence,
                    "ready",
                    Some(item_id.as_str()),
                    now_ms,
                )
                .await?;
        }
    }
    write.commit().await
}

fn quality_config_from_policy(value: &serde_json::Value) -> QualityProjectionConfig {
    let mut config = QualityProjectionConfig::default();
    let object = value.as_object();
    let sampling = object
        .and_then(|object| object.get("reliabilitySampling"))
        .and_then(serde_json::Value::as_object);
    let weights = object
        .and_then(|object| object.get("reliabilitySourceWeights"))
        .and_then(serde_json::Value::as_object);
    if let Some(value) = sampling
        .and_then(|value| value.get("recentMinimumSamples"))
        .and_then(serde_json::Value::as_u64)
    {
        config.recent_minimum_samples = value.clamp(1, 10_000);
    }
    if let Some(value) = sampling
        .and_then(|value| value.get("historicalMinimumSamples"))
        .and_then(serde_json::Value::as_u64)
    {
        config.historical_minimum_samples = value.clamp(1, 10_000);
    }
    if let Some(value) = sampling
        .and_then(|value| value.get("optimisticReliabilityPercent"))
        .and_then(serde_json::Value::as_u64)
    {
        config.optimistic_reliability_basis_points = (value.min(100) * 100) as u16;
    }
    if let Some(value) = sampling
        .and_then(|value| value.get("optimisticLatencyMs"))
        .and_then(serde_json::Value::as_u64)
    {
        config.optimistic_latency_ms = value.clamp(100, 120_000) as u32;
    }
    if let Some(value) = weights
        .and_then(|value| value.get("realTrafficPercent"))
        .and_then(serde_json::Value::as_u64)
    {
        config.real_traffic_weight_basis_points = (value.min(100) * 100) as u16;
    }
    if let Some(value) = weights
        .and_then(|value| value.get("monitoringPercent"))
        .and_then(serde_json::Value::as_u64)
    {
        config.monitoring_weight_basis_points = (value.min(100) * 100) as u16;
    }
    config
}

async fn load_quality_config(
    connection: &mut sqlx::SqliteConnection,
) -> Result<QualityProjectionConfig, crate::persistence::error::PersistenceError> {
    // Incremental projection is an active-generation consumer. A staged
    // policy is only an explicit input to the shadow rebuilder and must not
    // affect this read model before qualification and atomic cutover.
    let stored =
        crate::persistence::stores::routing_policy_v3_stage_upgrade::load_effective_active_in(
            connection,
        )
        .await?;
    let mut config = stored
        .as_ref()
        .map(|stored| quality_config_from_policy(&stored.config))
        .unwrap_or_default();
    if let Some(stored) = stored {
        config.quality_policy_revision = stored.revision.max(1);
    }
    Ok(config)
}

/// Load the current key binding and probe-profile facts in one read.  These
/// values are deliberately not inferred from observations: an old lifecycle
/// can remain in the immutable log after a key is replaced, and a key with no
/// monitor profile must not receive an invented 30% monitoring source.
async fn load_quality_key_contexts(
    connection: &mut sqlx::SqliteConnection,
    scopes: &[String],
    source_profile_snapshot_id: Option<&str>,
) -> Result<
    std::collections::BTreeMap<String, QualityKeyContext>,
    crate::persistence::error::PersistenceError,
> {
    let key_scopes = scopes
        .iter()
        .filter(|scope| scope.starts_with("station_key:") && scope.len() > "station_key:".len())
        .take(MAX_PROJECTION_SCOPES)
        .cloned()
        .collect::<Vec<_>>();
    if key_scopes.is_empty() {
        return Ok(std::collections::BTreeMap::new());
    }
    let placeholders = (1..=key_scopes.len())
        .map(|index| format!("?{index}"))
        .collect::<Vec<_>>()
        .join(",");
    if let Some(snapshot_id) = source_profile_snapshot_id {
        let sql = format!(
            "SELECT 'station_key:' || item.station_key_id AS scope,
                    CASE
                        WHEN alias.target_lifecycle_revision > item.station_key_lifecycle_revision
                        THEN alias.target_lifecycle_revision
                        ELSE item.station_key_lifecycle_revision
                    END AS lifecycle_revision,
                    item.real_source_eligible, item.monitoring_source_eligible
             FROM routing_quality_source_profile_snapshot_item_v3 item
             LEFT JOIN routing_quality_lifecycle_alias_v1 alias
               ON alias.station_key_id = item.station_key_id
             WHERE item.snapshot_id = ?1
               AND ('station_key:' || item.station_key_id) IN ({})
             ORDER BY item.station_key_id ASC",
            (2..=(key_scopes.len() + 1))
                .map(|index| format!("?{index}"))
                .collect::<Vec<_>>()
                .join(",")
        );
        let mut query = sqlx::query(&sql).bind(snapshot_id);
        for scope in &key_scopes {
            query = query.bind(scope);
        }
        let rows = query.fetch_all(&mut *connection).await?;
        let mut contexts = std::collections::BTreeMap::new();
        for row in rows {
            let revision =
                u64::try_from(row.get::<i64, _>("lifecycle_revision")).map_err(|_| {
                    crate::persistence::error::PersistenceError::InvariantViolation(
                        "snapshot station key lifecycle revision is negative".into(),
                    )
                })?;
            contexts.insert(
                row.get::<String, _>("scope"),
                QualityKeyContext {
                    lifecycle_revision: Some(revision),
                    real_source_eligible: row.get::<i64, _>("real_source_eligible") != 0,
                    monitoring_source_eligible: row.get::<i64, _>("monitoring_source_eligible")
                        != 0,
                },
            );
        }
        return Ok(contexts);
    }
    let sql = format!(
        "SELECT 'station_key:' || k.id AS scope, r.revision AS lifecycle_revision, CASE WHEN k.enabled = 1 AND s.enabled = 1 AND (TRIM(k.api_key) <> '' OR k.api_key_secret_id IS NOT NULL) AND r.revision IS NOT NULL THEN 1 ELSE 0 END AS real_source_eligible, CASE WHEN r.revision IS NOT NULL AND EXISTS (SELECT 1 FROM channel_monitors m WHERE m.enabled = 1 AND m.station_id = k.station_id AND (m.station_key_id = k.id OR m.station_key_id IS NULL) AND m.client_profile_id = 'standard_api') THEN 1 ELSE 0 END AS monitoring_source_eligible FROM station_keys k JOIN stations s ON s.id = k.station_id LEFT JOIN domain_revisions r ON r.scope = 'station_key:' || k.id WHERE ('station_key:' || k.id) IN ({placeholders}) ORDER BY k.id ASC"
    );
    let mut query = sqlx::query(&sql);
    for scope in &key_scopes {
        query = query.bind(scope);
    }
    let rows = query.fetch_all(&mut *connection).await?;
    let mut contexts = std::collections::BTreeMap::new();
    for row in rows {
        let scope = row.get::<String, _>("scope");
        let revision = row
            .try_get::<Option<i64>, _>("lifecycle_revision")?
            .map(|value| {
                u64::try_from(value).map_err(|_| {
                    crate::persistence::error::PersistenceError::InvariantViolation(
                        "station key lifecycle revision is negative".into(),
                    )
                })
            })
            .transpose()?;
        contexts.insert(
            scope,
            QualityKeyContext {
                lifecycle_revision: revision.or(Some(0)),
                real_source_eligible: row.get::<i64, _>("real_source_eligible") != 0
                    && revision.is_some(),
                monitoring_source_eligible: row.get::<i64, _>("monitoring_source_eligible") != 0
                    && revision.is_some(),
            },
        );
    }
    Ok(contexts)
}

fn observation_scope(observation: &RoutingObservation) -> String {
    observation
        .scope
        .station_key_id
        .as_deref()
        .map(|id| format!("station_key:{id}"))
        .or_else(|| {
            observation
                .scope
                .station_id
                .as_deref()
                .map(|id| format!("station:{id}"))
        })
        .or_else(|| {
            observation
                .scope
                .model
                .as_deref()
                .map(|model| format!("model:{model}"))
        })
        .unwrap_or_else(|| "global".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{
            routing_generation::{canonical_json_sha256, ROUTING_GENERATION_ALGORITHM_VERSION},
            routing_generation_coordinator::RoutingGenerationCoordinator,
        },
        background_tasks::routing_generation_cutover_runner::build_ready_once,
        models::{
            routing_generation::RoutingGenerationQualification,
            routing_policy::RoutingPolicyConfigV3,
        },
        persistence::{
            runtime::{PersistenceHandle, PersistenceRuntime},
            stores::routing_observation_store::RoutingObservationAppend,
        },
    };

    #[tokio::test]
    async fn runner_honors_cancellation_and_batch_bound() {
        let token = CancellationToken::new();
        token.cancel();
        let scopes = (0..(MAX_ROUTING_PROJECTION_BATCH + 1))
            .map(|index| format!("station_key:key-{index}"))
            .collect::<Vec<_>>();
        let result =
            RoutingProjectionRunner.project_batch(&scopes, &[], BetaPrior::default(), &token);
        assert!(result.cancelled);
        assert_eq!(result.processed, 0);
    }

    #[test]
    fn ingestion_cursor_is_private_to_the_projector() {
        assert!(ROUTING_QUALITY_CURSOR_SCOPE.starts_with("__routing_projection_"));
        assert_ne!(ROUTING_QUALITY_CURSOR_SCOPE, "global");
    }

    #[tokio::test]
    async fn retention_deletes_only_expired_rows_and_preserves_redacted_rollups() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("retention.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        let now_ms = RAW_EVENT_RETENTION_WINDOW_MS + 10_000;
        let expired_at_ms = 1;
        let fresh_at_ms = now_ms - RAW_EVENT_RETENTION_WINDOW_MS + 1;

        let mut write = handle.begin_write().await.expect("begin retention fixture");
        append_retention_observation(write.connection(), "expired-observation", 1, expired_at_ms)
            .await;
        append_retention_observation(write.connection(), "fresh-observation", 2, fresh_at_ms).await;
        insert_retention_circuit_event(write.connection(), "expired-circuit", 1, expired_at_ms)
            .await;
        insert_retention_circuit_event(write.connection(), "fresh-circuit", 2, fresh_at_ms).await;
        write.commit().await.expect("commit retention fixture");

        let report = retain_raw_events_once(&handle, &CancellationToken::new(), now_ms)
            .await
            .expect("run retention");
        assert_eq!(report.observations_deleted, 1);
        assert_eq!(report.circuit_events_deleted, 1);

        let mut read = handle.begin_read().await.expect("read retention result");
        let observation_ids =
            sqlx::query_scalar::<_, String>("SELECT id FROM routing_observations ORDER BY id ASC")
                .fetch_all(read.connection())
                .await
                .expect("remaining observations");
        assert_eq!(observation_ids, vec!["fresh-observation"]);
        let circuit_ids = sqlx::query_scalar::<_, String>(
            "SELECT event_id FROM routing_circuit_event_v3 ORDER BY event_id ASC",
        )
        .fetch_all(read.connection())
        .await
        .expect("remaining circuit events");
        assert_eq!(circuit_ids, vec!["fresh-circuit"]);
        let rollup_count: i64 =
            sqlx::query_scalar("SELECT SUM(deleted_count) FROM routing_raw_event_retention_rollup")
                .fetch_one(read.connection())
                .await
                .expect("retention rollup");
        assert_eq!(rollup_count, 2);
        let successful_runs: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_raw_event_retention_run
             WHERE status = 'succeeded' AND observations_deleted = 1
               AND circuit_events_deleted = 1",
        )
        .fetch_one(read.connection())
        .await
        .expect("retention audit");
        assert_eq!(successful_runs, 1);
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn retention_respects_building_generation_checkpoints() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime =
            PersistenceRuntime::initialize_new(&root.path().join("retention-checkpoint.sqlite3"))
                .await
                .expect("initialize runtime");
        let handle = runtime.handle();
        let now_ms = RAW_EVENT_RETENTION_WINDOW_MS + 10_000;
        let mut write = handle
            .begin_write()
            .await
            .expect("begin checkpoint fixture");
        append_retention_observation(write.connection(), "observation-before", 1, 1).await;
        append_retention_observation(write.connection(), "observation-after", 2, 2).await;
        let observation_safe_sequence: i64 = sqlx::query_scalar(
            "SELECT ingestion_sequence FROM routing_observations
             WHERE id = 'observation-before'",
        )
        .fetch_one(write.connection())
        .await
        .expect("observation safe sequence");
        insert_retention_circuit_event(write.connection(), "circuit-before", 1, 1).await;
        insert_retention_circuit_event(write.connection(), "circuit-after", 2, 2).await;
        let circuit_safe_sequence: i64 = sqlx::query_scalar(
            "SELECT ingestion_sequence FROM routing_circuit_event_v3
             WHERE event_id = 'circuit-before'",
        )
        .fetch_one(write.connection())
        .await
        .expect("circuit safe sequence");
        insert_building_retention_generations(
            write.connection(),
            observation_safe_sequence,
            circuit_safe_sequence,
        )
        .await;
        write.commit().await.expect("commit checkpoint fixture");

        let report = retain_raw_events_once(&handle, &CancellationToken::new(), now_ms)
            .await
            .expect("run checkpoint retention");
        assert_eq!(
            report.observation_safe_sequence,
            observation_safe_sequence as u64
        );
        assert_eq!(report.circuit_safe_sequence, circuit_safe_sequence as u64);
        assert_eq!(report.observations_deleted, 1);
        assert_eq!(report.circuit_events_deleted, 1);

        let mut read = handle.begin_read().await.expect("read checkpoint result");
        let remaining_observation: String =
            sqlx::query_scalar("SELECT id FROM routing_observations")
                .fetch_one(read.connection())
                .await
                .expect("protected observation");
        assert_eq!(remaining_observation, "observation-after");
        let remaining_circuit: String =
            sqlx::query_scalar("SELECT event_id FROM routing_circuit_event_v3")
                .fetch_one(read.connection())
                .await
                .expect("protected circuit event");
        assert_eq!(remaining_circuit, "circuit-after");
        drop(read);
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn incremental_projection_uses_staged_quality_policy_only_after_activation() {
        let root = tempfile::tempdir().expect("tempdir");
        let runtime = PersistenceRuntime::initialize_new(&root.path().join("projection.sqlite3"))
            .await
            .expect("initialize runtime");
        let handle = runtime.handle();
        crate::persistence::stores::routing_policy_v3_stage_upgrade::stage_all(&handle, 1)
            .await
            .expect("stage baseline policy");

        let baseline_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build baseline generation")
            .expect("baseline generation");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        let baseline_created_at_ms = generation_created_at_ms(&handle, &baseline_id).await;
        qualify(&coordinator, &baseline_id, baseline_created_at_ms + 1).await;
        let baseline_fence = coordinator
            .begin_cutover(&baseline_id, None, baseline_created_at_ms + 2)
            .await
            .expect("fence baseline generation");
        coordinator
            .complete_cutover(&baseline_fence, baseline_created_at_ms + 3)
            .await
            .expect("activate baseline generation");

        let mut target = RoutingPolicyConfigV3::default();
        target.reliability_source_weights.real_traffic_percent = 82;
        target.reliability_source_weights.monitoring_percent = 18;
        let staged =
            crate::persistence::stores::routing_policy_v3_stage_upgrade::stage_user_policy(
                &handle,
                1,
                &target,
                "user",
                chrono::Utc::now()
                    .timestamp_millis()
                    .max(baseline_created_at_ms + 4),
            )
            .await
            .expect("stage target policy");
        assert_eq!(staged.revision, 2);

        let mut read = handle.begin_read().await.expect("read active config");
        let before_activation = load_quality_config(read.connection())
            .await
            .expect("load active quality config before activation");
        drop(read);
        assert_eq!(before_activation.quality_policy_revision, 1);
        assert_eq!(before_activation.real_traffic_weight_basis_points, 7_000);
        assert_eq!(before_activation.monitoring_weight_basis_points, 3_000);

        let target_id = build_ready_once(&handle, &CancellationToken::new())
            .await
            .expect("build target generation")
            .expect("target generation");
        let target_created_at_ms = generation_created_at_ms(&handle, &target_id).await;
        qualify(&coordinator, &target_id, target_created_at_ms + 1).await;
        let target_fence = coordinator
            .begin_cutover(&target_id, Some(&baseline_id), target_created_at_ms + 2)
            .await
            .expect("fence target generation");
        coordinator
            .complete_cutover(&target_fence, target_created_at_ms + 3)
            .await
            .expect("activate target generation");

        let mut read = handle.begin_read().await.expect("read activated config");
        let after_activation = load_quality_config(read.connection())
            .await
            .expect("load active quality config after activation");
        drop(read);
        assert_eq!(after_activation.quality_policy_revision, 2);
        assert_eq!(after_activation.real_traffic_weight_basis_points, 8_200);
        assert_eq!(after_activation.monitoring_weight_basis_points, 1_800);

        runtime.close().await.expect("close runtime");
    }

    async fn qualify(
        coordinator: &RoutingGenerationCoordinator,
        runtime_generation_id: &str,
        qualified_at_ms: i64,
    ) {
        let (comparison_report, replay_report) =
            crate::models::routing_generation::test_activation_qualification_reports(
                runtime_generation_id,
            );
        coordinator
            .record_qualification(&RoutingGenerationQualification {
                runtime_generation_id: runtime_generation_id.to_string(),
                comparison_report_hash: canonical_json_sha256(&comparison_report)
                    .expect("comparison hash"),
                comparison_report,
                replay_report_hash: canonical_json_sha256(&replay_report).expect("replay hash"),
                replay_report,
                qualified_at_ms,
            })
            .await
            .expect("qualify generation");
    }

    async fn generation_created_at_ms(
        handle: &PersistenceHandle,
        runtime_generation_id: &str,
    ) -> i64 {
        let mut read = handle.begin_read().await.expect("read generation time");
        sqlx::query_scalar(
            "SELECT created_at_ms FROM routing_runtime_generation
             WHERE runtime_generation_id = ?1",
        )
        .bind(runtime_generation_id)
        .fetch_one(read.connection())
        .await
        .expect("generation creation time")
    }

    async fn append_retention_observation(
        connection: &mut sqlx::SqliteConnection,
        id: &str,
        producer_sequence: u64,
        at_ms: i64,
    ) {
        RoutingObservationStore
            .append_with_generation_eligibility(
                connection,
                &RoutingObservationAppend {
                    id: id.to_string(),
                    producer_id: "retention-test".to_string(),
                    producer_sequence,
                    payload_hash: format!("{:064x}", producer_sequence),
                    event_at_ms: at_ms,
                    ingested_at_ms: at_ms,
                    scope: "station_key:retention-key".to_string(),
                    source: "real_request".to_string(),
                    traffic_equivalence: "exact_request".to_string(),
                    outcome_kind: "success".to_string(),
                    latency_ms: Some(100),
                    mass_basis_points: Some(10_000),
                    comparability_key: None,
                    evidence: serde_json::json!({}),
                    correlation_id: format!("retention-correlation-{producer_sequence}"),
                    attempt_index: 0,
                    station_key_lifecycle_revision: 1,
                    cluster_finalized: true,
                    cluster_expected_attempt_count: 1,
                    boundary_crossed: true,
                    event_time_status: crate::models::routing_observation::EventTimeStatus::Valid,
                    response_origin: "upstream".to_string(),
                    failure_code: None,
                    failure_attribution: "key".to_string(),
                    recovery_origin: "normal".to_string(),
                    retry_disposition: "end".to_string(),
                },
                Some("active"),
                at_ms,
            )
            .await
            .expect("append retention observation");
    }

    async fn insert_retention_circuit_event(
        connection: &mut sqlx::SqliteConnection,
        event_id: &str,
        reducer_sequence: i64,
        at_ms: i64,
    ) {
        sqlx::query(
            "INSERT INTO routing_circuit_event_v3 (
                 event_id, effect_kind, source, attempt_id, station_key_id,
                 station_key_lifecycle_revision, reducer_commit_sequence,
                 policy_revision, expected_state_revision, occurred_at_ms,
                 canonical_outcome, failure_code, recovery_origin,
                 retry_disposition, lease_revision, boundary_crossed, created_at_ms)
             VALUES (?1, 'circuit', 'real_request', ?2, 'retention-key', 1, ?3,
                     1, 1, ?4, 'success', NULL, 'normal', 'end', NULL, 1, ?4)",
        )
        .bind(event_id)
        .bind(format!("attempt-{event_id}"))
        .bind(reducer_sequence)
        .bind(at_ms)
        .execute(&mut *connection)
        .await
        .expect("insert retention circuit event");
    }

    async fn insert_building_retention_generations(
        connection: &mut sqlx::SqliteConnection,
        observation_safe_sequence: i64,
        circuit_safe_sequence: i64,
    ) {
        sqlx::query(
            "INSERT INTO routing_quality_generation_v3 (
                 quality_generation_id, scope, quality_policy_revision,
                 quality_algorithm_version, status, processed_observation_count,
                 created_at_ms, updated_at_ms)
             VALUES ('qg-retention', 'station_key', 1, 'routing_quality_v3',
                     'building', 0, 1, 1)",
        )
        .execute(&mut *connection)
        .await
        .expect("insert building quality generation");
        sqlx::query(
            "INSERT INTO routing_quality_generation_v3_checkpoint (
                 quality_generation_id, input_observation_watermark,
                 processed_observation_count, status, updated_at_ms)
             VALUES ('qg-retention', ?1, 0, 'building', 1)",
        )
        .bind(observation_safe_sequence)
        .execute(&mut *connection)
        .await
        .expect("insert quality checkpoint");
        sqlx::query(
            "INSERT INTO routing_circuit_generation_v3 (
                 circuit_generation_id, scope, circuit_policy_revision,
                 circuit_algorithm_version, status, processed_event_count,
                 created_at_ms, updated_at_ms)
             VALUES ('cg-retention', 'station_key', 1, 'routing_circuit_v3',
                     'building', 0, 1, 1)",
        )
        .execute(&mut *connection)
        .await
        .expect("insert building circuit generation");
        sqlx::query(
            "INSERT INTO routing_circuit_generation_v3_checkpoint (
                 circuit_generation_id, input_circuit_event_watermark,
                 processed_event_count, status, updated_at_ms)
             VALUES ('cg-retention', ?1, 0, 'building', 1)",
        )
        .bind(circuit_safe_sequence)
        .execute(&mut *connection)
        .await
        .expect("insert circuit checkpoint");
    }
}
