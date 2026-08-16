use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::DataDirectoryCommandFacade,
    commands::error,
    ipc::dto::{settings::SettingsDto, EmptyInputDto},
    observability::correlation,
};

#[tauri::command]
pub async fn choose_data_dir(
    facade: State<'_, DataDirectoryCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "choose_data_dir",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .choose_data_dir()
                .await
                .map(SettingsDto::from)
                .map_err(super::public_data_directory_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn reset_data_dir(
    facade: State<'_, DataDirectoryCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "reset_data_dir",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .reset_data_dir()
                .await
                .map(SettingsDto::from)
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
