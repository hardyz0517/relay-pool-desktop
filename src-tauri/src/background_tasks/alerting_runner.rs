use std::sync::Arc;
use std::time::Duration;

use crate::application::alerting::{
    delivery_worker::DeliveryWorker, policy_service::PolicyService, reconcile::AlertingReconciler,
    retention_worker::AlertingRetentionWorker,
};
use crate::background_tasks::{TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor};
use crate::persistence::runtime::PersistenceHandle;
use crate::services::alerting::DesktopNotificationAdapter;

pub(crate) const ALERTING_RUNTIME_TASK_ID: &str = "alerting-runtime-v1";

/// Owns the durable alerting deadlines, delivery leases, policy reconcile and
/// retention cadence. The task is deliberately a single supervised loop so
/// that multiple runtime components cannot compete for the same delivery
/// claims or cleanup budget.
pub(crate) fn register_alerting_runtime_task(
    supervisor: &TaskSupervisor,
    runtime: PersistenceHandle,
    desktop_adapter: Arc<dyn DesktopNotificationAdapter>,
) -> Result<TaskId, String> {
    let task_id = TaskId::from(ALERTING_RUNTIME_TASK_ID);
    let worker_runtime = runtime.clone();
    let reconcile_runtime = runtime.clone();
    let retention_runtime = runtime;
    let desktop_adapter = desktop_adapter.clone();
    supervisor
        .register(
            TaskSpec::new(task_id.clone(), "alerting_runtime_v1", move |context: TaskRunContext| {
                let delivery = DeliveryWorker::new(worker_runtime.clone());
                let reconciler = AlertingReconciler::new(reconcile_runtime.clone());
                let retention = AlertingRetentionWorker::new(retention_runtime.clone());
                let policy_service = PolicyService::new(reconcile_runtime.clone());
                let desktop_adapter = desktop_adapter.clone();
                Box::pin(async move {
                    loop {
                        if context.cancellation_token.is_cancelled() {
                            return Err(TaskFailure::cancelled());
                        }
                        let now_ms = chrono::Utc::now().timestamp_millis();

                        let settings = policy_service
                            .load_settings()
                            .await
                            .map_err(|error| TaskFailure::transient(format!("settings:{error}")))?;
                        let policies = policy_service
                            .list_policies()
                            .await
                            .map_err(|error| TaskFailure::transient(format!("policies:{error}")))?;
                        let _ = reconciler
                            .reconcile_page(None, &policies, &settings, now_ms)
                            .await
                            .map_err(|error| {
                                TaskFailure::transient(format!("reconcile:{error}"))
                            })?;

                        let claims = delivery
                            .claim_due(now_ms, 50)
                            .await
                            .map_err(|error| TaskFailure::transient(format!("claim:{error}")))?;
                        for claim in claims {
                            let outcome = match claim.channel {
                                // The in-app channel is represented by the
                                // durable current/read model. Desktop delivery
                                // remains explicitly unavailable until the
                                // platform adapter is installed, so it is
                                // retried with the normal bounded ledger rules.
                                crate::models::alerting::NotificationChannel::InApp => {
                                    delivery.mark_delivered(&claim, now_ms).await
                                }
                                crate::models::alerting::NotificationChannel::Desktop => {
                                    match claim.desktop_payload() {
                                        Err(error) => {
                                            delivery.mark_failed(&claim, error.error_code(), now_ms).await
                                        }
                                        Ok(payload) => match desktop_adapter.send(&payload) {
                                            Ok(()) => delivery.mark_delivered(&claim, now_ms).await,
                                            Err(error) if error.is_retryable() => {
                                                delivery.mark_adapter_failure(&claim, now_ms).await
                                            }
                                            Err(error) => {
                                                delivery.mark_failed(&claim, error.error_code(), now_ms).await
                                            }
                                        },
                                    }
                                }
                            };
                            outcome.map_err(|error| {
                                TaskFailure::transient(format!("delivery:{error}"))
                            })?;
                        }
                        delivery
                            .recover_expired(now_ms, 50)
                            .await
                            .map_err(|error| TaskFailure::transient(format!("lease:{error}")))?;
                        retention
                            .run_once(
                                now_ms,
                                settings.history_retention_days,
                                settings.delivery_retention_days,
                                settings.delete_resolved_incidents,
                                100,
                            )
                            .await
                            .map_err(|error| TaskFailure::transient(format!("retention:{error}")))?;

                        tokio::select! {
                            _ = context.cancellation_token.cancelled() => return Err(TaskFailure::cancelled()),
                            _ = tokio::time::sleep(Duration::from_secs(15)) => {}
                        }
                    }
                })
            })
            .with_concurrency_key("alerting-runtime-v1")
            .with_restart_policy(crate::background_tasks::task::RestartPolicy::transient(
                5,
                Duration::from_secs(1),
                Duration::from_secs(30),
            ))
            .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_is_stable_and_singleton_scoped() {
        assert_eq!(ALERTING_RUNTIME_TASK_ID, "alerting-runtime-v1");
    }
}
