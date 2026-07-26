#![allow(
    dead_code,
    reason = "Task 18B freezes the local runtime diagnostics read model before command/UI access is wired"
)]

use std::sync::{Arc, Mutex};

use crate::{
    background_tasks::{
        BlockingExecutor, BlockingJobMetrics, OperationRegistry, OperationRegistryMetrics,
        RuntimeTaskStatus, RuntimeTaskSummary, TaskSupervisor,
    },
    observability::metrics::{
        LocalMetricBuffer, MetricError, MetricEvent, MetricKind, MetricLabel, MetricOutcome,
        MetricSnapshot, RuntimeMetricLabel,
    },
    outbound::{AsyncOutboundClient, OutboundClientMetrics},
};

const DEFAULT_DIAGNOSTIC_METRIC_CAPACITY: usize = 512;

#[derive(Clone)]
pub(crate) struct RuntimeDiagnostics {
    metrics: Arc<Mutex<LocalMetricBuffer>>,
}

impl RuntimeDiagnostics {
    pub(crate) fn new(capacity: usize) -> Result<Self, MetricError> {
        Ok(Self {
            metrics: Arc::new(Mutex::new(LocalMetricBuffer::new(capacity)?)),
        })
    }

    pub(crate) fn architecture_budget() -> Self {
        Self::new(DEFAULT_DIAGNOSTIC_METRIC_CAPACITY)
            .expect("diagnostic metric capacity is non-zero")
    }

    pub(crate) fn record(&self, event: MetricEvent) {
        self.metrics
            .lock()
            .expect("runtime diagnostics metrics mutex")
            .record(event);
    }

    pub(crate) fn snapshot_runtime(
        &self,
        supervisor: &TaskSupervisor,
        blocking: &BlockingExecutor,
        outbound: &AsyncOutboundClient,
        operation: &OperationRegistry,
    ) -> RuntimeDiagnosticsSnapshot {
        let tasks = supervisor
            .statuses()
            .into_iter()
            .map(RuntimeTaskSummary::from)
            .collect::<Vec<_>>();
        let blocking = blocking.metrics();
        let outbound = outbound.metrics();
        let operation = operation.metrics();
        self.record_runtime_metrics(&tasks, &blocking, &outbound, &operation);
        RuntimeDiagnosticsSnapshot {
            tasks,
            blocking: BlockingRuntimeSummary::from(blocking),
            outbound: OutboundRuntimeSummary::from(outbound),
            operations: OperationRuntimeSummary::from(operation),
            metrics: self.metric_snapshot(),
        }
    }

    pub(crate) fn metric_snapshot(&self) -> MetricSnapshot {
        self.metrics
            .lock()
            .expect("runtime diagnostics metrics mutex")
            .snapshot()
    }

