use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::RoutingCommandFacade,
    commands::error,
    ipc::dto::{
        routing_health_reads::{
            ProxyTimeoutFactsDto, RecentRouteDecisionsInputDto, RecentRouteDecisionsPageDto,
            RequestDecisionTraceDto, RequestDecisionTraceInputDto, RouteSimulationInputDto,
            RouteSimulationResultDto, RoutingCircuitStatusDto, RoutingProtectionStatusDto,
            RoutingProtectionStatusInputDto, RoutingRuntimeOverlayDto, RoutingWorkspaceSnapshotDto,
            RoutingWorkspaceSnapshotInputDto, StationEndpointHealthDto,
        },
        routing_mutations::{
            ApplyRoutingPolicyDocumentInputDto, RoutingDocumentSyncDto,
            RoutingPolicyActivationPathDto, RoutingPolicyFallbackReasonDto,
            RoutingPolicyPublicationFailureCodeDto, RoutingPolicyPublicationStateDto,
            RoutingPolicyPublicationStatusDto, RoutingPolicyPublicationStatusInputDto,
            RoutingPolicySnapshotDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

fn routing_policy_snapshot(
    stored: crate::persistence::stores::routing_policy_store::StoredRoutingPolicy,
    document_sync: Option<crate::persistence::stores::document_sync_store::StoredDocumentSync>,
    activation_path: Option<RoutingPolicyActivationPathDto>,
    runtime_status: Option<RoutingPolicyPublicationStateDto>,
    active_revision: Option<u64>,
    fallback_reason: Option<RoutingPolicyFallbackReasonDto>,
) -> Result<RoutingPolicySnapshotDto, error::CommandError> {
    let config = crate::application::routing::routing_policy_v3_from_stored(&stored.config)
        .map_err(|_| error::CommandError::internal(None))?;
    let runtime_status = match runtime_status {
        Some(status) => Some(status),
        None => Some(
            RoutingPolicyPublicationStateDto::from_internal_code(&stored.status)
                .ok_or_else(|| error::CommandError::internal(None))?,
        ),
    };
    Ok(RoutingPolicySnapshotDto {
        config: config.into(),
        revision: stored.revision,
        policy_version: stored.policy_version,
        system_version: stored.system_version,
        status: stored.status,
        activation_path,
        runtime_status,
        active_revision,
        fallback_reason,
        updated_at_ms: stored.updated_at_ms,
        document_sync: document_sync.map(RoutingDocumentSyncDto::from),
    })
}

#[tauri::command]
pub async fn get_routing_protection_status(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingProtectionStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_routing_protection_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            RoutingProtectionStatusInputDto::parse(input)?;
            facade
                .get_routing_protection_status()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_routing_circuit_status(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingCircuitStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_routing_circuit_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .get_routing_circuit_status()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_proxy_timeout_facts(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ProxyTimeoutFactsDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_proxy_timeout_facts",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            Ok(facade.get_proxy_timeout_facts())
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
            let document_sync = facade
                .load_routing_policy_document_sync()
                .await
                .map_err(super::public_command_application_error)?;
            let publication = facade
                .load_routing_policy_publication(stored.revision, None)
                .await
                .map_err(super::public_command_application_error)?;
            let activation_path = publication
                .activation_path
                .map(|value| {
                    RoutingPolicyActivationPathDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))
                })
                .transpose()?;
            let runtime_status = publication
                .runtime_status
                .map(|value| {
                    RoutingPolicyPublicationStateDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))
                })
                .transpose()?;
            let fallback_reason = publication
                .fallback_reason
                .map(|value| {
                    RoutingPolicyFallbackReasonDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))
                })
                .transpose()?;
            routing_policy_snapshot(
                stored,
                document_sync,
                activation_path,
                runtime_status,
                publication.active_revision,
                fallback_reason,
            )
        },
    )
    .await
}

#[tauri::command]
pub async fn get_routing_policy_publication_status(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingPolicyPublicationStatusDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_routing_policy_publication_status",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = RoutingPolicyPublicationStatusInputDto::parse(input)?;
            let publication = facade
                .load_routing_policy_publication(
                    input.revision,
                    input.policy_generation_id.as_deref(),
                )
                .await
                .map_err(super::public_command_application_error)?;
            let status =
                RoutingPolicyPublicationStateDto::from_internal_code(publication.status.as_str())
                    .ok_or_else(|| error::CommandError::internal(None))?;
            let activation_path = match publication.activation_path {
                Some(value) => Some(
                    RoutingPolicyActivationPathDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))?,
                ),
                None => None,
            };
            let runtime_status = match publication.runtime_status {
                Some(value) => Some(
                    RoutingPolicyPublicationStateDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))?,
                ),
                None => None,
            };
            let fallback_reason = match publication.fallback_reason {
                Some(value) => Some(
                    RoutingPolicyFallbackReasonDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))?,
                ),
                None => None,
            };
            let failure_code = match publication.failure_code {
                Some(value) => Some(
                    RoutingPolicyPublicationFailureCodeDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))?,
                ),
                None => None,
            };
            Ok(RoutingPolicyPublicationStatusDto {
                revision: publication.revision,
                policy_generation_id: publication.policy_generation_id,
                status,
                failure_code,
                activation_path,
                runtime_status,
                active_revision: publication.active_revision,
                fallback_reason,
                updated_at_ms: publication.updated_at_ms,
                terminal: publication.terminal,
            })
        },
    )
    .await
}

/// Apply a complete routing-policy document. The command keeps source
/// provenance internal and uses `document.baseRevision` as the sole CAS
/// precondition.
#[tauri::command]
pub async fn apply_routing_policy_document(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<RoutingPolicySnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "apply_routing_policy_document",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ApplyRoutingPolicyDocumentInputDto::parse(input)?;
            let document = input.into_domain()?;
            let stored = facade
                .apply_routing_policy_document_v3(document)
                .await
                .map_err(super::public_command_application_error)?;
            let document_sync = facade
                .load_routing_policy_document_sync()
                .await
                .map_err(super::public_command_application_error)?;
            let publication = facade
                .load_routing_policy_publication(stored.revision, None)
                .await
                .map_err(super::public_command_application_error)?;
            let activation_path = publication
                .activation_path
                .map(|value| {
                    RoutingPolicyActivationPathDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))
                })
                .transpose()?
                .or_else(|| {
                    (stored.status == "active").then_some(RoutingPolicyActivationPathDto::Fast)
                });
            let runtime_status = publication
                .runtime_status
                .map(|value| {
                    RoutingPolicyPublicationStateDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))
                })
                .transpose()?;
            let fallback_reason = publication
                .fallback_reason
                .map(|value| {
                    RoutingPolicyFallbackReasonDto::from_internal_code(value)
                        .ok_or_else(|| error::CommandError::internal(None))
                })
                .transpose()?;
            routing_policy_snapshot(
                stored,
                document_sync,
                activation_path,
                runtime_status,
                publication.active_revision,
                fallback_reason,
            )
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
