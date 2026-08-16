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
            ChannelMonitorDto, ChannelMonitorRequestTemplateDto, RunChannelMonitorNowInputDto,
            RunChannelMonitorReceiptDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_channel_monitors(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<ChannelMonitorDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_channel_monitors",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_channel_monitors(PageLimit::new(200).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn create_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "create_channel_monitor",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CreateChannelMonitorInputDto::parse(input)?.into_domain();
            facade
                .create_channel_monitor(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn update_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "update_channel_monitor",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpdateChannelMonitorInputDto::parse(input)?.into_domain();
            facade
                .update_channel_monitor(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "delete_channel_monitor",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelMonitorMutationIdInputDto::parse(input)?;
            facade
                .delete_channel_monitor(input.id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_templates(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<ChannelMonitorRequestTemplateDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_channel_monitor_templates",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_channel_monitor_templates(PageLimit::new(200).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn create_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "create_channel_monitor_template",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CreateChannelMonitorTemplateInputDto::parse(input)?.into_domain();
            facade
                .create_channel_monitor_template(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn update_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "update_channel_monitor_template",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpdateChannelMonitorTemplateInputDto::parse(input)?.into_domain();
            facade
                .update_channel_monitor_template(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn duplicate_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "duplicate_channel_monitor_template",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelMonitorMutationIdInputDto::parse(input)?;
            facade
                .duplicate_channel_monitor_template(input.id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "delete_channel_monitor_template",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelMonitorMutationIdInputDto::parse(input)?;
            facade
                .delete_channel_monitor_template(input.id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn run_channel_monitor_now(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RunChannelMonitorReceiptDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "run_channel_monitor_now",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RunChannelMonitorNowInputDto::parse(input)?;
            facade
                .run_channel_monitor_now(input.monitor_id, input.trigger_request_id)
                .await
                .map_err(public_channel_monitor_run_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn cancel_channel_monitor_execution(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CancelChannelMonitorExecutionReceiptDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "cancel_channel_monitor_execution",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CancelChannelMonitorExecutionInputDto::parse(input)?;
            facade
                .cancel_channel_monitor_execution(input.execution_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

pub(crate) fn public_channel_monitor_run_error(_: String) -> error::CommandError {
    error::CommandError::from_work(error::WorkFailure::ResultUnknown)
}