    fn record_runtime_metrics(
        &self,
        tasks: &[RuntimeTaskSummary],
        blocking: &BlockingJobMetrics,
        outbound: &OutboundClientMetrics,
        operation: &OperationRegistryMetrics,
    ) {
        for (label, value) in [
            (RuntimeMetricLabel::BlockingQueued, blocking.queued as u64),
            (RuntimeMetricLabel::BlockingRunning, blocking.running as u64),
            (
                RuntimeMetricLabel::BlockingOrphaned,
                blocking.orphaned as u64,
            ),
            (
                RuntimeMetricLabel::OutboundPoolSize,
                outbound.pool_size as u64,
            ),
            (
                RuntimeMetricLabel::OutboundClientInstancesCreated,
                outbound.client_instances_created as u64,
            ),
            (
                RuntimeMetricLabel::OperationRunning,
                operation.running as u64,
            ),
            (RuntimeMetricLabel::OperationStored, operation.stored as u64),
            (
                RuntimeMetricLabel::OperationTerminal,
                operation.terminal as u64,
            ),
            (
                RuntimeMetricLabel::OperationExpiredTombstones,
                operation.expired_tombstones as u64,
            ),
            (
                RuntimeMetricLabel::TaskRegistered,
                tasks
                    .iter()
                    .filter(|task| task.status == RuntimeTaskStatus::Registered)
                    .count() as u64,
            ),
            (
                RuntimeMetricLabel::TaskActive,
                tasks.iter().filter(|task| task.status.is_active()).count() as u64,
            ),
            (
                RuntimeMetricLabel::TaskTerminal,
                tasks
                    .iter()
                    .filter(|task| task.status.is_terminal())
                    .count() as u64,
            ),
        ] {
            self.record(
                MetricEvent::new(
                    MetricKind::RuntimeStatus,
                    value,
                    vec![
                        MetricLabel::Runtime(label),
                        MetricLabel::Outcome(MetricOutcome::Ok),
                    ],
                )
                .expect("runtime status metric uses bounded labels"),
            );
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeDiagnosticsSnapshot {
    pub(crate) tasks: Vec<RuntimeTaskSummary>,
    pub(crate) blocking: BlockingRuntimeSummary,
    pub(crate) outbound: OutboundRuntimeSummary,
    pub(crate) operations: OperationRuntimeSummary,
    pub(crate) metrics: MetricSnapshot,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BlockingRuntimeSummary {
    pub(crate) queued: usize,
    pub(crate) running: usize,
    pub(crate) orphaned: usize,
}

impl From<BlockingJobMetrics> for BlockingRuntimeSummary {
    fn from(metrics: BlockingJobMetrics) -> Self {
        Self {
            queued: metrics.queued,
            running: metrics.running,
            orphaned: metrics.orphaned,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OutboundRuntimeSummary {
    pub(crate) pool_size: usize,
    pub(crate) client_instances_created: usize,
}

impl From<OutboundClientMetrics> for OutboundRuntimeSummary {
    fn from(metrics: OutboundClientMetrics) -> Self {
        Self {
            pool_size: metrics.pool_size,
            client_instances_created: metrics.client_instances_created,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OperationRuntimeSummary {
    pub(crate) running: usize,
    pub(crate) stored: usize,
    pub(crate) terminal: usize,
    pub(crate) expired_tombstones: usize,
}

impl From<OperationRegistryMetrics> for OperationRuntimeSummary {
    fn from(metrics: OperationRegistryMetrics) -> Self {
        Self {
            running: metrics.running,
            stored: metrics.stored,
            terminal: metrics.terminal,
            expired_tombstones: metrics.expired_tombstones,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{mpsc, Arc},
        time::Duration,
    };

    use crate::{
        background_tasks::{
            BlockingExecutor, BlockingExecutorConfig, BlockingExecutorError, OperationOwner,
            OperationRegistry, OperationRegistryConfig, OperationRegistryError,
            OperationStartRequest, OperationTerminal, RestartPolicy, RuntimeTaskStatus,
            TaskFailure, TaskSpec, TaskState, TaskSupervisor, TaskSupervisorError,
        },
        observability::{
            diagnostics::RuntimeDiagnostics,
            metrics::{MetricKind, MetricLabel, RuntimeMetricLabel},
        },
        outbound::{AsyncOutboundClient, AsyncOutboundClientConfig},
    };

    #[tokio::test]
    async fn mixed_runtime_saturation_is_predictable_and_visible() {
        let diagnostics = RuntimeDiagnostics::new(32).expect("diagnostics");
        let supervisor = TaskSupervisor::new();
        let task_release = Arc::new(tokio::sync::Notify::new());
        supervisor
            .register(
                TaskSpec::new("collector-a", "station-collector", {
                    let task_release = Arc::clone(&task_release);
                    move |_| {
                        let task_release = Arc::clone(&task_release);
                        Box::pin(async move {
                            task_release.notified().await;
                            Ok(())
                        })
                    }
                })
                .with_concurrency_key("collector"),
            )
            .expect("register first task");
        supervisor
            .register(
                TaskSpec::new("collector-b", "station-collector", |_| {
                    Box::pin(async { Ok(()) })
                })
                .with_concurrency_key("collector"),
            )
            .expect("register second task");
        supervisor.start(&"collector-a".into()).expect("start task");
        assert_eq!(
            supervisor
                .start(&"collector-b".into())
                .expect_err("task concurrency must reject predictably"),
            TaskSupervisorError::ConcurrencyKeyRunning("collector".to_string())
        );

        let operation_release = Arc::new(tokio::sync::Notify::new());
        let operation = OperationRegistry::new(OperationRegistryConfig {
            max_running_global: 1,
            max_running_per_concurrency_key: 1,
            progress_ring_entries_per_operation: 2,
            progress_entry_max_bytes: 64,
            terminal_ttl: Duration::from_secs(60),
            terminal_max_entries: 2,
            expired_tombstone_ttl: Duration::from_secs(60),
            default_deadline: Duration::from_secs(30),
        });
        let operation_id = operation
            .start(OperationStartRequest::new(
                "connectivity",
                OperationOwner::new("key-pool"),
                {
                    let operation_release = Arc::clone(&operation_release);
                    move |_| {
                        let operation_release = Arc::clone(&operation_release);
                        Box::pin(async move {
                            operation_release.notified().await;
                            OperationTerminal::Completed
                        })
                    }
                },
            ))
            .expect("first operation starts");
        assert_eq!(
            operation
                .start(OperationStartRequest::new(
                    "connectivity",
                    OperationOwner::new("key-pool"),
                    |_| Box::pin(async { OperationTerminal::Completed }),
                ))
                .expect_err("operation capacity must reject predictably"),
            OperationRegistryError::Overloaded
        );

        let blocking = BlockingExecutor::new(BlockingExecutorConfig {
            max_running: 1,
            queue_capacity: 1,
            queue_timeout: Duration::from_secs(30),
            default_execution_timeout: Duration::from_secs(30),
        });
        let (release_blocking_tx, release_blocking_rx) = mpsc::channel::<()>();
        let running = blocking
            .submit("filesystem", None, None, None, move |_| {
                release_blocking_rx.recv().expect("release blocking job");
                Ok("running")
            })
            .expect("first blocking job runs");
        wait_for_blocking_metrics(&blocking, 1, 0).await;
        let queued = blocking
            .submit("filesystem", None, None, None, |_| Ok("queued"))
            .expect("second blocking job queues");
        assert_eq!(
            blocking
                .submit("filesystem", None, None, None, |_| Ok("rejected"))
                .expect_err("blocking queue must reject predictably"),
            BlockingExecutorError::QueueFull
        );
        wait_for_blocking_metrics(&blocking, 1, 1).await;

        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let snapshot = diagnostics.snapshot_runtime(&supervisor, &blocking, &outbound, &operation);

        assert_eq!(snapshot.blocking.running, 1);
        assert_eq!(snapshot.blocking.queued, 1);
        assert_eq!(snapshot.operations.running, 1);
        assert_eq!(snapshot.tasks.len(), 2);
        assert_eq!(
            metric_value(&snapshot, RuntimeMetricLabel::BlockingRunning),
            Some(1)
        );
        assert_eq!(
            metric_value(&snapshot, RuntimeMetricLabel::BlockingQueued),
            Some(1)
        );
        assert_eq!(
            metric_value(&snapshot, RuntimeMetricLabel::OperationRunning),
            Some(1)
        );
        assert_eq!(
            metric_value(&snapshot, RuntimeMetricLabel::TaskActive),
            Some(1)
        );
        assert_eq!(
            metric_value(&snapshot, RuntimeMetricLabel::TaskRegistered),
            Some(1)
        );

        task_release.notify_waiters();
        operation_release.notify_waiters();
        release_blocking_tx.send(()).expect("release blocking");
        assert_eq!(running.result().await.unwrap(), "running");
        assert_eq!(queued.result().await.unwrap(), "queued");
        assert_eq!(
            supervisor
                .join_finished(&"collector-a".into())
                .await
                .unwrap(),
            TaskState::Succeeded
        );
        wait_for_operation_terminal(&operation, operation_id).await;
    }

    #[tokio::test]
    async fn runtime_diagnostics_are_local_bounded_and_actionable() {
        let diagnostics = RuntimeDiagnostics::new(8).expect("diagnostics");
        let supervisor = TaskSupervisor::new();
        supervisor
            .register(
                TaskSpec::new("collector", "station-collector", |_| {
                    Box::pin(async { Err(TaskFailure::transient("api_key=sk-secret")) })
                })
                .with_restart_policy(RestartPolicy::transient(
                    1,
                    Duration::from_millis(10),
                    Duration::from_millis(10),
                )),
            )
            .expect("register task");
        supervisor.start(&"collector".into()).expect("start task");
        assert_eq!(
            supervisor.join_finished(&"collector".into()).await.unwrap(),
            TaskState::BackingOff { retry_at_ms: 10 }
        );

        let blocking = BlockingExecutor::new(BlockingExecutorConfig {
            max_running: 1,
            queue_capacity: 1,
            queue_timeout: Duration::from_millis(10),
            default_execution_timeout: Duration::from_millis(10),
        });
        let outbound = AsyncOutboundClient::new(AsyncOutboundClientConfig::architecture_budget());
        let operation = OperationRegistry::new(OperationRegistryConfig {
            max_running_global: 1,
            max_running_per_concurrency_key: 1,
            progress_ring_entries_per_operation: 2,
            progress_entry_max_bytes: 64,
            terminal_ttl: Duration::from_secs(60),
            terminal_max_entries: 2,
            expired_tombstone_ttl: Duration::from_secs(60),
            default_deadline: Duration::from_secs(5),
        });
        let operation_id = operation
            .start(OperationStartRequest::new(
                "connectivity",
                OperationOwner::new("key-pool"),
                |_| Box::pin(async { OperationTerminal::Completed }),
            ))
            .expect("operation starts");
        for _ in 0..100 {
            if operation.status(operation_id).unwrap().terminal.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }

        let snapshot = diagnostics.snapshot_runtime(&supervisor, &blocking, &outbound, &operation);

        assert_eq!(snapshot.tasks.len(), 1);
        assert_eq!(snapshot.tasks[0].status, RuntimeTaskStatus::BackingOff);
        assert_eq!(
            snapshot.tasks[0].last_failure_code.as_deref(),
            Some("[REDACTED]")
        );
        assert_eq!(snapshot.blocking.running, 0);
        assert_eq!(snapshot.operations.terminal, 1);
        assert_eq!(snapshot.metrics.events.len(), 8);
        assert!(snapshot.metrics.dropped > 0);
        assert!(snapshot
            .metrics
            .events
            .iter()
            .all(|event| event.kind == MetricKind::RuntimeStatus));
        assert!(snapshot.metrics.events.iter().any(|event| {
            event
                .labels
                .contains(&MetricLabel::Runtime(RuntimeMetricLabel::OperationTerminal))
                && event.value == 1
        }));
    }

    #[test]
    fn failed_task_summary_uses_redacted_bounded_failure_code() {
        let summary =
            super::RuntimeTaskSummary::from(crate::background_tasks::TaskStatusSnapshot {
                id: "collector".into(),
                kind: "station-collector".to_string(),
                run_id: None,
                state: TaskState::Failed {
                    code: "authorization bearer sk-secret".to_string(),
                },
                consecutive_failures: 1,
                last_delay: None,
                last_started_at_ms: None,
                last_succeeded_at_ms: None,
                last_failure_code: None,
                next_retry_at_ms: None,
            });

        assert_eq!(summary.status, RuntimeTaskStatus::Failed);
        assert_eq!(summary.last_failure_code.as_deref(), Some("[REDACTED]"));
    }

    async fn wait_for_blocking_metrics(blocking: &BlockingExecutor, running: usize, queued: usize) {
        for _ in 0..100 {
            let metrics = blocking.metrics();
            if metrics.running == running && metrics.queued == queued {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("blocking executor did not reach running={running}, queued={queued}");
    }

    async fn wait_for_operation_terminal(
        operation: &OperationRegistry,
        id: crate::background_tasks::OperationId,
    ) {
        for _ in 0..100 {
            if operation.status(id).unwrap().terminal.is_some() {
                return;
            }
            tokio::task::yield_now().await;
        }
        panic!("operation did not reach terminal");
    }

    fn metric_value(
        snapshot: &super::RuntimeDiagnosticsSnapshot,
        label: RuntimeMetricLabel,
    ) -> Option<u64> {
        snapshot.metrics.events.iter().find_map(|event| {
            event
                .labels
                .contains(&MetricLabel::Runtime(label))
                .then_some(event.value)
        })
    }
}
