use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::ChannelStatusCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{
        channel_monitor_operations::ChannelStatusWorkspaceDto,
        channel_monitor_reads::ChannelStatusSummaryDto, EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_channel_status_summaries(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelStatusSummaryDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_status_summaries", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_channel_status_summaries(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_channel_status_workspace(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,
) -> Result<ChannelStatusWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("load_channel_status_workspace", async {
        EmptyInputDto::parse(input)?;
        facade
            .load_channel_status_workspace(PageLimit::new(200).expect("bounded limit"))
            .await
            .map(ChannelStatusWorkspaceDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}
