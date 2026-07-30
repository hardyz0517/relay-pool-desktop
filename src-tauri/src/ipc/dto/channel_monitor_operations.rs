use serde_json::Value;

use crate::models::monitoring::{
    CancelChannelMonitorExecutionInput, CancelChannelMonitorExecutionReceipt,
    ChannelStatusWorkspaceV2,
};

use super::{invalid_input, TypeDescriptor};

pub type ChannelStatusWorkspaceDto = ChannelStatusWorkspaceV2;
pub type CancelChannelMonitorExecutionInputDto = CancelChannelMonitorExecutionInput;
pub type CancelChannelMonitorExecutionReceiptDto = CancelChannelMonitorExecutionReceipt;

impl CancelChannelMonitorExecutionInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The channel monitor operation payload is invalid.",
            )
        })?;
        let valid = !input.execution_id.is_empty()
            && input.execution_id.len() <= 128
            && input.execution_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            });
        if !valid {
            return Err(invalid_input(
                "executionId",
                "invalid_id",
                "The execution identifier is invalid.",
            ));
        }
        Ok(input)
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const CHANNEL_MONITOR_OPERATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ChannelMonitorOperationsDto",
    typescript: include_str!("channel_monitor_operations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "command": "run_channel_monitor_now",
            "input": {
                "monitorId": "monitor-1",
                "triggerRequestId": "manual:monitor-1:fixture"
            },
            "output": {
                "executionId": "execution-1",
                "monitorId": "monitor-1",
                "status": "completed",
                "triggerRequestId": "manual:monitor-1:fixture",
                "reusedExisting": false
            }
        }),
        serde_json::json!({
            "command": "cancel_channel_monitor_execution",
            "input": {
                "executionId": "execution-1"
            },
            "output": {
                "executionId": "execution-1",
                "status": "cancelled",
                "cancelled": true
            }
        }),
    ]
}
