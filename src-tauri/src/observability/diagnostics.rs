#![allow(
    dead_code,
    reason = "Task 18B freezes the local runtime diagnostics read model before command/UI access is wired"
)]

use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use crate::{
    background_tasks::{
        BlockingExecutor, BlockingJobMetrics, OperationRegistry, OperationRegistryMetrics,
        TaskRunId, TaskState, TaskStatusSnapshot, TaskSupervisor,
    },
    observability::{
        metrics::{
            LocalMetricBuffer, MetricError, MetricEvent, MetricKind, MetricLabel, MetricOutcome,
            MetricSnapshot, RuntimeMetricLabel,
        },
        redaction::redact_text_preview_with_limit,
    },
    outbound::{AsyncOutboundClient, OutboundClientMetrics},
};

const DEFAULT_DIAGNOSTIC_METRIC_CAPACITY: usize = 512;
const MAX_TASK_FAILURE_CODE_BYTES: usize = 96;

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
                    .filter(|task| task.state == RuntimeTaskState::Registered)
                    .count() as u64,
            ),
            (
                RuntimeMetricLabel::TaskActive,
                tasks.iter().filter(|task| task.state.is_active()).count() as u64,
            ),
            (
                RuntimeMetricLabel::TaskTerminal,
                tasks.iter().filter(|task| task.state.is_terminal()).count() as u64,
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
pub(crate) struct RuntimeTaskSummary {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) run_id: Option<u64>,
    pub(crate) state: RuntimeTaskState,
    pub(crate) failure_code: Option<String>,
    pub(crate) consecutive_failures: u32,
    pub(crate) last_delay_ms: Option<u64>,
}

impl From<TaskStatusSnapshot> for RuntimeTaskSummary {
    fn from(snapshot: TaskStatusSnapshot) -> Self {
        let (state, failure_code) = runtime_task_state(snapshot.state);
        Self {
            id: snapshot.id.as_str().to_string(),
            kind: snapshot.kind,
            run_id: snapshot.run_id.map(TaskRunId::into_u64),
            state,
            failure_code,
            consecutive_failures: snapshot.consecutive_failures,
            last_delay_ms: snapshot.last_delay.map(duration_millis_u64),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeTaskState {
    Registered,
    Running,
    Stopping,
    BackingOff,
    Succeeded,
    Failed,
    Cancelled,
    Panicked,
}

impl RuntimeTaskState {
    fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Stopping)
    }

    fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Panicked
        )
    }
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

fn runtime_task_state(state: TaskState) -> (RuntimeTaskState, Option<String>) {
    match state {
        TaskState::Registered => (RuntimeTaskState::Registered, None),
        TaskState::Running => (RuntimeTaskState::Running, None),
        TaskState::Stopping => (RuntimeTaskState::Stopping, None),
        TaskState::BackingOff { .. } => (RuntimeTaskState::BackingOff, None),
        TaskState::Succeeded => (RuntimeTaskState::Succeeded, None),
        TaskState::Failed { code } => (
            RuntimeTaskState::Failed,
            Some(redact_text_preview_with_limit(
                &code,
                MAX_TASK_FAILURE_CODE_BYTES,
            )),
        ),
        TaskState::Cancelled => (RuntimeTaskState::Cancelled, None),
        TaskState::Panicked => (RuntimeTaskState::Panicked, None),
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

impl TaskRunId {
    fn into_u64(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::{
        background_tasks::{
            BlockingExecutor, BlockingExecutorConfig, OperationOwner, OperationRegistry,
            OperationRegistryConfig, OperationStartRequest, OperationTerminal, RestartPolicy,
            TaskFailure, TaskSpec, TaskState, TaskSupervisor,
        },
        observability::{
            diagnostics::{RuntimeDiagnostics, RuntimeTaskState},
            metrics::{MetricKind, MetricLabel, RuntimeMetricLabel},
        },
        outbound::{AsyncOutboundClient, AsyncOutboundClientConfig},
    };

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
        assert_eq!(snapshot.tasks[0].state, RuntimeTaskState::BackingOff);
        assert_eq!(snapshot.tasks[0].failure_code, None);
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
            });

        assert_eq!(summary.state, RuntimeTaskState::Failed);
        assert_eq!(summary.failure_code.as_deref(), Some("[REDACTED]"));
    }
}
