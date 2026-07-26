use std::time::Duration;

use crate::background_tasks::task::TaskId;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TaskRunId(pub u64);

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
}
