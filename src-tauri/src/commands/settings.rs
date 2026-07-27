use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::SettingsStationsCommandFacade,
    commands::error,
    ipc::dto::{
        settings::{SettingsDto, UpdateLocalAccessKeyInputDto, UpdateSettingsInputDto},
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn get_settings(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("get_settings", async {
        EmptyInputDto::parse(input)?;
        facade
            .get_settings()
            .await
            .map(SettingsDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_local_access_key(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<String, error::CommandError> {
    correlation::in_command_scope("get_local_access_key", async {
        EmptyInputDto::parse(input)?;
        facade
            .get_local_access_key()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_local_access_key(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("update_local_access_key", async {
        let input = UpdateLocalAccessKeyInputDto::parse(input)?;
        facade
            .update_local_access_key(input.value)
            .await
            .map(SettingsDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_settings(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("update_settings", async {
        let input = UpdateSettingsInputDto::parse(input)?.into_domain()?;
        let settings = facade
            .update_settings(input)
            .await
            .map_err(super::public_command_application_error)?;
        Ok(SettingsDto::from(settings))
    })
    .await
}
