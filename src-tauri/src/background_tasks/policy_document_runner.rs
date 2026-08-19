//! Shared managed-policy reconciliation task.
//!
//! Native filesystem events are an optimisation and are intentionally not the
//! correctness boundary. This task provides the bounded digest reconciliation
//! fallback for both document kinds after startup, resume, or a missed event.

use std::{path::Path, time::Duration};

use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;

use crate::background_tasks::{
    RestartPolicy, TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor,
};
use crate::persistence::runtime::PersistenceHandle;

pub const POLICY_DOCUMENT_TASK_ID: &str = "policy-document-reconciliation-v1";
const RECONCILIATION_INTERVAL: Duration = Duration::from_secs(30);

pub fn register_policy_document_task(
    supervisor: &TaskSupervisor,
    runtime: PersistenceHandle,
) -> Result<TaskId, String> {
    let task_id = TaskId::from(POLICY_DOCUMENT_TASK_ID);
    supervisor
        .register(
            TaskSpec::new(
                task_id.clone(),
                "policy_document_reconciliation_v1",
                move |context: TaskRunContext| {
                    let runtime = runtime.clone();
                    Box::pin(async move {
                        run_reconciliation_loop(&runtime, &context.cancellation_token).await
                    })
                },
            )
            .with_concurrency_key(POLICY_DOCUMENT_TASK_ID)
            .with_restart_policy(RestartPolicy::transient(
                8,
                Duration::from_secs(1),
                Duration::from_secs(60),
            ))
            .with_shutdown_timeout(Duration::from_secs(10)),
        )
        .map_err(|error| error.to_string())?;
    Ok(task_id)
}

async fn run_reconciliation_loop(
    runtime: &PersistenceHandle,
    cancellation: &tokio_util::sync::CancellationToken,
) -> Result<(), TaskFailure> {
    let config_dir = runtime
        .database_path()
        .parent()
        .map(|root| root.join("config"));
    let (event_tx, mut event_rx) = mpsc::channel::<Result<Event, notify::Error>>(64);
    let mut watcher = config_dir
        .as_deref()
        .and_then(|directory| start_watcher(directory, event_tx.clone()));
    let mut ticker = tokio::time::interval(RECONCILIATION_INTERVAL);
    loop {
        tokio::select! {
            _ = cancellation.cancelled() => return Err(TaskFailure::cancelled()),
            _ = ticker.tick() => {
                // A failed initial watch or a backend restart must not disable
                // low-latency handling permanently. The digest reconciliation
                // below remains authoritative even while re-registration is
                // unavailable.
                if watcher.is_none() {
                    if let Some(directory) = config_dir.as_deref() {
                        watcher = start_watcher(directory, event_tx.clone());
                    }
                }
                reconcile_once(runtime).await.map_err(|error| TaskFailure::transient(error.to_string()))?;
            }
            event = event_rx.recv() => {
                match event {
                    None => return Err(TaskFailure::transient("policy document watcher stopped")),
                    Some(Err(_watcher_error)) => {
                        // notify reports queue overflow and backend failures on
                        // the callback channel. Treat those events as a lost
                        // wakeup, reconcile immediately, and rebuild the native
                        // watcher so later edits regain low-latency handling.
                        watcher = restart_watcher(config_dir.as_deref(), event_tx.clone());
                        reconcile_once(runtime).await.map_err(|error| TaskFailure::transient(error.to_string()))?;
                    }
                    Some(Ok(_event)) => {
                        // Coalesce bursty rename/modify events and wait for an
                        // atomic writer to finish before reading the file twice
                        // for stability.
                        tokio::time::sleep(Duration::from_millis(750)).await;
                        while event_rx.try_recv().is_ok() {}
                        reconcile_once(runtime).await.map_err(|error| TaskFailure::transient(error.to_string()))?;
                    }
                }
            }
        }
    }
}

fn restart_watcher(
    config_dir: Option<&Path>,
    event_tx: mpsc::Sender<Result<Event, notify::Error>>,
) -> Option<RecommendedWatcher> {
    config_dir.and_then(|directory| start_watcher(directory, event_tx))
}

fn start_watcher(
    config_dir: &Path,
    event_tx: mpsc::Sender<Result<Event, notify::Error>>,
) -> Option<RecommendedWatcher> {
    if std::fs::create_dir_all(config_dir).is_err() {
        return None;
    }
    let callback_tx = event_tx;
    let mut watcher = RecommendedWatcher::new(
        move |event| {
            let _ = callback_tx.blocking_send(event);
        },
        Config::default(),
    )
    .ok()?;
    if watcher
        .watch(config_dir, RecursiveMode::NonRecursive)
        .is_err()
    {
        return None;
    }
    Some(watcher)
}

async fn reconcile_once(
    runtime: &PersistenceHandle,
) -> Result<(), crate::persistence::error::PersistenceError> {
    crate::application::model_mapping::reconcile_external_model_mapping_document(runtime.clone())
        .await?;
    crate::application::routing::reconcile_external_routing_policy_document(runtime.clone())
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_id_is_stable_and_interval_is_bounded() {
        assert_eq!(POLICY_DOCUMENT_TASK_ID, "policy-document-reconciliation-v1");
        assert!(RECONCILIATION_INTERVAL <= Duration::from_secs(30));
    }

    #[test]
    fn watcher_start_is_bounded_and_does_not_panic_on_invalid_directory() {
        let (sender, _receiver) = mpsc::channel(1);
        let temp = tempfile::tempdir().expect("tempdir");
        let invalid = temp.path().join("not-a-directory");
        std::fs::write(&invalid, b"fixture").expect("fixture file");
        assert!(start_watcher(&invalid, sender).is_none());
    }

    #[test]
    fn watcher_restart_rebuilds_valid_directory_and_stays_fail_closed_for_invalid_one() {
        let (sender, _receiver) = mpsc::channel(1);
        let temp = tempfile::tempdir().expect("tempdir");
        let valid = temp.path().join("config");
        assert!(restart_watcher(Some(&valid), sender.clone()).is_some());

        let invalid = temp.path().join("not-a-directory");
        std::fs::write(&invalid, b"fixture").expect("fixture file");
        assert!(restart_watcher(Some(&invalid), sender).is_none());
        assert!(restart_watcher(None, mpsc::channel(1).0).is_none());
    }
}
