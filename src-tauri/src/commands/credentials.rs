use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::CredentialsCommandFacade,
    commands::error,
    ipc::dto::station_keys::{
        CommonLoginProfileDto, CommonLoginProfileIdInputDto, StationCredentialsDto,
        StationIdInputDto, UpdateStationCredentialsInputDto, UpdateStationSessionInputDto,
        UpsertCommonLoginProfileInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_common_login_profiles(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<Vec<CommonLoginProfileDto>, error::CommandError> {
    correlation::in_command_scope("list_common_login_profiles", async {
        crate::ipc::dto::EmptyInputDto::parse(input)?;
        facade
            .list_common_login_profiles()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_common_login_profile(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<CommonLoginProfileDto, error::CommandError> {
    correlation::in_command_scope("upsert_common_login_profile", async {
        let input = UpsertCommonLoginProfileInputDto::parse(input)?;
        facade
            .upsert_common_login_profile(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_common_login_profile(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_common_login_profile", async {
        let input = CommonLoginProfileIdInputDto::parse(input)?;
        facade
            .delete_common_login_profile(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_common_login_profile_password(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<String, error::CommandError> {
    correlation::in_command_scope("get_common_login_profile_password", async {
        let input = CommonLoginProfileIdInputDto::parse(input)?;
        facade
            .get_common_login_profile_password(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

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
