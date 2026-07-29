use std::{
    collections::BTreeMap,
    panic::AssertUnwindSafe,
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use futures_util::FutureExt;
use tokio::{runtime::Handle, task::JoinHandle};
use tokio_util::{sync::CancellationToken, task::TaskTracker};

use crate::background_tasks::{
    shutdown::{ShutdownError, ShutdownReport},
    status::{TaskState, TaskStatusSnapshot},
    task::{RestartClass, TaskFailure, TaskId, TaskRunContext, TaskRunId, TaskSpec},
};
use crate::observability::correlation;

type TaskJoinResult = Result<Result<(), TaskFailure>, ()>;

#[derive(Clone)]
pub struct TaskSupervisor {
    inner: Arc<Mutex<SupervisorInner>>,
    tracker: TaskTracker,
    spawn_handle: Option<Handle>,
}

#[derive(Default)]
struct SupervisorInner {
    tasks: BTreeMap<TaskId, TaskSlot>,
    next_run_id: u64,
}

struct TaskSlot {
    spec: TaskSpec,
    state: TaskState,
    run_id: Option<TaskRunId>,
    token: Option<CancellationToken>,
    join: Option<JoinHandle<TaskJoinResult>>,
    consecutive_failures: u32,
    last_delay: Option<Duration>,
    last_started_at_ms: Option<u64>,
    last_succeeded_at_ms: Option<u64>,
    last_failure_code: Option<String>,
    next_retry_at_ms: Option<u64>,
}

impl TaskSupervisor {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(SupervisorInner::default())),
            tracker: TaskTracker::new(),
            spawn_handle: None,
        }
    }

    pub fn with_spawn_handle(spawn_handle: Handle) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SupervisorInner::default())),
            tracker: TaskTracker::new(),
            spawn_handle: Some(spawn_handle),
        }
    }

    pub fn register(&self, spec: TaskSpec) -> Result<(), TaskSupervisorError> {
        let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
        if inner.tasks.contains_key(&spec.id) {
            return Err(TaskSupervisorError::DuplicateTaskId(spec.id));
        }
        inner.tasks.insert(
            spec.id.clone(),
            TaskSlot {
                spec,
                state: TaskState::Registered,
                run_id: None,
                token: None,
                join: None,
                consecutive_failures: 0,
                last_delay: None,
                last_started_at_ms: None,
                last_succeeded_at_ms: None,
                last_failure_code: None,
                next_retry_at_ms: None,
            },
        );
        Ok(())
    }

    pub fn start(&self, id: &TaskId) -> Result<TaskRunId, TaskSupervisorError> {
        let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
        self.start_locked(&mut inner, id)
    }

    pub fn cancel(&self, id: &TaskId) -> Result<(), TaskSupervisorError> {
        let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
        let slot = inner
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskSupervisorError::TaskNotFound(id.clone()))?;
        if !slot.state.is_active() {
            return Err(TaskSupervisorError::NotRunning(id.clone()));
        }
        if let Some(token) = &slot.token {
            token.cancel();
        }
        slot.state = TaskState::Stopping;
        Ok(())
    }

    pub fn status(&self, id: &TaskId) -> Result<TaskStatusSnapshot, TaskSupervisorError> {
        let inner = self.inner.lock().expect("task supervisor mutex poisoned");
        let slot = inner
            .tasks
            .get(id)
            .ok_or_else(|| TaskSupervisorError::TaskNotFound(id.clone()))?;
        Ok(slot.snapshot())
    }

    pub fn statuses(&self) -> Vec<TaskStatusSnapshot> {
        let inner = self.inner.lock().expect("task supervisor mutex poisoned");
        inner.tasks.values().map(TaskSlot::snapshot).collect()
    }

    pub async fn join_finished(&self, id: &TaskId) -> Result<TaskState, TaskSupervisorError> {
        let (join, run_id) = {
            let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
            let slot = inner
                .tasks
                .get_mut(id)
                .ok_or_else(|| TaskSupervisorError::TaskNotFound(id.clone()))?;
            let run_id = slot
                .run_id
                .ok_or_else(|| TaskSupervisorError::JoinHandleMissing(id.clone()))?;
            let join = slot
                .join
                .take()
                .ok_or_else(|| TaskSupervisorError::JoinHandleMissing(id.clone()))?;
            (join, run_id)
        };

        let joined = join
            .await
            .map_err(|_| TaskSupervisorError::JoinHandleMissing(id.clone()))?;
        let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
        let state = self.apply_join_result_locked(&mut inner, id, run_id, joined)?;
        Ok(state)
    }

    pub fn tick(&self, now_ms: u64) -> Result<Vec<TaskRunId>, TaskSupervisorError> {
        let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
        let ready = inner
            .tasks
            .iter()
            .filter_map(|(id, slot)| match slot.state {
                TaskState::BackingOff { retry_at_ms } if retry_at_ms <= now_ms => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut started = Vec::with_capacity(ready.len());
        for id in ready {
            started.push(self.start_locked(&mut inner, &id)?);
        }
        Ok(started)
    }

    pub async fn shutdown(&self, timeout: Duration) -> Result<ShutdownReport, ShutdownError> {
        let active_ids = {
            let mut inner = self.inner.lock().expect("task supervisor mutex poisoned");
            let mut active_ids = Vec::new();
            for (id, slot) in &mut inner.tasks {
                if slot.state.is_active() {
                    if let Some(token) = &slot.token {
                        token.cancel();
                    }
                    slot.state = TaskState::Stopping;
                    active_ids.push(id.clone());
                }
            }
            active_ids
        };

        let mut report = ShutdownReport {
            cancelled: active_ids.clone(),
            ..ShutdownReport::default()
        };
        self.tracker.close();
        if tokio::time::timeout(timeout, self.tracker.wait())
            .await
            .is_err()
        {
            report.timed_out = active_ids;
            return Err(ShutdownError { timeout, report });
        }

        for id in active_ids {
            if self.join_finished(&id).await.is_ok() {
                report.completed.push(id);
            }
        }
        Ok(report)
    }

    fn start_locked(
        &self,
        inner: &mut SupervisorInner,
        id: &TaskId,
    ) -> Result<TaskRunId, TaskSupervisorError> {
        let concurrency_key = {
            let slot = inner
                .tasks
                .get(id)
                .ok_or_else(|| TaskSupervisorError::TaskNotFound(id.clone()))?;
            if slot.state.is_active() {
                return Err(TaskSupervisorError::AlreadyRunning(id.clone()));
            }
            slot.spec.concurrency_key.clone()
        };
        if let Some(key) = &concurrency_key {
            if inner.tasks.iter().any(|(other_id, slot)| {
                other_id != id
                    && slot.spec.concurrency_key.as_ref() == Some(key)
                    && slot.state.is_active()
            }) {
                return Err(TaskSupervisorError::ConcurrencyKeyRunning(key.clone()));
            }
        }

        inner.next_run_id = inner.next_run_id.saturating_add(1);
        let run_id = TaskRunId(inner.next_run_id);
        let slot = inner
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskSupervisorError::TaskNotFound(id.clone()))?;
        let correlation_id = correlation::current_or_new();
        let token = CancellationToken::new();
        let context = TaskRunContext {
            task_id: slot.spec.id.clone(),
            run_id,
            correlation_id: correlation_id.as_str().to_string(),
            cancellation_token: token.clone(),
        };
        let body = Arc::clone(&slot.spec.body);
        let task = async move {
            correlation::in_scope("task.run", correlation_id, async move {
                match AssertUnwindSafe((body)(context)).catch_unwind().await {
                    Ok(result) => Ok(result),
                    Err(_) => Err(()),
                }
            })
            .await
        };
        let join = if let Some(handle) = &self.spawn_handle {
            self.tracker.spawn_on(task, handle)
        } else {
            self.tracker.spawn(task)
        };

        slot.state = TaskState::Running;
        slot.run_id = Some(run_id);
        slot.token = Some(token);
        slot.join = Some(join);
        slot.last_started_at_ms = Some(now_epoch_millis());
        slot.next_retry_at_ms = None;
        Ok(run_id)
    }

    fn apply_join_result_locked(
        &self,
        inner: &mut SupervisorInner,
        id: &TaskId,
        run_id: TaskRunId,
        joined: TaskJoinResult,
    ) -> Result<TaskState, TaskSupervisorError> {
        let slot = inner
            .tasks
            .get_mut(id)
            .ok_or_else(|| TaskSupervisorError::TaskNotFound(id.clone()))?;
        if slot.run_id != Some(run_id) {
            return Err(TaskSupervisorError::IllegalTransition(id.clone()));
        }
        slot.token = None;
        slot.join = None;
        let next_state = match joined {
            Ok(Ok(())) => {
                slot.consecutive_failures = 0;
                slot.last_delay = None;
                slot.last_failure_code = None;
                slot.next_retry_at_ms = None;
                slot.last_succeeded_at_ms = Some(now_epoch_millis());
                TaskState::Succeeded
            }
            Ok(Err(error)) if error.class == RestartClass::Cancelled => {
                slot.next_retry_at_ms = None;
                TaskState::Cancelled
            }
            Ok(Err(error))
                if error.class == RestartClass::Transient
                    && slot.consecutive_failures < slot.spec.restart_policy.max_retries =>
            {
                slot.consecutive_failures = slot.consecutive_failures.saturating_add(1);
                let delay = slot
                    .spec
                    .restart_policy
                    .delay_for_attempt(slot.consecutive_failures);
                slot.last_delay = Some(delay);
                slot.last_failure_code = Some(error.code);
                slot.next_retry_at_ms =
                    Some(now_epoch_millis().saturating_add(duration_millis_u64(delay)));
                TaskState::BackingOff {
                    retry_at_ms: delay.as_millis() as u64,
                }
            }
            Ok(Err(error)) => {
                slot.consecutive_failures = slot.consecutive_failures.saturating_add(1);
                slot.last_failure_code = Some(error.code.clone());
                slot.next_retry_at_ms = None;
                TaskState::Failed { code: error.code }
            }
            Err(()) => {
                slot.last_failure_code = Some("panic".to_string());
                slot.next_retry_at_ms = None;
                TaskState::Panicked
            }
        };
        slot.state = next_state.clone();
        Ok(next_state)
    }
}

