use std::sync::Arc;
use std::time::Duration;

pub(crate) use super::maintenance_policy::*;
use crate::{
    application::monitoring::MonitoringService,
    background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor},
};

pub(crate) const MONITORING_MAINTENANCE_TASK_ID: &str = "monitoring-maintenance-v2";

pub(crate) fn register_monitoring_maintenance_task(
    supervisor: &TaskSupervisor,
    monitoring: Arc<MonitoringService>,
    config: MonitoringMaintenanceConfig,
    installation_hash: u64,
) -> Result<TaskId, String> {
    config.validate().map_err(str::to_string)?;
    let task_id = TaskId::from(MONITORING_MAINTENANCE_TASK_ID);
    let state = MonitoringMaintenanceState::default();
    supervisor
        .register(
        TaskSpec::new(task_id.clone(), "monitoring_maintenance_v2", move |context: TaskRunContext| {
            let monitoring = Arc::clone(&monitoring);
            let state = state.clone();
            let config = config.clone();
            Box::pin(async move {
                tokio::select! {
                    _ = context.cancellation_token.cancelled() => {
                        crate::observability::runtime::bootstrap::emit(
                            crate::services::monitoring::runtime_events::maintenance_cancelled(),
                        );
                        return Err(TaskFailure::cancelled());
                    },
                    _ = tokio::time::sleep(config.deterministic_startup_delay(installation_hash)) => {}
                }
                loop {
                    if context.cancellation_token.is_cancelled() {
                        return Err(TaskFailure::cancelled());
                    }
                    if let Some(guard) = state.try_begin_cycle() {
                        let cycle = run_maintenance_cycle(
                            monitoring.as_ref(),
                            &config,
                            &context.cancellation_token,
                            &guard,
                        );
                        tokio::select! {
                            _ = context.cancellation_token.cancelled() => return Err(TaskFailure::cancelled()),
                            result = tokio::time::timeout(
                                Duration::from_millis(config.time_budget_ms),
                                cycle,
                            ) => {
                                match result {
                                    Ok(Ok(())) => {}
                                    Ok(Err(_)) => crate::observability::runtime::bootstrap::emit(
                                        crate::services::monitoring::runtime_events::maintenance_failed(),
                                    ),
                                    Err(_) => crate::observability::runtime::bootstrap::emit(
                                        crate::services::monitoring::runtime_events::maintenance_timeout(),
                                    ),
                                }
                            }
                        }
                    }
                    tokio::select! {
                        _ = context.cancellation_token.cancelled() => {
                            crate::observability::runtime::bootstrap::emit(
                                crate::services::monitoring::runtime_events::maintenance_cancelled(),
                            );
                            return Err(TaskFailure::cancelled());
                        },
                        _ = tokio::time::sleep(Duration::from_millis(config.interval_ms)) => {}
                    }
                }
            })
        })
        .with_concurrency_key("monitoring-maintenance-v2")
        .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

async fn run_maintenance_cycle(
    monitoring: &MonitoringService,
    config: &MonitoringMaintenanceConfig,
    cancellation: &tokio_util::sync::CancellationToken,
    guard: &MonitoringMaintenanceCycleGuard,
) -> Result<(), String> {
    let mut processed_rows = monitoring
        .mark_corrupt_monitoring_rollups()
        .await
        .map_err(|error| error.to_string())?
        .min(config.row_budget);
    if !guard.should_continue(cancellation, processed_rows, config) {
        return Ok(());
    }

    let remaining = config.row_budget.saturating_sub(processed_rows);
    let repaired = monitoring
        .repair_pending_monitoring_rollups(remaining)
        .await
        .map_err(|error| error.to_string())?;
    processed_rows = processed_rows
        .saturating_add(repaired)
        .min(config.row_budget);
    if !guard.should_continue(cancellation, processed_rows, config) {
        return Ok(());
    }

    let remaining = config.row_budget.saturating_sub(processed_rows);
    let now_ms = chrono::Utc::now().timestamp_millis();
    let cutoff_ms = now_ms.saturating_sub(30_i64 * 24 * 60 * 60 * 1_000);
    monitoring
        .delete_rolled_up_monitoring_executions(cutoff_ms, remaining, remaining)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}
