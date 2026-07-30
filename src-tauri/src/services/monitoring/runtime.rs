use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
};

use crate::background_tasks::{
    TaskFailure, TaskId, TaskRunContext, TaskSpec, TaskSupervisor, TaskSupervisorError,
};

use super::scheduler::{MonitorTriggerKind, SchedulerCommand};

pub(crate) const MONITORING_RUNTIME_TASK_ID: &str = "monitoring-runtime-v2";
const MONITORING_RUNTIME_TASK_KIND: &str = "monitoring_runtime_v2";
const MONITORING_RUNTIME_CONCURRENCY_KEY: &str = "monitoring-runtime-v2";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonitoringRuntimeLimits {
    pub(crate) queue_capacity: usize,
    pub(crate) global_concurrency: usize,
    pub(crate) station_concurrency: usize,
    pub(crate) key_concurrency: usize,
}

impl MonitoringRuntimeLimits {
    pub(crate) fn normalized(self) -> Self {
        Self {
            queue_capacity: self.queue_capacity.max(1),
            global_concurrency: self.global_concurrency.max(1),
            station_concurrency: self.station_concurrency.max(1),
            key_concurrency: self.key_concurrency.max(1),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeAdmission {
    Queued {
        execution_id: String,
        queue_depth: usize,
    },
    Reused {
        execution_id: String,
    },
    QueueFull {
        lag_ms: i64,
    },
    ShuttingDown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeStart {
    Started {
        execution_id: String,
        command: SchedulerCommand,
    },
    QueueEmpty,
    PermitBlocked,
    ShuttingDown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeDiagnostics {
    pub(crate) admitting: bool,
    pub(crate) queue_depth: usize,
    pub(crate) active_count: usize,
    pub(crate) global_in_use: usize,
    pub(crate) max_queue_depth: usize,
    pub(crate) queue_full_count: usize,
    pub(crate) max_lag_ms: i64,
    pub(crate) terminal_interrupted_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ShutdownPlan {
    pub(crate) queued_cancelled: usize,
    pub(crate) running_to_interrupt: Vec<String>,
}

#[derive(Debug, Clone)]
struct QueuedExecution {
    execution_id: String,
    command: SchedulerCommand,
}

#[derive(Debug, Clone)]
struct ActiveExecution {
    command: SchedulerCommand,
}

#[derive(Debug)]
struct RuntimeState {
    limits: MonitoringRuntimeLimits,
    admitting: bool,
    next_execution_ordinal: u64,
    queue: VecDeque<QueuedExecution>,
    queued_by_monitor: HashMap<String, String>,
    queued_by_key: HashMap<String, String>,
    active: HashMap<String, ActiveExecution>,
    active_by_monitor: HashMap<String, String>,
    active_by_key: HashMap<String, String>,
    station_in_use: HashMap<String, usize>,
    key_in_use: HashMap<String, usize>,
    queue_full_count: usize,
    max_lag_ms: i64,
    terminal_interrupted_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct MonitoringRuntime {
    state: Arc<Mutex<RuntimeState>>,
}

impl MonitoringRuntime {
    pub(crate) fn new(limits: MonitoringRuntimeLimits) -> Self {
        Self {
            state: Arc::new(Mutex::new(RuntimeState {
                limits: limits.normalized(),
                admitting: true,
                next_execution_ordinal: 1,
                queue: VecDeque::new(),
                queued_by_monitor: HashMap::new(),
                queued_by_key: HashMap::new(),
                active: HashMap::new(),
                active_by_monitor: HashMap::new(),
                active_by_key: HashMap::new(),
                station_in_use: HashMap::new(),
                key_in_use: HashMap::new(),
                queue_full_count: 0,
                max_lag_ms: 0,
                terminal_interrupted_count: 0,
            })),
        }
    }

    pub(crate) fn admit(&self, command: SchedulerCommand) -> RuntimeAdmission {
        let mut state = self
            .state
            .lock()
            .expect("monitoring runtime mutex poisoned");
        if !state.admitting {
            return RuntimeAdmission::ShuttingDown;
        }
        if let Some(existing) = state
            .queued_by_monitor
            .get(&command.monitor_id)
            .or_else(|| state.active_by_monitor.get(&command.monitor_id))
        {
            return RuntimeAdmission::Reused {
                execution_id: existing.clone(),
            };
        }
        for key in &command.station_key_ids {
            if let Some(existing) = state
                .queued_by_key
                .get(key)
                .or_else(|| state.active_by_key.get(key))
            {
                return RuntimeAdmission::Reused {
                    execution_id: existing.clone(),
                };
            }
        }
        if state.queue.len() >= state.limits.queue_capacity {
            state.queue_full_count = state.queue_full_count.saturating_add(1);
            state.max_lag_ms = state.max_lag_ms.max(command.lag_ms);
            return RuntimeAdmission::QueueFull {
                lag_ms: command.lag_ms,
            };
        }

        let execution_id = next_execution_id(&mut state, &command);
        state
            .queued_by_monitor
            .insert(command.monitor_id.clone(), execution_id.clone());
        for key in &command.station_key_ids {
            state
                .queued_by_key
                .insert(key.clone(), execution_id.clone());
        }
        state.max_lag_ms = state.max_lag_ms.max(command.lag_ms);
        state.queue.push_back(QueuedExecution {
            execution_id: execution_id.clone(),
            command,
        });
        RuntimeAdmission::Queued {
            execution_id,
            queue_depth: state.queue.len(),
        }
    }

    pub(crate) fn start_next(&self) -> RuntimeStart {
        let mut state = self
            .state
            .lock()
            .expect("monitoring runtime mutex poisoned");
        if !state.admitting && state.queue.is_empty() {
            return RuntimeStart::ShuttingDown;
        }
        let Some(front) = state.queue.front().cloned() else {
            return RuntimeStart::QueueEmpty;
        };
        if !can_acquire(&state, &front.command) {
            return RuntimeStart::PermitBlocked;
        }
        let queued = state.queue.pop_front().expect("front queue item");
        state.queued_by_monitor.remove(&queued.command.monitor_id);
        for key in &queued.command.station_key_ids {
            state.queued_by_key.remove(key);
        }
        acquire(&mut state, &queued.command);
        state.active_by_monitor.insert(
            queued.command.monitor_id.clone(),
            queued.execution_id.clone(),
        );
        for key in &queued.command.station_key_ids {
            state
                .active_by_key
                .insert(key.clone(), queued.execution_id.clone());
        }
        state.active.insert(
            queued.execution_id.clone(),
            ActiveExecution {
                command: queued.command.clone(),
            },
        );
        RuntimeStart::Started {
            execution_id: queued.execution_id,
            command: queued.command,
        }
    }

    pub(crate) fn guard(&self, execution_id: &str) -> Option<ExecutionGuard> {
        let state = self
            .state
            .lock()
            .expect("monitoring runtime mutex poisoned");
        state
            .active
            .contains_key(execution_id)
            .then(|| ExecutionGuard {
                runtime: self.clone(),
                execution_id: Some(execution_id.to_string()),
            })
    }

    pub(crate) fn shutdown_begin(&self) -> ShutdownPlan {
        let mut state = self
            .state
            .lock()
            .expect("monitoring runtime mutex poisoned");
        state.admitting = false;
        let queued_cancelled = state.queue.len();
        state.queue.clear();
        state.queued_by_monitor.clear();
        state.queued_by_key.clear();
        let running_to_interrupt = state.active.keys().cloned().collect::<Vec<_>>();
        ShutdownPlan {
            queued_cancelled,
            running_to_interrupt,
        }
    }

    pub(crate) fn interrupt_running(&self) -> usize {
        let execution_ids = {
            let state = self
                .state
                .lock()
                .expect("monitoring runtime mutex poisoned");
            state.active.keys().cloned().collect::<Vec<_>>()
        };
        let interrupted = execution_ids.len();
        for execution_id in execution_ids {
            self.release(&execution_id, true);
        }
        interrupted
    }

    pub(crate) fn diagnostics(&self) -> RuntimeDiagnostics {
        let state = self
            .state
            .lock()
            .expect("monitoring runtime mutex poisoned");
        RuntimeDiagnostics {
            admitting: state.admitting,
            queue_depth: state.queue.len(),
            active_count: state.active.len(),
            global_in_use: state.active.len(),
            max_queue_depth: state.limits.queue_capacity,
            queue_full_count: state.queue_full_count,
            max_lag_ms: state.max_lag_ms,
            terminal_interrupted_count: state.terminal_interrupted_count,
        }
    }

    fn release(&self, execution_id: &str, interrupted: bool) {
        let mut state = self
            .state
            .lock()
            .expect("monitoring runtime mutex poisoned");
        let Some(active) = state.active.remove(execution_id) else {
            return;
        };
        state.active_by_monitor.remove(&active.command.monitor_id);
        for key in &active.command.station_key_ids {
            state.active_by_key.remove(key);
        }
        release_permits(&mut state, &active.command);
        if interrupted {
            state.terminal_interrupted_count = state.terminal_interrupted_count.saturating_add(1);
        }
    }
}

pub(crate) fn register_monitoring_runtime_task(
    supervisor: &TaskSupervisor,
    runtime: MonitoringRuntime,
) -> Result<TaskId, TaskSupervisorError> {
    let task_id = TaskId::from(MONITORING_RUNTIME_TASK_ID);
    supervisor.register(
        TaskSpec::new(
            task_id.clone(),
            MONITORING_RUNTIME_TASK_KIND,
            move |context: TaskRunContext| {
                let runtime = runtime.clone();
                Box::pin(async move {
                    context.cancellation_token.cancelled().await;
                    runtime.shutdown_begin();
                    runtime.interrupt_running();
                    Err(TaskFailure::cancelled())
                })
            },
        )
        .with_concurrency_key(MONITORING_RUNTIME_CONCURRENCY_KEY),
    )?;
    Ok(task_id)
}

#[derive(Debug)]
pub(crate) struct ExecutionGuard {
    runtime: MonitoringRuntime,
    execution_id: Option<String>,
}

impl ExecutionGuard {
    pub(crate) fn finish(mut self) {
        if let Some(execution_id) = self.execution_id.take() {
            self.runtime.release(&execution_id, false);
        }
    }
}

impl Drop for ExecutionGuard {
    fn drop(&mut self) {
        if let Some(execution_id) = self.execution_id.take() {
            self.runtime.release(&execution_id, false);
        }
    }
}

fn next_execution_id(state: &mut RuntimeState, command: &SchedulerCommand) -> String {
    let prefix = match command.trigger_kind {
        MonitorTriggerKind::Scheduled => "scheduled",
        MonitorTriggerKind::Manual => "manual",
    };
    let ordinal = state.next_execution_ordinal;
    state.next_execution_ordinal = state.next_execution_ordinal.saturating_add(1);
    format!("{prefix}-{}-{ordinal}", command.monitor_id)
}

fn can_acquire(state: &RuntimeState, command: &SchedulerCommand) -> bool {
    if state.active.len() >= state.limits.global_concurrency {
        return false;
    }
    if state
        .station_in_use
        .get(&command.station_id)
        .copied()
        .unwrap_or(0)
        >= state.limits.station_concurrency
    {
        return false;
    }
    command
        .station_key_ids
        .iter()
        .all(|key| state.key_in_use.get(key).copied().unwrap_or(0) < state.limits.key_concurrency)
}

fn acquire(state: &mut RuntimeState, command: &SchedulerCommand) {
    *state
        .station_in_use
        .entry(command.station_id.clone())
        .or_insert(0) += 1;
    for key in &command.station_key_ids {
        *state.key_in_use.entry(key.clone()).or_insert(0) += 1;
    }
}

fn release_permits(state: &mut RuntimeState, command: &SchedulerCommand) {
    decrement_or_remove(&mut state.station_in_use, &command.station_id);
    for key in &command.station_key_ids {
        decrement_or_remove(&mut state.key_in_use, key);
    }
}

fn decrement_or_remove(map: &mut HashMap<String, usize>, key: &str) {
    if let Some(value) = map.get_mut(key) {
        *value = value.saturating_sub(1);
        if *value == 0 {
            map.remove(key);
        }
    }
}
