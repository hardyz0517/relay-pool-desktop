use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::CredentialsCommandFacade,
    commands::error,
    ipc::dto::station_keys::{
        StationCredentialsDto, StationIdInputDto, UpdateStationCredentialsInputDto,
        UpdateStationSessionInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn get_station_credentials(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("get_station_credentials", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .get_station_credentials(input.station_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_credentials(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("update_station_credentials", async {
        let input = UpdateStationCredentialsInputDto::parse(input)?;
        facade
            .update_station_credentials(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_session(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("update_station_session", async {
        let input = UpdateStationSessionInputDto::parse(input)?;
        facade
            .update_station_session(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_station_credentials(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("clear_station_credentials", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .clear_station_credentials(input.station_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
