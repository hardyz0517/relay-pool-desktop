#![allow(
    dead_code,
    reason = "Task 18.D publishes the runtime status DTO before frontend diagnostics surfaces consume it"
)]

use serde::Serialize;

use crate::background_tasks::{RuntimeTaskStatus, RuntimeTaskSummary};

use super::TypeDescriptor;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeStatusDto {
    pub tasks: Vec<RuntimeTaskSummaryDto>,
}

impl From<Vec<RuntimeTaskSummary>> for RuntimeStatusDto {
    fn from(tasks: Vec<RuntimeTaskSummary>) -> Self {
        Self {
            tasks: tasks.into_iter().map(Into::into).collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTaskSummaryDto {
    pub id: String,
    pub kind: String,
    pub run_id: Option<u64>,
    pub status: RuntimeTaskStatusDto,
    pub last_started_at_ms: Option<u64>,
    pub last_succeeded_at_ms: Option<u64>,
    pub last_failure_code: Option<String>,
    pub consecutive_failures: u32,
    pub next_retry_at_ms: Option<u64>,
}

impl From<RuntimeTaskSummary> for RuntimeTaskSummaryDto {
    fn from(summary: RuntimeTaskSummary) -> Self {
        Self {
            id: summary.id,
            kind: summary.kind,
            run_id: summary.run_id,
            status: summary.status.into(),
            last_started_at_ms: summary.last_started_at_ms,
            last_succeeded_at_ms: summary.last_succeeded_at_ms,
            last_failure_code: summary.last_failure_code,
            consecutive_failures: summary.consecutive_failures,
            next_retry_at_ms: summary.next_retry_at_ms,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeTaskStatusDto {
    Registered,
    Running,
    Stopping,
    BackingOff,
    Succeeded,
    Failed,
    Cancelled,
    Panicked,
}

impl From<RuntimeTaskStatus> for RuntimeTaskStatusDto {
    fn from(status: RuntimeTaskStatus) -> Self {
        match status {
            RuntimeTaskStatus::Registered => Self::Registered,
            RuntimeTaskStatus::Running => Self::Running,
            RuntimeTaskStatus::Stopping => Self::Stopping,
            RuntimeTaskStatus::BackingOff => Self::BackingOff,
            RuntimeTaskStatus::Succeeded => Self::Succeeded,
            RuntimeTaskStatus::Failed => Self::Failed,
            RuntimeTaskStatus::Cancelled => Self::Cancelled,
            RuntimeTaskStatus::Panicked => Self::Panicked,
        }
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const RUNTIME_STATUS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "RuntimeStatusDto",
    typescript: include_str!("runtime_status.typescript.txt"),
};

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::background_tasks::RuntimeTaskStatus;

    #[test]
    fn runtime_status_dto_exposes_only_actionable_task_fields() {
        let dto = RuntimeStatusDto::from(vec![RuntimeTaskSummary {
            id: "station-collector-runner".to_string(),
            kind: "station_collector_runner".to_string(),
            run_id: Some(7),
            status: RuntimeTaskStatus::BackingOff,
            last_started_at_ms: Some(100),
            last_succeeded_at_ms: Some(90),
            last_failure_code: Some("[REDACTED]".to_string()),
            consecutive_failures: 2,
            next_retry_at_ms: Some(150),
        }]);

        let value = serde_json::to_value(dto).expect("serialize runtime status");
        assert_eq!(value["tasks"][0]["id"], "station-collector-runner");
        assert_eq!(value["tasks"][0]["status"], "backing_off");
        assert_eq!(value["tasks"][0]["lastFailureCode"], "[REDACTED]");

        let root = value.as_object().expect("runtime status root object");
        assert_eq!(
            root.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from(["tasks"])
        );
        let task = value["tasks"][0]
            .as_object()
            .expect("runtime status task object");
        assert_eq!(
            task.keys().map(String::as_str).collect::<BTreeSet<_>>(),
            BTreeSet::from([
                "consecutiveFailures",
                "id",
                "kind",
                "lastFailureCode",
                "lastStartedAtMs",
                "lastSucceededAtMs",
                "nextRetryAtMs",
                "runId",
                "status",
            ])
        );
        for forbidden in [
            "blocking",
            "databasePath",
            "errorChain",
            "metrics",
            "operations",
            "outbound",
            "rawDiagnostics",
        ] {
            assert!(task.get(forbidden).is_none(), "{forbidden}");
            assert!(root.get(forbidden).is_none(), "{forbidden}");
        }
    }
}
