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
) -> Result<Vec<ModelAliasDto>, error::CommandError> {
    correlation::in_command_scope("list_model_aliases", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_model_aliases()
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_model_alias(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<ModelAliasDto, error::CommandError> {
    correlation::in_command_scope("upsert_model_alias", async {
        let input = UpsertModelAliasInputDto::parse(input)?.into_domain();
        facade
            .upsert_model_alias(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_model_alias(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_model_alias", async {
        let input = DeleteModelAliasInputDto::parse(input)?;
        facade
            .delete_model_alias(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
