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
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("choose_data_dir", async {
        EmptyInputDto::parse(input)?;
        facade
            .choose_data_dir()
            .await
            .map(SettingsDto::from)
            .map_err(super::public_data_directory_error)
    })
    .await
}

#[tauri::command]
pub async fn reset_data_dir(
    facade: State<'_, DataDirectoryCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("reset_data_dir", async {
        EmptyInputDto::parse(input)?;
        facade
            .reset_data_dir()
            .await
            .map(SettingsDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}
