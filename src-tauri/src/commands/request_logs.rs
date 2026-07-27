use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::RequestLogsCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{change_logs::RequestLogDto, EmptyInputDto},
    observability::correlation,
};

#[tauri::command]
pub async fn list_request_logs(
    facade: State<'_, RequestLogsCommandFacade>,
    input: Value,
) -> Result<Vec<RequestLogDto>, error::CommandError> {
    correlation::in_command_scope("list_request_logs", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_request_logs(PageLimit::new(500).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_request_logs(
    facade: State<'_, RequestLogsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("clear_request_logs", async {
        EmptyInputDto::parse(input)?;
        facade
            .clear_request_logs()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
