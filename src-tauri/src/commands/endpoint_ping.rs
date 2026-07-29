use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::RoutingCommandFacade,
    commands::error,
    ipc::dto::{routing_mutations::EndpointPingResultDto, station_keys::StationIdInputDto},
    observability::correlation,
};

#[tauri::command]
pub async fn ping_station_endpoint(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<EndpointPingResultDto, error::CommandError> {
    correlation::in_command_scope("ping_station_endpoint", async {
        let input = StationIdInputDto::parse(input)?;
        let result = facade
            .ping_station_endpoint(input.station_id)
            .await
            .map_err(super::public_endpoint_ping_error)?;
        EndpointPingResultDto::try_from(result)
            .map_err(|_| error::CommandError::from_work(error::WorkFailure::ResultUnknown))
    })
    .await
}