impl Default for TaskSupervisor {
    fn default() -> Self {
        Self::new()
    }
}

impl TaskSlot {
    fn snapshot(&self) -> TaskStatusSnapshot {
        TaskStatusSnapshot {
            id: self.spec.id.clone(),
            kind: self.spec.kind.clone(),
            run_id: self.run_id,
            state: self.state.clone(),
            consecutive_failures: self.consecutive_failures,
            last_delay: self.last_delay,
            last_started_at_ms: self.last_started_at_ms,
            last_succeeded_at_ms: self.last_succeeded_at_ms,
            last_failure_code: self.last_failure_code.clone(),
            next_retry_at_ms: self.next_retry_at_ms,
        }
    }
}

fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

fn duration_millis_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}

#[derive(Debug, PartialEq, Eq)]
pub enum TaskSupervisorError {
    DuplicateTaskId(TaskId),
    TaskNotFound(TaskId),
    AlreadyRunning(TaskId),
    NotRunning(TaskId),
    ConcurrencyKeyRunning(String),
    JoinHandleMissing(TaskId),
    IllegalTransition(TaskId),
}

impl std::fmt::Display for TaskSupervisorError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateTaskId(id) => write!(formatter, "duplicate task id: {id}"),
            Self::TaskNotFound(id) => write!(formatter, "task not found: {id}"),
            Self::AlreadyRunning(id) => write!(formatter, "task already running: {id}"),
            Self::NotRunning(id) => write!(formatter, "task is not running: {id}"),
            Self::ConcurrencyKeyRunning(key) => {
                write!(formatter, "concurrency key already running: {key}")
            }
            Self::JoinHandleMissing(id) => write!(formatter, "join handle missing: {id}"),
            Self::IllegalTransition(id) => write!(formatter, "illegal task transition: {id}"),
        }
    }
}

impl std::error::Error for TaskSupervisorError {}
