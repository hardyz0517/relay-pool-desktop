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
) -> Result<DashboardLiveRequestMetricsSnapshotDto, error::CommandError> {
    correlation::in_command_scope("load_dashboard_live_request_metrics", async {
        let input = DashboardRequestMetricsInputDto::parse(input)?.into_domain();
        facade
            .load_live(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_dashboard_cumulative_request_metrics(
    facade: State<'_, DashboardMetricsCommandFacade>,
    input: Value,
) -> Result<DashboardCumulativeRequestMetricsSnapshotDto, error::CommandError> {
    correlation::in_command_scope("load_dashboard_cumulative_request_metrics", async {
        EmptyInputDto::parse(input)?;
        facade
            .load_cumulative()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
