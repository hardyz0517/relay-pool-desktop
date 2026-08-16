use serde_json::Value;
use tauri::State;

use crate::{
    application::command_facades::RoutingCommandFacade,
    commands::error,
    ipc::dto::{
        routing_health_reads::ModelAliasDto,
        routing_mutations::{DeleteModelAliasInputDto, UpsertModelAliasInputDto},
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_model_aliases(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<ModelAliasDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_model_aliases",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_model_aliases()
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn upsert_model_alias(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ModelAliasDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "upsert_model_alias",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpsertModelAliasInputDto::parse(input)?.into_domain();
            facade
                .upsert_model_alias(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_model_alias(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "delete_model_alias",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = DeleteModelAliasInputDto::parse(input)?;
            facade
                .delete_model_alias(input.id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
