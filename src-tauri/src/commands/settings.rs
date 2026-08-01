use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::SettingsStationsCommandFacade,
    commands::error,
    ipc::dto::{
        settings::{
            ConfirmHierarchicalRoutingMigrationInputDto, OpenExternalUrlInputDto, SettingsDto,
            UpdateLocalAccessKeyInputDto, UpdateSettingsInputDto,
        },
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

#[tauri::command]
pub async fn confirm_hierarchical_routing_migration(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("confirm_hierarchical_routing_migration", async {
        let input = ConfirmHierarchicalRoutingMigrationInputDto::parse(input)?.into_domain()?;
        let settings = facade
            .confirm_hierarchical_routing_migration(input)
            .await
            .map_err(super::public_command_application_error)?;
        Ok(SettingsDto::from(settings))
    })
    .await
}

#[tauri::command]
pub async fn open_external_url(input: Value) -> Result<(), error::CommandError> {
    correlation::in_command_scope("open_external_url", async {
        let input = OpenExternalUrlInputDto::parse(input)?;
        let url = super::validate_external_http_url(&input.url)?;
        Ok(super::open_url_with_system(url)?)
    })
    .await
}
