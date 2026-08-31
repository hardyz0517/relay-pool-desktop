use std::time::Duration;

use tokio_util::sync::CancellationToken;

use crate::{
    application::{request_finalization::RequestFinalizationService, routing::RoutingService},
    background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor},
    persistence::stores::routing_policy_store::RoutingPolicyStore,
    persistence::{
        error::PersistenceError, runtime::PersistenceHandle,
        stores::station_key_circuit_store::StationKeyCircuitStore,
    },
};

pub(crate) const STATION_KEY_CIRCUIT_REAPER_TASK_ID: &str = "station-key-circuit-reaper-v1";
const REAPER_INTERVAL: Duration = Duration::from_secs(5);

/// Registers the single owner responsible for recovering durable Half-Open
/// leases after deadline or process failure. The reaper is deliberately
/// independent from request execution so a dead request cannot strand a Key.
pub(crate) fn register_station_key_circuit_reaper_task(
    supervisor: &TaskSupervisor,
    runtime: PersistenceHandle,
    routing: std::sync::Arc<RoutingService>,
    request_finalization: std::sync::Arc<RequestFinalizationService>,
) -> Result<TaskId, String> {
    let task_id = TaskId::from(STATION_KEY_CIRCUIT_REAPER_TASK_ID);
    supervisor
        .register(
            TaskSpec::new(
                task_id.clone(),
                "station_key_circuit_reaper_v1",
                move |context: TaskRunContext| {
                    let runtime = runtime.clone();
                    let routing = std::sync::Arc::clone(&routing);
                    let request_finalization = std::sync::Arc::clone(&request_finalization);
                    Box::pin(async move {
                        run_reaper_loop(
                            runtime,
                            routing,
                            request_finalization,
                            context.cancellation_token,
                        )
                        .await
                    })
                },
            )
            .with_concurrency_key("station-key-circuit-reaper-v1")
            .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

async fn run_reaper_loop(
    runtime: PersistenceHandle,
    routing: std::sync::Arc<RoutingService>,
    request_finalization: std::sync::Arc<RequestFinalizationService>,
    cancellation: CancellationToken,
) -> Result<(), TaskFailure> {
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return Err(TaskFailure::cancelled()),
            _ = tokio::time::sleep(REAPER_INTERVAL) => {}
        }
        if let Err(error) = reap_once(&runtime, &routing, &request_finalization).await {
            // A transient database failure must not stop future recovery. The
            // task remains alive and retries on the next fixed interval.
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::routing_projection_tick_failed(),
            );
            let _ = error;
        }
    }
}

async fn reap_once(
    runtime: &PersistenceHandle,
    routing: &RoutingService,
    request_finalization: &RequestFinalizationService,
) -> Result<u32, PersistenceError> {
    let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
    request_finalization
        .replay_circuit_persistence_backlog()
        .await
        .map_err(|_| PersistenceError::RuntimeUnavailable)?;
    routing
        .health_check_station_key_circuit_persistence(now_ms)
        .await
        .map_err(|_| PersistenceError::RuntimeUnavailable)?;
    let mut write = runtime.begin_write().await?;
    let policy = RoutingPolicyStore
        .load_circuit_policy_parameters(write.connection())
        .await?;
    let reaped = StationKeyCircuitStore
        .reap_expired_leases(
            write.connection(),
            now_ms,
            policy.policy_revision,
            policy.consecutive_failure_threshold,
            policy.recovery_success_threshold,
            policy.recovery_wait_ms,
        )
        .await?;
    write.commit().await?;
    Ok(reaped)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaper_interval_is_bounded_and_stable() {
        assert_eq!(REAPER_INTERVAL, Duration::from_secs(5));
    }
}
