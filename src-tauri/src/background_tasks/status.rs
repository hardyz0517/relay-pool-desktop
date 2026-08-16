use std::time::Duration;

use crate::background_tasks::task::{TaskId, TaskRunId};
use crate::services::secrets::mask::redact_text_preview;

const MAX_RUNTIME_FAILURE_CODE_BYTES: usize = 96;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TaskState {
    Registered,
    Running,
    Stopping,
    BackingOff { retry_at_ms: u64 },
    Succeeded,
    Failed { code: String },
    Cancelled,
    Panicked,
}

impl TaskState {
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Running | Self::Stopping)
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed { .. } | Self::Cancelled | Self::Panicked
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TaskStatusSnapshot {
    pub id: TaskId,
    pub kind: String,
    pub run_id: Option<TaskRunId>,
    pub state: TaskState,
    pub consecutive_failures: u32,
    pub last_delay: Option<Duration>,
    pub last_started_at_ms: Option<u64>,
    pub last_succeeded_at_ms: Option<u64>,
    pub last_failure_code: Option<String>,
    pub next_retry_at_ms: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeTaskSummary {
    pub id: String,
    pub kind: String,
    pub run_id: Option<u64>,
    pub status: RuntimeTaskStatus,
    pub last_started_at_ms: Option<u64>,
    pub last_succeeded_at_ms: Option<u64>,
    pub last_failure_code: Option<String>,
    pub consecutive_failures: u32,
    pub next_retry_at_ms: Option<u64>,
}

impl From<TaskStatusSnapshot> for RuntimeTaskSummary {
    fn from(snapshot: TaskStatusSnapshot) -> Self {
        let status = RuntimeTaskStatus::from(&snapshot.state);
        Self {
            id: snapshot.id.as_str().to_string(),
            kind: snapshot.kind,
            run_id: snapshot.run_id.map(|run_id| run_id.0),
            status,
            last_started_at_ms: snapshot.last_started_at_ms,
            last_succeeded_at_ms: snapshot.last_succeeded_at_ms,
            last_failure_code: snapshot
                .last_failure_code
                .or_else(|| failure_code_from_state(&snapshot.state))
                .map(|code| redact_runtime_failure_code(&code)),
            consecutive_failures: snapshot.consecutive_failures,
            next_retry_at_ms: snapshot.next_retry_at_ms.or_else(|| {
                matches!(snapshot.state, TaskState::BackingOff { .. })
                    .then(|| snapshot.last_delay.map(duration_millis_u64))
                    .flatten()
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeTaskStatus {
    Registered,
    Running,
    Stopping,
    BackingOff,
    Succeeded,
    Failed,
    Cancelled,
    Panicked,
}

impl RuntimeTaskStatus {
    pub fn is_active(self) -> bool {
        matches!(self, Self::Running | Self::Stopping)
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Succeeded | Self::Failed | Self::Cancelled | Self::Panicked
        )
    }
}

impl From<&TaskState> for RuntimeTaskStatus {
    fn from(state: &TaskState) -> Self {
        match state {
            TaskState::Registered => Self::Registered,
            TaskState::Running => Self::Running,
            TaskState::Stopping => Self::Stopping,
            TaskState::BackingOff { .. } => Self::BackingOff,
            TaskState::Succeeded => Self::Succeeded,
            TaskState::Failed { .. } => Self::Failed,
            TaskState::Cancelled => Self::Cancelled,
            TaskState::Panicked => Self::Panicked,
        }
    }
}

fn failure_code_from_state(state: &TaskState) -> Option<String> {
    match state {
        TaskState::Failed { code } => Some(code.clone()),
        _ => None,
    }
}

fn redact_runtime_failure_code(code: &str) -> String {
    let redacted = redact_text_preview(code, MAX_RUNTIME_FAILURE_CODE_BYTES);
    if redacted.contains("[REDACTED]") {
        "[REDACTED]".to_string()
    } else {
        redacted
    }
}

fn duration_millis_u64(duration: Duration) -> u64 {
    duration.as_millis().min(u128::from(u64::MAX)) as u64
}
