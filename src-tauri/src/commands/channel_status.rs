use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::ChannelStatusCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{
        channel_monitor_operations::ChannelStatusWorkspaceDto,
        channel_monitor_reads::{
            ChannelMonitorAttemptHistoryInputDto, ChannelMonitorAttemptPageDto,
            ChannelMonitorExecutionDetailDto, ChannelMonitorExecutionIdInputDto,
            ChannelMonitorExecutionListInputDto, ChannelMonitorExecutionPageDto,
            ChannelStatusSummaryDto, ChannelStatusWorkspaceInputDto,
            MonitoringCapabilityCatalogDto,
        },
        EmptyInputDto,
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
        let input = ChannelStatusWorkspaceInputDto::parse(input)?;
        facade
            .load_channel_status_workspace(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_executions(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorExecutionPageDto, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_executions", async {
        let input = ChannelMonitorExecutionListInputDto::parse(input)?;
        facade
            .list_channel_monitor_executions(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_channel_monitor_execution(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorExecutionDetailDto, error::CommandError> {
    correlation::in_command_scope("get_channel_monitor_execution", async {
        let input = ChannelMonitorExecutionIdInputDto::parse(input)?;
        facade
            .get_channel_monitor_execution(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_attempts(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorAttemptPageDto, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_attempts", async {
        let input = ChannelMonitorAttemptHistoryInputDto::parse(input)?;
        facade
            .list_channel_monitor_attempts(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_monitoring_capabilities(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,
) -> Result<MonitoringCapabilityCatalogDto, error::CommandError> {
    correlation::in_command_scope("list_monitoring_capabilities", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_monitoring_capabilities()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
