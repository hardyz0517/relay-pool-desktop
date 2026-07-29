use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::RoutingCommandFacade,
    commands::error,
    ipc::dto::{
        routing_health_reads::{
            RouteSimulationInputDto, RouteSimulationResultDto, RoutingStationKeyIdInputDto,
            StationEndpointHealthDto, StationKeyHealthDto,
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
