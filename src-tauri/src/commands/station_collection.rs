use serde_json::Value;
use tauri::State;

use crate::{
    app_composition::ManagedWorkRuntime,
    application::command_facades::{StationCollectionCommandError, StationCollectionCommandFacade},
    commands::error,
    ipc::dto::{
        collector_facts::CollectorStationIdInputDto,
        station_collector_operations::{
            CollectorRunResultDto, StationCollectorTaskInputDto, StationCollectorTaskTypeDto,
            StationLoginTestInputDto, StationLoginTestResultDto,
        },
    },
    observability::correlation,
    services::collectors,
};

#[tauri::command]
pub async fn detect_sub2api_station(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("detect_sub2api_station", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .run_station_collection(
                input.station_id,
                collectors::adapters::CollectorTask::Detect,
            )
            .await
            .map_err(public_station_collection_error)
    })
    .await
}

#[tauri::command]
pub async fn collect_sub2api_station(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("collect_sub2api_station", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .run_station_collection(input.station_id, collectors::adapters::CollectorTask::Full)
            .await
            .map_err(public_station_collection_error)
    })
    .await
}

#[tauri::command]
pub async fn detect_station_info(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("detect_station_info", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .run_station_collection(
                input.station_id,
                collectors::adapters::CollectorTask::Detect,
            )
            .await
            .map_err(public_station_collection_error)
    })
    .await
}

#[tauri::command]
pub async fn collect_station_info(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("collect_station_info", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .run_station_collection(input.station_id, collectors::adapters::CollectorTask::Full)
            .await
            .map_err(public_station_collection_error)
    })
    .await
}

#[tauri::command]
pub async fn collect_station_task(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("collect_station_task", async {
        let input = StationCollectorTaskInputDto::parse(input)?;
        let task = match input.task_type {
            StationCollectorTaskTypeDto::Detect => collectors::adapters::CollectorTask::Detect,
            StationCollectorTaskTypeDto::Balance => collectors::adapters::CollectorTask::Balance,
            StationCollectorTaskTypeDto::Groups => collectors::adapters::CollectorTask::Groups,
            StationCollectorTaskTypeDto::Models => collectors::adapters::CollectorTask::Models,
            StationCollectorTaskTypeDto::Full => collectors::adapters::CollectorTask::Full,
        };
        facade
            .run_station_collection(input.station_id, task)
            .await
            .map_err(public_station_collection_error)
    })
    .await
}

#[tauri::command]
pub async fn test_station_login(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("test_station_login", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .test_station_login(input.station_id)
            .await
            .map_err(public_station_collection_error)
    })
    .await
}

#[tauri::command]
pub async fn test_station_login_input(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,
) -> Result<StationLoginTestResultDto, error::CommandError> {
    correlation::in_command_scope("test_station_login_input", async {
        let input = StationLoginTestInputDto::parse(input)?.into_domain();
        collectors::test_station_login_input_async(
            &runtime.outbound,
            input,
            tokio_util::sync::CancellationToken::new(),
            super::current_correlation_id(),
        )
        .await
        .map_err(public_station_login_probe_error)
    })
    .await
}

fn public_station_login_probe_error(_: String) -> error::CommandError {
    error::CommandError::from_driver(error::DriverFailure::ExternalUnavailable {
        provider: None,
        upstream_status: None,
    })
}

fn public_station_collection_error(error: StationCollectionCommandError) -> error::CommandError {
    match error {
        StationCollectionCommandError::Prepare(error) => {
            super::public_command_application_error(error)
        }
        StationCollectionCommandError::Apply(error) => super::command_application_error(error),
        StationCollectionCommandError::Blocking(error) => {
            super::public_blocking_executor_error(error)
        }
    }
}
