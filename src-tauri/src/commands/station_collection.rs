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

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "detect_sub2api_station",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .run_station_collection(input.station_id, collectors::output::CollectorTask::Detect)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn collect_sub2api_station(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "collect_sub2api_station",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .run_station_collection(input.station_id, collectors::output::CollectorTask::Full)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn detect_station_info(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "detect_station_info",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .run_station_collection(input.station_id, collectors::output::CollectorTask::Detect)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn collect_station_info(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "collect_station_info",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .run_station_collection(input.station_id, collectors::output::CollectorTask::Full)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn collect_station_task(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "collect_station_task",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationCollectorTaskInputDto::parse(input)?;
            let task = match input.task_type {
                StationCollectorTaskTypeDto::Detect => collectors::output::CollectorTask::Detect,
                StationCollectorTaskTypeDto::Balance => collectors::output::CollectorTask::Balance,
                StationCollectorTaskTypeDto::Groups => collectors::output::CollectorTask::Groups,
                StationCollectorTaskTypeDto::PublishedStatus => {
                    collectors::output::CollectorTask::PublishedStatus
                }
                StationCollectorTaskTypeDto::Full => collectors::output::CollectorTask::Full,
            };
            facade
                .run_station_collection(input.station_id, task)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn scan_station_recharge(
    app: tauri::AppHandle,
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "scan_station_recharge",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .scan_station_recharge(&app, input.station_id)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn test_station_login(
    facade: State<'_, StationCollectionCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "test_station_login",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .test_station_login(input.station_id)
                .await
                .map_err(public_station_collection_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn test_station_login_input(
    runtime: State<'_, ManagedWorkRuntime>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationLoginTestResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "test_station_login_input",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationLoginTestInputDto::parse(input)?.into_domain();
            collectors::test_station_login_input_async(
                &runtime.outbound,
                input,
                tokio_util::sync::CancellationToken::new(),
                super::current_correlation_id(),
            )
            .await
            .map_err(public_station_login_probe_error)
        },
    )
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
        StationCollectionCommandError::Admission(admission) => match admission {
            crate::services::station_collection_coordinator::StationCollectionAdmissionError::AlreadyRunning => {
                error::CommandError::try_new(
                    error::CommandErrorCode::Conflict,
                    "A collection for this station is already running.",
                    true,
                    None,
                    None,
                )
                .unwrap_or_else(|_| error::CommandError::internal(None))
            }
            crate::services::station_collection_coordinator::StationCollectionAdmissionError::AtCapacity => {
                error::CommandError::from_work(error::WorkFailure::Overloaded)
            }
            crate::services::station_collection_coordinator::StationCollectionAdmissionError::Cancelled
            | crate::services::station_collection_coordinator::StationCollectionAdmissionError::InvalidStationId => {
                error::CommandError::internal(None)
            }
        },
        StationCollectionCommandError::Prepare(error) => {
            super::public_command_application_error(error)
        }
        StationCollectionCommandError::Apply(error) => super::command_application_error(error),
        StationCollectionCommandError::Blocking(error) => {
            super::public_blocking_executor_error(error)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        commands::error::CommandErrorCode,
        services::station_collection_coordinator::StationCollectionAdmissionError,
    };

    #[test]
    fn station_collection_admission_errors_have_stable_public_contracts() {
        let conflict = public_station_collection_error(StationCollectionCommandError::Admission(
            StationCollectionAdmissionError::AlreadyRunning,
        ));
        assert_eq!(conflict.code, CommandErrorCode::Conflict);
        assert!(conflict.retryable);
        assert_eq!(
            conflict.message,
            "A collection for this station is already running."
        );
        assert!(conflict.details.is_none());

        let overloaded = public_station_collection_error(StationCollectionCommandError::Admission(
            StationCollectionAdmissionError::AtCapacity,
        ));
        assert_eq!(overloaded.code, CommandErrorCode::Overloaded);
        assert!(overloaded.retryable);
    }
}
