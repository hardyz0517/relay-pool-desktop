use serde_json::Value;
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    commands::error,
    ipc::dto::operations::{
        CancelOperationInputDto, CancelOperationOutcomeDto, OperationIdInputDto,
        OperationSnapshotDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn get_operation_status(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<OperationSnapshotDto, error::CommandError> {
    correlation::in_command_scope("get_operation_status", async {
        let input = OperationIdInputDto::parse(input)?;
        runtime
            .operation
            .status(input.operation_id())
            .map(OperationSnapshotDto::from)
            .map_err(super::public_operation_registry_error)
    })
    .await
}

#[tauri::command]
pub async fn cancel_operation(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<CancelOperationOutcomeDto, error::CommandError> {
    correlation::in_command_scope("cancel_operation", async {
        let input = CancelOperationInputDto::parse(input)?;
        runtime
            .operation
            .cancel(input.operation_id(), input.wait())
            .await
            .map(CancelOperationOutcomeDto::from)
            .map_err(super::public_operation_registry_error)
    })
    .await
}
