use serde_json::Value;
use tauri::State;

use crate::{
    application::app_services::AppServices,
    commands::error,
    ipc::dto::station_published_status::{
        StationPublishedStatusWorkspaceDto, StationPublishedStatusWorkspaceInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn get_station_published_status_workspace(
    services: State<'_, AppServices>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationPublishedStatusWorkspaceDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_station_published_status_workspace",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationPublishedStatusWorkspaceInputDto::parse(input)?;
            let settings = services
                .settings
                .load()
                .await
                .map_err(super::public_command_application_error)?;
            services
                .station_published_status
                .load_workspace(
                    &input.station_id,
                    settings.published_status_interval_minutes,
                )
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
