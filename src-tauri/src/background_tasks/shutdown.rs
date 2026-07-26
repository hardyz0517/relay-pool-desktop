use std::time::Duration;

use crate::background_tasks::task::TaskId;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ShutdownReport {
    pub cancelled: Vec<TaskId>,
    pub completed: Vec<TaskId>,
    pub timed_out: Vec<TaskId>,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ShutdownError {
    pub timeout: Duration,
    pub report: ShutdownReport,
}

impl std::fmt::Display for ShutdownError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "task supervisor shutdown timed out after {:?}",
            self.timeout
        )
    }
}

impl std::error::Error for ShutdownError {}
