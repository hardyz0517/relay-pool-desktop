use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::DashboardMetricsCommandFacade,
    commands::error,
    ipc::dto::{
        dashboard_reads::{
            DashboardCumulativeRequestMetricsSnapshotDto, DashboardLiveRequestMetricsSnapshotDto,
            DashboardRequestMetricsInputDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn load_dashboard_live_request_metrics(
    facade: State<'_, DashboardMetricsCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<DashboardLiveRequestMetricsSnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_dashboard_live_request_metrics",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = DashboardRequestMetricsInputDto::parse(input)?.into_domain();
            facade
                .load_live(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn load_dashboard_cumulative_request_metrics(
    facade: State<'_, DashboardMetricsCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<DashboardCumulativeRequestMetricsSnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_dashboard_cumulative_request_metrics",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .load_cumulative()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
