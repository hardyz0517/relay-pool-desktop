use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::CredentialsCommandFacade,
    commands::error,
    ipc::dto::station_keys::{
        CommonLoginEmailDto, CommonLoginIdInputDto, CommonLoginOptionsDto, CommonLoginPasswordDto,
        StationCredentialsDto, StationIdInputDto, UpdateStationCredentialsInputDto,
        UpdateStationSessionInputDto, UpsertCommonLoginEmailInputDto,
        UpsertCommonLoginPasswordInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_common_login_options(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<CommonLoginOptionsDto, error::CommandError> {
    correlation::in_command_scope("list_common_login_options", async {
        crate::ipc::dto::EmptyInputDto::parse(input)?;
        facade
            .list_common_login_options()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_common_login_email(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<CommonLoginEmailDto, error::CommandError> {
    correlation::in_command_scope("upsert_common_login_email", async {
        let input = UpsertCommonLoginEmailInputDto::parse(input)?;
        facade
            .upsert_common_login_email(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_common_login_email(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_common_login_email", async {
        let input = CommonLoginIdInputDto::parse(input)?;
        facade
            .delete_common_login_email(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_common_login_password(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<CommonLoginPasswordDto, error::CommandError> {
    correlation::in_command_scope("upsert_common_login_password", async {
        let input = UpsertCommonLoginPasswordInputDto::parse(input)?;
        facade
            .upsert_common_login_password(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_common_login_password(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_common_login_password", async {
        let input = CommonLoginIdInputDto::parse(input)?;
        facade
            .delete_common_login_password(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_common_login_password(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<String, error::CommandError> {
    correlation::in_command_scope("get_common_login_password", async {
        let input = CommonLoginIdInputDto::parse(input)?;
        facade
            .get_common_login_password(input.id)
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
