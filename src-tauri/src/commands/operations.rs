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

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<OperationSnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_operation_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = OperationIdInputDto::parse(input)?;
            runtime
                .operation
                .status(input.operation_id())
                .map(OperationSnapshotDto::from)
                .map_err(super::public_operation_registry_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn cancel_operation(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CancelOperationOutcomeDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "cancel_operation",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CancelOperationInputDto::parse(input)?;
            runtime
                .operation
                .cancel(input.operation_id(), input.wait())
                .await
                .map(CancelOperationOutcomeDto::from)
                .map_err(super::public_operation_registry_error)
        },
    )
    .await
}
