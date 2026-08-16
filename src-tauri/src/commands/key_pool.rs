use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::{KeyPoolCommandFacade, RemoteKeysCommandFacade},
    commands::error,
    ipc::dto::{
        routing_health_reads::{RoutingStationKeyIdInputDto, StationKeyCapabilitiesDto},
        routing_mutations::UpdateStationKeyCapabilitiesInputDto,
        station_keys::{
            BindRemoteStationKeyInputDto, CreateLocalStationKeyFromRemoteResultDto,
            CreateRemoteStationKeyInputDto, CreateRemoteStationKeyResultDto,
            CreateStationKeyInputDto, DeleteRemoteStationKeyResultDto, KeyPoolItemDto,
            RemoteKeyCapabilityDto, RemoteKeyScanResultDto, RemoteStationKeyDto,
            RemoteStationKeyInputDto, ReorderKeyPoolInputDto, ReorderStationKeysInputDto,
            SaveStationKeyWithDefaultsInputDto, SaveStationKeyWithDefaultsResultDto,
            StationIdInputDto, StationKeyDto, StationKeyIdInputDto,
            UpdateStationKeyGroupBindingInputDto, UpdateStationKeyInputDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
    services::remote_keys,
};

#[tauri::command]
pub async fn list_station_keys(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<StationKeyDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_station_keys",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationIdInputDto::parse(input)?;
            facade
                .list_station_keys(input.station_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn create_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "create_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CreateStationKeyInputDto::parse(input)?;
            facade
                .create_station_key(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn update_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "update_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpdateStationKeyInputDto::parse(input)?;
            facade
                .update_station_key(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn save_station_key_with_defaults(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<SaveStationKeyWithDefaultsResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "save_station_key_with_defaults",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = SaveStationKeyWithDefaultsInputDto::parse(input)?;
            facade
                .save_station_key_with_defaults(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn update_station_key_group_binding(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "update_station_key_group_binding",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpdateStationKeyGroupBindingInputDto::parse(input)?;
            facade
                .update_station_key_group_binding(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "delete_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationKeyIdInputDto::parse(input)?;
            facade
                .delete_station_key(input.id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn reorder_station_keys(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<StationKeyDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "reorder_station_keys",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ReorderStationKeysInputDto::parse(input)?;
            facade
                .reorder_station_keys(input.station_id, input.key_ids)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_remote_key_capability(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RemoteKeyCapabilityDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_remote_key_capability",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationIdInputDto::parse(input)?;
            facade
                .get_remote_key_capability(input.station_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_remote_station_keys(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<RemoteStationKeyDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_remote_station_keys",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationIdInputDto::parse(input)?;
            facade
                .list_remote_station_keys(input.station_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn scan_remote_station_keys(
    facade: State<'_, RemoteKeysCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RemoteKeyScanResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "scan_remote_station_keys",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationIdInputDto::parse(input)?;
            facade
                .scan_remote_station_keys(input.station_id)
                .await
                .map_err(public_remote_key_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn create_remote_station_key(
    facade: State<'_, RemoteKeysCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CreateRemoteStationKeyResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "create_remote_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CreateRemoteStationKeyInputDto::parse(input)?;
            facade
                .create_remote_station_key(input)
                .await
                .map_err(public_remote_key_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn create_local_station_key_from_remote(
    facade: State<'_, RemoteKeysCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<CreateLocalStationKeyFromRemoteResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "create_local_station_key_from_remote",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RemoteStationKeyInputDto::parse(input)?;
            facade
                .create_local_station_key_from_remote(input.station_id, input.remote_key_id)
                .await
                .map_err(public_remote_key_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_remote_station_key(
    facade: State<'_, RemoteKeysCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<DeleteRemoteStationKeyResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "delete_remote_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RemoteStationKeyInputDto::parse(input)?;
            facade
                .delete_remote_station_key(input.station_id, input.remote_key_id)
                .await
                .map_err(public_remote_key_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn bind_remote_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<RemoteStationKeyDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "bind_remote_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = BindRemoteStationKeyInputDto::parse(input)?;
            facade
                .bind_remote_station_key(input.remote_key_id, input.station_key_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn unbind_remote_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<RemoteStationKeyDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "unbind_remote_station_key",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RemoteStationKeyInputDto::parse(input)?;
            facade
                .unbind_remote_station_key(input.remote_key_id, input.station_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_key_pool_items(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<KeyPoolItemDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_key_pool_items",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_key_pool_items()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn reorder_key_pool(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<KeyPoolItemDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "reorder_key_pool",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ReorderKeyPoolInputDto::parse(input)?;
            facade
                .reorder_key_pool(input.key_ids)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_station_key_capabilities(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyCapabilitiesDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_station_key_capabilities",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RoutingStationKeyIdInputDto::parse(input)?;
            facade
                .get_station_key_capabilities(input.station_key_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn update_station_key_capabilities(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyCapabilitiesDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "update_station_key_capabilities",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpdateStationKeyCapabilitiesInputDto::parse(input)?.into_domain();
            facade
                .update_station_key_capabilities(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

pub(crate) fn public_remote_key_error(
    error: remote_keys::RemoteKeyOperationError,
) -> error::CommandError {
    match error {
        remote_keys::RemoteKeyOperationError::Application(error) => {
            super::public_command_application_error(error)
        }
        remote_keys::RemoteKeyOperationError::Unsupported => {
            error::CommandError::from_driver(error::DriverFailure::Unsupported)
        }
        remote_keys::RemoteKeyOperationError::UnsupportedWithDetail(detail) => {
            error::CommandError::unsupported_with_detail(detail)
        }
        remote_keys::RemoteKeyOperationError::ExternalUnavailable => {
            error::CommandError::from_driver(error::DriverFailure::ExternalUnavailable {
                provider: None,
                upstream_status: None,
            })
        }
        remote_keys::RemoteKeyOperationError::ResultUnknown => {
            error::CommandError::from_driver(error::DriverFailure::ResultUnknown)
        }
        remote_keys::RemoteKeyOperationError::Conflict => super::public_command_application_error(
            crate::application::error::ApplicationError::StaleRevision,
        ),
        remote_keys::RemoteKeyOperationError::Internal => error::CommandError::internal(None),
    }
}
