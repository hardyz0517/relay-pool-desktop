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
        routing_mutations::{RoutingPolicySnapshotDto, UpdateRoutingPolicyInputDto},
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_station_key_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<StationKeyHealthDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_station_key_health",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_station_key_health()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn load_routing_policy(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingPolicySnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_routing_policy",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            crate::ipc::dto::EmptyInputDto::parse(input)?;
            let stored = facade
                .load_routing_policy()
                .await
                .map_err(super::public_command_application_error)?;
            let config: crate::models::routing_policy::RoutingPolicyConfigV1 =
                serde_json::from_value(stored.config)
                    .map_err(|_| error::CommandError::internal(None))?;
            Ok(RoutingPolicySnapshotDto {
                config: config.into(),
                revision: stored.revision,
                policy_version: stored.policy_version,
                system_version: stored.system_version,
                status: stored.status,
                updated_at_ms: stored.updated_at_ms,
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn update_routing_policy(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingPolicySnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "update_routing_policy",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpdateRoutingPolicyInputDto::parse(input)?;
            let config = input.config.into_domain()?;
            let stored = facade
                .save_routing_policy(config, input.expected_revision)
                .await
                .map_err(super::public_command_application_error)?;
            let config: crate::models::routing_policy::RoutingPolicyConfigV1 =
                serde_json::from_value(stored.config)
                    .map_err(|_| error::CommandError::internal(None))?;
            Ok(RoutingPolicySnapshotDto {
                config: config.into(),
                revision: stored.revision,
                policy_version: stored.policy_version,
                system_version: stored.system_version,
                status: stored.status,
                updated_at_ms: stored.updated_at_ms,
            })
        },
    )
    .await
}

#[tauri::command]
pub async fn load_routing_workspace_snapshot(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingWorkspaceSnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_routing_workspace_snapshot",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RoutingWorkspaceSnapshotInputDto::parse(input)?.into_domain();
            facade
                .load_routing_workspace_snapshot(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn load_routing_runtime_overlay(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingRuntimeOverlayDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "load_routing_runtime_overlay",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .load_routing_runtime_overlay()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_recent_route_decisions(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RecentRouteDecisionsPageDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_recent_route_decisions",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RecentRouteDecisionsInputDto::parse(input)?.into_domain();
            facade
                .list_recent_route_decisions(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_station_key_operational_detail(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyOperationalDetailDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_station_key_operational_detail",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = StationKeyOperationalDetailInputDto::parse(input)?.into_domain();
            facade
                .get_station_key_operational_detail(input.station_key_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_request_decision_trace(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RequestDecisionTraceDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_request_decision_trace",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RequestDecisionTraceInputDto::parse(input)?;
            facade
                .get_request_decision_trace(input.request_log_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_station_endpoint_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<StationEndpointHealthDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_station_endpoint_health",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_station_endpoint_health()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_station_key_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<StationKeyHealthDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_station_key_health",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RoutingStationKeyIdInputDto::parse(input)?;
            facade
                .get_station_key_health(input.station_key_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn simulate_route(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RouteSimulationResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "simulate_route",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RouteSimulationInputDto::parse(input)?.into_domain()?;
            facade
                .simulate_route(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
