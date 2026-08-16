use serde_json::Value;
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    commands::error,
    ipc::dto::{
        updater_data_recovery::{
            PublishedUpdateInspectionDto, PublishedUpdateInspectionInputDto,
            UpdaterNetworkConfigDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
    services::updater,
};

#[tauri::command]
pub async fn updater_network_config(
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<UpdaterNetworkConfigDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "updater_network_config",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            Ok(updater::network_config())
        },
    )
    .await
}

#[tauri::command]
pub async fn inspect_latest_update_manifest(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<PublishedUpdateInspectionDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "inspect_latest_update_manifest",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = PublishedUpdateInspectionInputDto::parse(input)?;
            Ok(crate::observability::runtime::bootstrap::record_failure(
                crate::services::updater::runtime_events::manifest_inspect_failed(),
                updater::inspect_latest_update_manifest(&runtime.outbound, &input.current_version)
                    .await,
            )?)
        },
    )
    .await
}
