use serde_json::Value;
use tauri::State;

use crate::{
    application::{command_facades::CollectorMetadataCommandFacade, pagination::PageLimit},
    commands::error,
    ipc::dto::collector_facts::{
        CollectorRunDto, CollectorSnapshotDto, CollectorStationIdInputDto,
        CollectorStationIdsInputDto, GroupRateRecordDto, StationGroupBindingDto,
        StationGroupOptionDto, UpsertStationGroupBindingInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_station_group_bindings(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Vec<StationGroupBindingDto>, error::CommandError> {
    correlation::in_command_scope("list_station_group_bindings", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .list_station_group_bindings(&input.station_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_group_options(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Vec<StationGroupOptionDto>, error::CommandError> {
    correlation::in_command_scope("list_station_group_options", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .list_station_group_options(
                &input.station_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_station_group_binding(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<StationGroupBindingDto, error::CommandError> {
    correlation::in_command_scope("upsert_station_group_binding", async {
        let input = UpsertStationGroupBindingInputDto::parse(input)?.into_domain();
        facade
            .upsert_station_group_binding(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_group_rate_records(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Vec<GroupRateRecordDto>, error::CommandError> {
    correlation::in_command_scope("list_group_rate_records", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .list_group_rate_records(
                &input.station_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_collector_runs(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Vec<CollectorRunDto>, error::CommandError> {
    correlation::in_command_scope("list_collector_runs", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .list_collector_runs(
                &input.station_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_collector_snapshots(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Vec<CollectorSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_collector_snapshots", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        let limit = PageLimit::new(100).map_err(super::public_command_application_error)?;
        facade
            .list_collector_snapshots(&input.station_id, limit)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_latest_collector_snapshot(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Option<CollectorSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("get_latest_collector_snapshot", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .get_latest_collector_snapshot(&input.station_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_latest_collector_snapshots(
    facade: State<'_, CollectorMetadataCommandFacade>,
    input: Value,
) -> Result<Vec<CollectorSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_latest_collector_snapshots", async {
        let input = CollectorStationIdsInputDto::parse(input)?;
        facade
            .list_latest_collector_snapshots(input.station_ids)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
