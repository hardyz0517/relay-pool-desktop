use std::time::Duration;
use tokio_util::sync::CancellationToken;

use crate::background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor};

use crate::{
    application::quality_projection::{
        rebuild_quality_summary_with_checkpoint, BetaPrior, QUALITY_PROJECTOR_VERSION,
    },
    models::routing_observation::RoutingObservation,
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            routing_observation_store::RoutingObservationStore,
            routing_quality_store::RoutingQualityStore,
        },
    },
};

#[cfg(test)]
use crate::application::quality_projection::QualitySummary;

pub(crate) const MAX_ROUTING_PROJECTION_BATCH: usize = 256;
pub const ROUTING_PROJECTION_TASK_ID: &str = "routing-projection-v1";
const ROUTING_PROJECTION_CURSOR_SCOPE: &str = "__routing_projection_ingestion_cursor__";

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
                    loop {
                        tokio::select! {
                            _ = context.cancellation_token.cancelled() => return Err(TaskFailure::cancelled()),
                            _ = tokio::time::sleep(Duration::from_millis(1_000)) => {}
                        }
                        if let Err(error) = project_once(&runtime, &context.cancellation_token).await {
                            tracing::warn!(task = ROUTING_PROJECTION_TASK_ID, error = %error, "routing projection tick failed");
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

async fn project_once(
    runtime: &PersistenceHandle,
    cancellation: &CancellationToken,
) -> Result<(), crate::persistence::error::PersistenceError> {
    if cancellation.is_cancelled() {
        return Ok(());
    }
    let observation_store = RoutingObservationStore;
    let quality_store = RoutingQualityStore;
    let (observations, scoped_histories) = {
        let mut read = runtime.begin_read().await?;
        let checkpoint = quality_store
            .load_checkpoint_cursor(
                read.connection(),
                ROUTING_PROJECTION_TASK_ID,
                QUALITY_PROJECTOR_VERSION,
                ROUTING_PROJECTION_CURSOR_SCOPE,
            )
            .await?
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
        let observations = observation_store
            .list_after(read.connection(), checkpoint, MAX_ROUTING_PROJECTION_BATCH)
            .await?;
        let mut scopes = observations
            .iter()
            .map(observation_scope)
            .collect::<Vec<_>>();
        scopes.sort();
        scopes.dedup();
        let mut scoped_histories = Vec::with_capacity(scopes.len());
        for scope in scopes {
            if cancellation.is_cancelled() {
                return Ok(());
            }
            scoped_histories.push((
                scope.clone(),
                observation_store.list_for_scope(read.connection(), &scope).await?,
            ));
        }
        (observations, scoped_histories)
    };
    if scoped_histories.is_empty() {
        return Ok(());
    }
    let ingestion_cursor = observations
        .last()
        .map(|observation| (observation.order.ingested_at_ms, observation.id.clone()))
        .ok_or_else(|| {
            crate::persistence::error::PersistenceError::InvariantViolation(
                "routing projection batch had scopes without observations".into(),
            )
        })?;
    let ingestion_checkpoint = u64::try_from(ingestion_cursor.0).map_err(|_| {
        crate::persistence::error::PersistenceError::InvariantViolation(
            "routing projection ingestion checkpoint is negative".into(),
        )
    })?;
    let now_ms = chrono::Utc::now().timestamp_millis().max(0);
    let mut write = runtime.begin_write().await?;
    for (scope, history) in scoped_histories {
        if cancellation.is_cancelled() {
            return Ok(());
        }
        let summary = rebuild_quality_summary_with_checkpoint(
            &scope,
            &history,
            BetaPrior::default(),
            ingestion_checkpoint,
        );
        let json = serde_json::to_value(&summary).map_err(|error| {
            crate::persistence::error::PersistenceError::InvariantViolation(error.to_string())
        })?;
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
                summary.latency_coverage_basis_points,
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
    // The cursor is advanced in the same transaction as every derived row. A
    // failed projection therefore retries the exact uncommitted ingestion range
    // instead of starving all observations after the first batch.
    quality_store
        .save_checkpoint(
            write.connection(),
            ROUTING_PROJECTION_TASK_ID,
            QUALITY_PROJECTOR_VERSION,
            ROUTING_PROJECTION_CURSOR_SCOPE,
            ingestion_checkpoint,
            "ready",
            Some(ingestion_cursor.1.as_str()),
            now_ms,
        )
        .await?;
    write.commit().await
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
        assert!(ROUTING_PROJECTION_CURSOR_SCOPE.starts_with("__routing_projection_"));
        assert_ne!(ROUTING_PROJECTION_CURSOR_SCOPE, "global");
    }
}
