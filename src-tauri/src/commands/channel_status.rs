use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::ChannelStatusCommandFacade,
    commands::error,
    ipc::dto::{
        channel_monitor_operations::ChannelStatusWorkspaceDto,
        channel_monitor_reads::{
            ChannelMonitorAttemptHistoryInputDto, ChannelMonitorAttemptPageDto,
            ChannelMonitorExecutionDetailDto, ChannelMonitorExecutionIdInputDto,
            ChannelMonitorExecutionListInputDto, ChannelMonitorExecutionPageDto,
            ChannelStatusWorkspaceInputDto, MonitoringCapabilityCatalogDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn load_channel_status_workspace(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelStatusWorkspaceDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_channel_status_workspace",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelStatusWorkspaceInputDto::parse(input)?;
            facade
                .load_channel_status_workspace(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_executions(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorExecutionPageDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_channel_monitor_executions",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelMonitorExecutionListInputDto::parse(input)?;
            facade
                .list_channel_monitor_executions(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_channel_monitor_execution(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorExecutionDetailDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_channel_monitor_execution",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelMonitorExecutionIdInputDto::parse(input)?;
            facade
                .get_channel_monitor_execution(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_attempts(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ChannelMonitorAttemptPageDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_channel_monitor_attempts",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ChannelMonitorAttemptHistoryInputDto::parse(input)?;
            facade
                .list_channel_monitor_attempts(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_monitoring_capabilities(
    facade: State<'_, ChannelStatusCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<MonitoringCapabilityCatalogDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_monitoring_capabilities",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_monitoring_capabilities()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
