#![allow(
    dead_code,
    reason = "Task 15.D publishes stable operation DTOs before Task 15.E wires production operation commands"
)]

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::background_tasks::{
    OperationCancelOutcome, OperationId, OperationProgress, OperationSnapshot, OperationState,
    OperationTerminal,
};

use super::{invalid_input, TypeDescriptor};

const MAX_OPERATION_ID_DIGITS: usize = 20;
const DEFAULT_CANCEL_WAIT_MS: u64 = 250;
const MAX_CANCEL_WAIT_MS: u64 = 5_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationIdInputDto {
    pub operation_id: String,
}

impl OperationIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        parse_operation_id("operationId", &input.operation_id)?;
        Ok(input)
    }

    pub fn operation_id(&self) -> OperationId {
        parse_operation_id("operationId", &self.operation_id)
            .expect("OperationIdInputDto is validated during parse")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CancelOperationInputDto {
    pub operation_id: String,
    pub wait_ms: Option<u64>,
}

impl CancelOperationInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        parse_operation_id("operationId", &input.operation_id)?;
        if input.wait_ms.unwrap_or(DEFAULT_CANCEL_WAIT_MS) > MAX_CANCEL_WAIT_MS {
            return Err(invalid_input(
                "waitMs",
                "out_of_range",
                "The cancellation wait budget is out of range.",
            ));
        }
        Ok(input)
    }

    pub fn operation_id(&self) -> OperationId {
        parse_operation_id("operationId", &self.operation_id)
            .expect("CancelOperationInputDto is validated during parse")
    }

    pub fn wait(&self) -> Duration {
        Duration::from_millis(self.wait_ms.unwrap_or(DEFAULT_CANCEL_WAIT_MS))
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationSnapshotDto {
    pub operation_id: String,
    pub kind: String,
    pub owner_feature: String,
    pub state: OperationStateDto,
    pub progress: Vec<OperationProgressDto>,
    pub terminal: Option<OperationTerminalDto>,
}

impl From<OperationSnapshot> for OperationSnapshotDto {
    fn from(snapshot: OperationSnapshot) -> Self {
        Self {
            operation_id: snapshot.id.as_u64().to_string(),
            kind: snapshot.kind,
            owner_feature: snapshot.owner.feature,
            state: OperationStateDto::from(snapshot.state),
            progress: snapshot.progress.into_iter().map(Into::into).collect(),
            terminal: snapshot.terminal.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum OperationStateDto {
    Running,
    Stopping,
    Terminal { terminal: OperationTerminalDto },
}

impl From<OperationState> for OperationStateDto {
    fn from(state: OperationState) -> Self {
        match state {
            OperationState::Running => Self::Running,
            OperationState::Stopping => Self::Stopping,
            OperationState::Terminal { terminal, .. } => Self::Terminal {
                terminal: terminal.into(),
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "terminal", rename_all = "snake_case")]
pub enum OperationTerminalDto {
    Completed,
    Failed { code: String },
    Cancelled,
    TimedOut,
    ResultUnknown,
}

impl From<OperationTerminal> for OperationTerminalDto {
    fn from(terminal: OperationTerminal) -> Self {
        match terminal {
            OperationTerminal::Completed => Self::Completed,
            OperationTerminal::Failed { code } => Self::Failed {
                code: code.as_str().to_string(),
            },
            OperationTerminal::Cancelled => Self::Cancelled,
            OperationTerminal::TimedOut => Self::TimedOut,
            OperationTerminal::ResultUnknown => Self::ResultUnknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct OperationProgressDto {
    pub sequence: u64,
    pub message: String,
}

impl From<OperationProgress> for OperationProgressDto {
    fn from(progress: OperationProgress) -> Self {
        Self {
            sequence: progress.sequence,
            message: progress.message,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CancelOperationOutcomeDto {
    Stopped { terminal: OperationTerminalDto },
    StillStopping,
    AlreadyTerminal { terminal: OperationTerminalDto },
}

impl From<OperationCancelOutcome> for CancelOperationOutcomeDto {
    fn from(outcome: OperationCancelOutcome) -> Self {
        match outcome {
            OperationCancelOutcome::Stopped { terminal } => Self::Stopped {
                terminal: terminal.into(),
            },
            OperationCancelOutcome::StillStopping => Self::StillStopping,
            OperationCancelOutcome::AlreadyTerminal { terminal } => Self::AlreadyTerminal {
                terminal: terminal.into(),
            },
        }
    }
}

fn parse_operation_id(
    field: &'static str,
    value: &str,
) -> Result<OperationId, crate::commands::error::CommandError> {
    if value.is_empty()
        || value.len() > MAX_OPERATION_ID_DIGITS
        || !value.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The operation id is invalid.",
        ));
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| invalid_input(field, "invalid_id", "The operation id is invalid."))?;
    OperationId::from_u64(parsed)
        .ok_or_else(|| invalid_input(field, "invalid_id", "The operation id is invalid."))
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The command input shape is invalid.",
        )
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub const OPERATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "OperationSnapshotDto",
    typescript: include_str!("operations.typescript.txt"),
};

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use serde_json::json;

    use super::*;
    use crate::background_tasks::{
        OperationFailureCode, OperationOwner, OperationProgress, OperationState, OperationTerminal,
    };

    #[test]
    fn operation_id_input_rejects_missing_zero_and_non_numeric_ids() {
        for value in [
            json!({}),
            json!({ "operationId": "" }),
            json!({ "operationId": "0" }),
            json!({ "operationId": "not-a-number" }),
            json!({ "operationId": "18446744073709551616" }),
        ] {
            OperationIdInputDto::parse(value).expect_err("invalid operation id must fail");
        }

        let input = OperationIdInputDto::parse(json!({ "operationId": "42" })).expect("valid id");
        assert_eq!(input.operation_id().as_u64(), 42);
    }

    #[test]
    fn cancel_operation_input_bounds_wait_budget() {
        let input = CancelOperationInputDto::parse(json!({ "operationId": "7" }))
            .expect("default wait is valid");
        assert_eq!(input.operation_id().as_u64(), 7);
        assert_eq!(input.wait(), Duration::from_millis(DEFAULT_CANCEL_WAIT_MS));

        let input = CancelOperationInputDto::parse(json!({
            "operationId": "7",
            "waitMs": 5_000
        }))
        .expect("maximum wait is valid");
        assert_eq!(input.wait(), Duration::from_millis(5_000));

        CancelOperationInputDto::parse(json!({
            "operationId": "7",
            "waitMs": 5_001
        }))
        .expect_err("oversized cancel wait is rejected");
    }

    #[test]
    fn operation_snapshot_projects_stable_public_shape_without_instants() {
        let snapshot = OperationSnapshot {
            id: OperationId::from_u64(9).expect("valid operation id"),
            kind: "connectivity".to_string(),
            owner: OperationOwner::new("key-pool"),
            state: OperationState::Terminal {
                terminal: OperationTerminal::Failed {
                    code: OperationFailureCode::new("provider-timeout"),
                },
                recorded_at: Instant::now(),
            },
            started_at: Instant::now(),
            progress: vec![OperationProgress {
                id: OperationId::from_u64(9).expect("valid operation id"),
                sequence: 3,
                message: "probing".to_string(),
            }],
            terminal: Some(OperationTerminal::Failed {
                code: OperationFailureCode::new("provider-timeout"),
            }),
        };

        let value = serde_json::to_value(OperationSnapshotDto::from(snapshot))
            .expect("operation snapshot dto serializes");

        assert_eq!(value["operationId"], "9");
        assert_eq!(value["kind"], "connectivity");
        assert_eq!(value["ownerFeature"], "key-pool");
        assert_eq!(value["state"]["state"], "terminal");
        assert_eq!(value["state"]["terminal"]["terminal"], "failed");
        assert_eq!(value["state"]["terminal"]["code"], "provider-timeout");
        assert_eq!(value["progress"][0]["sequence"], 3);
        assert_eq!(value["progress"][0]["message"], "probing");
        assert!(value.get("startedAt").is_none());
    }

    #[test]
    fn cancel_operation_outcome_projects_stable_public_shape() {
        let stopped = serde_json::to_value(CancelOperationOutcomeDto::from(
            OperationCancelOutcome::Stopped {
                terminal: OperationTerminal::Cancelled,
            },
        ))
        .expect("stopped outcome serializes");
        assert_eq!(stopped["outcome"], "stopped");
        assert_eq!(stopped["terminal"]["terminal"], "cancelled");

        let still_stopping = serde_json::to_value(CancelOperationOutcomeDto::from(
            OperationCancelOutcome::StillStopping,
        ))
        .expect("still-stopping outcome serializes");
        assert_eq!(still_stopping["outcome"], "still_stopping");

        let already_terminal = serde_json::to_value(CancelOperationOutcomeDto::from(
            OperationCancelOutcome::AlreadyTerminal {
                terminal: OperationTerminal::TimedOut,
            },
        ))
        .expect("already-terminal outcome serializes");
        assert_eq!(already_terminal["outcome"], "already_terminal");
        assert_eq!(already_terminal["terminal"]["terminal"], "timed_out");
    }
}
