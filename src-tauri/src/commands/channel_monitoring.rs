use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::ChannelMonitoringCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::{
        channel_monitor_mutations::{
            ChannelMonitorMutationIdInputDto, CreateChannelMonitorInputDto,
            CreateChannelMonitorTemplateInputDto, UpdateChannelMonitorInputDto,
            UpdateChannelMonitorTemplateInputDto,
        },
        channel_monitor_operations::{
            CancelChannelMonitorExecutionInputDto, CancelChannelMonitorExecutionReceiptDto,
        },
        channel_monitor_reads::{
            ChannelMonitorDto, ChannelMonitorIdInputDto, ChannelMonitorRequestTemplateDto,
            ChannelMonitorRunDto, RunChannelMonitorNowInputDto, RunChannelMonitorReceiptDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_channel_monitors(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitors", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_channel_monitors(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn create_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorDto, error::CommandError> {
    correlation::in_command_scope("create_channel_monitor", async {
        let input = CreateChannelMonitorInputDto::parse(input)?.into_domain();
        facade
            .create_channel_monitor(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorDto, error::CommandError> {
    correlation::in_command_scope("update_channel_monitor", async {
        let input = UpdateChannelMonitorInputDto::parse(input)?.into_domain();
        facade
            .update_channel_monitor(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_channel_monitor", async {
        let input = ChannelMonitorMutationIdInputDto::parse(input)?;
        facade
            .delete_channel_monitor(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_runs(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorRunDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_runs", async {
        let input = ChannelMonitorIdInputDto::parse(input)?;
        facade
            .list_channel_monitor_runs(
                &input.monitor_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_templates(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorRequestTemplateDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_templates", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_channel_monitor_templates(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn create_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope("create_channel_monitor_template", async {
        let input = CreateChannelMonitorTemplateInputDto::parse(input)?.into_domain();
        facade
            .create_channel_monitor_template(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope("update_channel_monitor_template", async {
        let input = UpdateChannelMonitorTemplateInputDto::parse(input)?.into_domain();
        facade
            .update_channel_monitor_template(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn duplicate_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope("duplicate_channel_monitor_template", async {
        let input = ChannelMonitorMutationIdInputDto::parse(input)?;
        facade
            .duplicate_channel_monitor_template(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_channel_monitor_template", async {
        let input = ChannelMonitorMutationIdInputDto::parse(input)?;
        facade
            .delete_channel_monitor_template(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn run_channel_monitor_now(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<RunChannelMonitorReceiptDto, error::CommandError> {
    correlation::in_command_scope("run_channel_monitor_now", async {
        let input = RunChannelMonitorNowInputDto::parse(input)?;
        facade
            .run_channel_monitor_now(input.monitor_id, input.trigger_request_id)
            .await
            .map_err(public_channel_monitor_run_error)
    })
    .await
}

#[tauri::command]
pub async fn cancel_channel_monitor_execution(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<CancelChannelMonitorExecutionReceiptDto, error::CommandError> {
    correlation::in_command_scope("cancel_channel_monitor_execution", async {
        let input = CancelChannelMonitorExecutionInputDto::parse(input)?;
        facade
            .cancel_channel_monitor_execution(input.execution_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

pub(crate) fn public_channel_monitor_run_error(_: String) -> error::CommandError {
    error::CommandError::from_work(error::WorkFailure::ResultUnknown)
}
