use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::RoutingCommandFacade,
    commands::error,
    ipc::dto::{
        routing_health_reads::{
            RecentRouteDecisionsInputDto, RecentRouteDecisionsPageDto, RequestDecisionTraceDto,
            RequestDecisionTraceInputDto, RouteSimulationInputDto, RouteSimulationResultDto,
            RoutingRuntimeOverlayDto, RoutingStationKeyIdInputDto, RoutingWorkspaceSnapshotDto,
            RoutingWorkspaceSnapshotInputDto, StationEndpointHealthDto, StationKeyHealthDto,
            StationKeyOperationalDetailDto, StationKeyOperationalDetailInputDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_station_key_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<Vec<StationKeyHealthDto>, error::CommandError> {
    correlation::in_command_scope("list_station_key_health", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_station_key_health()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_routing_workspace_snapshot(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<RoutingWorkspaceSnapshotDto, error::CommandError> {
    correlation::in_command_scope("load_routing_workspace_snapshot", async {
        let input = RoutingWorkspaceSnapshotInputDto::parse(input)?.into_domain();
        facade
            .load_routing_workspace_snapshot(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_routing_runtime_overlay(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<RoutingRuntimeOverlayDto, error::CommandError> {
    correlation::in_command_scope("load_routing_runtime_overlay", async {
        EmptyInputDto::parse(input)?;
        facade
            .load_routing_runtime_overlay()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_recent_route_decisions(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<RecentRouteDecisionsPageDto, error::CommandError> {
    correlation::in_command_scope("list_recent_route_decisions", async {
        let input = RecentRouteDecisionsInputDto::parse(input)?.into_domain();
        facade
            .list_recent_route_decisions(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_station_key_operational_detail(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<StationKeyOperationalDetailDto, error::CommandError> {
    correlation::in_command_scope("get_station_key_operational_detail", async {
        let input = StationKeyOperationalDetailInputDto::parse(input)?.into_domain();
        facade
            .get_station_key_operational_detail(input.station_key_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_request_decision_trace(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<RequestDecisionTraceDto, error::CommandError> {
    correlation::in_command_scope("get_request_decision_trace", async {
        let input = RequestDecisionTraceInputDto::parse(input)?;
        facade
            .get_request_decision_trace(input.request_log_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_endpoint_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<Vec<StationEndpointHealthDto>, error::CommandError> {
    correlation::in_command_scope("list_station_endpoint_health", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_station_endpoint_health()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_station_key_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<StationKeyHealthDto, error::CommandError> {
    correlation::in_command_scope("get_station_key_health", async {
        let input = RoutingStationKeyIdInputDto::parse(input)?;
        facade
            .get_station_key_health(input.station_key_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn simulate_route(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<RouteSimulationResultDto, error::CommandError> {
    correlation::in_command_scope("simulate_route", async {
        let input = RouteSimulationInputDto::parse(input)?.into_domain();
        facade
            .simulate_route(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
