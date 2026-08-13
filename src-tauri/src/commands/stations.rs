use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::SettingsStationsCommandFacade,
    commands::error,
    ipc::dto::{
        stations::{
            ClearStationCapacityDomainInputDto, CreateStationInputDto, DeleteStationInputDto,
            ReorderStationsInputDto, StationCapacityDomainDto, StationCapacityDomainQueryInputDto,
            UpdateStationInputDto, UpsertStationCapacityDomainInputDto,
        },
        EmptyInputDto, StationDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_stations(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<Vec<StationDto>, error::CommandError> {
    correlation::in_command_scope("list_stations", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_stations()
            .await
            .map(|stations| stations.into_iter().map(StationDto::from).collect())
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_station_capacity_domain(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<Option<StationCapacityDomainDto>, error::CommandError> {
    correlation::in_command_scope("get_station_capacity_domain", async {
        let input = StationCapacityDomainQueryInputDto::parse(input)?;
        facade
            .get_station_capacity_domain(input.station_id)
            .await
            .map(|value| value.map(StationCapacityDomainDto::from))
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_station_capacity_domain(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<StationCapacityDomainDto, error::CommandError> {
    correlation::in_command_scope("upsert_station_capacity_domain", async {
        let input = UpsertStationCapacityDomainInputDto::parse(input)?.into_domain()?;
        facade
            .upsert_station_capacity_domain(input)
            .await
            .map(StationCapacityDomainDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_station_capacity_domain(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("clear_station_capacity_domain", async {
        let input = ClearStationCapacityDomainInputDto::parse(input)?.into_domain();
        facade
            .clear_station_capacity_domain(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn create_station(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<StationDto, error::CommandError> {
    correlation::in_command_scope("create_station", async {
        let input = CreateStationInputDto::parse(input)?.into_domain()?;
        facade
            .create_station(input)
            .await
            .map(StationDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<StationDto, error::CommandError> {
    correlation::in_command_scope("update_station", async {
        let input = UpdateStationInputDto::parse(input)?.into_domain()?;
        facade
            .update_station(input)
            .await
            .map(StationDto::from)
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_station(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_station", async {
        let input = DeleteStationInputDto::parse(input)?;
        facade
            .delete_station(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reorder_stations(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<Vec<StationDto>, error::CommandError> {
    correlation::in_command_scope("reorder_stations", async {
        let input = ReorderStationsInputDto::parse(input)?;
        facade
            .reorder_stations(input.station_ids)
            .await
            .map(|stations| stations.into_iter().map(StationDto::from).collect())
            .map_err(super::public_command_application_error)
    })
    .await
}
