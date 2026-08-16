use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::RequestLogsCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{request_logs::RequestLogDto, EmptyInputDto},
    observability::correlation,
};

#[tauri::command]
pub async fn list_request_logs(
    facade: State<'_, RequestLogsCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<RequestLogDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_request_logs",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_request_logs(PageLimit::new(500).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn clear_request_logs(
    facade: State<'_, RequestLogsCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "clear_request_logs",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .clear_request_logs()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
