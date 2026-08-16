use serde_json::Value;
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    commands::error,
    ipc::{
        dto::{runtime_status::RuntimeStatusDto, settings::AppStatusDto, EmptyInputDto},
        runtime_contract::{current_runtime_contract, RuntimeContractInfo},
    },
    models::AppStatus,
    observability::correlation,
};

/// Request a full desktop restart after a successful maintenance/update
/// operation. This is intentionally a non-idempotent command: once the
/// request is accepted, the process may exit before the IPC response reaches
/// the frontend. The shared helper records the typed restart event and lets
/// `ExitCoordinator` own the shutdown drain.
#[tauri::command]
pub async fn restart_application(
    app: tauri::AppHandle,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "restart_application",
        runtime_context_registry.inner(),
        runtime_context,
        async move {
            EmptyInputDto::parse(input)?;
            crate::request_application_restart(&app);
            Ok(())
        },
    )
    .await
}

#[tauri::command]
pub async fn app_status(
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<AppStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "app_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            Ok(AppStatus::default())
        },
    )
    .await
}

/// Returns only the immutable build/IPC identity needed before normal app queries.
#[tauri::command]
pub async fn get_runtime_contract_info(
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RuntimeContractInfo, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_runtime_contract_info",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            Ok(current_runtime_contract())
        },
    )
    .await
}

#[tauri::command]
pub async fn get_runtime_status(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RuntimeStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_runtime_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            Ok(RuntimeStatusDto::from(
                runtime
                    .supervisor
                    .statuses()
                    .into_iter()
                    .map(Into::into)
                    .collect::<Vec<_>>(),
            ))
        },
    )
    .await
}
