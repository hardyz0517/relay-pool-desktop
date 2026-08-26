use serde_json::Value;
use tauri::State;

use crate::{
    application::{
        command_facades::{PricingCommandFacade, RoutingCommandFacade},
        pagination::PageLimit,
    },
    commands::error,
    ipc::dto::{
        collector_facts::{
            BalanceSnapshotDto, CollectorStationIdInputDto, UpsertBalanceSnapshotInputDto,
        },
        pricing_mutations::{
            ModelBasePriceIdInputDto, SaveModelPriceSyncConfigInputDto, SyncModelPricesInputDto,
            UpsertModelBasePriceInputDto,
        },
        pricing_reads::{
            ModelBasePriceDto, ModelPriceCatalogEntryDto, ModelPriceSyncResultDto,
            ModelPriceSyncStateDto, PricingContextInputDto, ResolvedPricingContextDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
    services::model_price_sync::ModelPriceSyncError,
};

#[tauri::command]
pub async fn list_model_base_prices(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<ModelBasePriceDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_model_base_prices",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_model_base_prices(PageLimit::new(500).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn upsert_model_base_price(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ModelBasePriceDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "upsert_model_base_price",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpsertModelBasePriceInputDto::parse(input)?.into_domain();
            facade
                .upsert_model_base_price(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn delete_model_base_price(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "delete_model_base_price",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = ModelBasePriceIdInputDto::parse(input)?;
            facade
                .delete_model_base_price(input.id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn reset_model_base_prices_to_builtins(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<ModelBasePriceDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "reset_model_base_prices_to_builtins",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .reset_model_base_prices_to_builtins(PageLimit::new(500).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn get_model_price_sync_state(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ModelPriceSyncStateDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "get_model_price_sync_state",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .get_model_price_sync_state()
                .map(to_sync_state_dto)
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_model_price_sync_catalog(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<ModelPriceCatalogEntryDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_model_price_sync_catalog",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_model_price_sync_catalog()
                .map(|entries| entries.into_iter().map(to_catalog_entry_dto).collect())
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn save_model_price_sync_config(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ModelPriceSyncStateDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "save_model_price_sync_config",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = SaveModelPriceSyncConfigInputDto::parse(input)?.into_domain();
            facade
                .save_model_price_sync_config(input)
                .await
                .map(to_sync_state_dto)
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn sync_model_prices(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ModelPriceSyncResultDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "sync_model_prices",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = SyncModelPricesInputDto::parse(input)?;
            facade
                .sync_model_prices(input.force)
                .await
                .map(to_sync_result_dto)
                .map_err(model_price_sync_command_error)
        },
    )
    .await
}

fn model_price_sync_command_error(error: ModelPriceSyncError) -> error::CommandError {
    match error {
        ModelPriceSyncError::Application(error) => super::public_command_application_error(error),
        ModelPriceSyncError::ExternalUnavailable { upstream_status } => {
            error::CommandError::from_driver(error::DriverFailure::ExternalUnavailable {
                provider: Some("models.dev".into()),
                upstream_status,
            })
        }
    }
}

#[tauri::command]
pub async fn reload_model_price_catalog(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ModelPriceSyncStateDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "reload_model_price_catalog",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .reload_model_price_catalog()
                .await
                .map(to_sync_state_dto)
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn open_model_price_catalog_directory(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "open_model_price_catalog_directory",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .open_model_price_catalog_directory()
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

fn to_sync_state_dto(
    state: crate::services::model_price_sync::ModelPriceSyncState,
) -> ModelPriceSyncStateDto {
    ModelPriceSyncStateDto {
        source_url: state.source_url,
        auto_sync_enabled: state.auto_sync_enabled,
        include_common_models: state.include_common_models,
        selected_model_keys: state.selected_model_keys,
        excluded_common_model_keys: state.excluded_common_model_keys,
        last_sync_at: state.last_sync_at,
        last_sync_error: state.last_sync_error,
        model_count: state.model_count,
        common_model_count: state.common_model_count,
        auto_sync_model_count: state.auto_sync_model_count,
        file_path: state.file_path,
    }
}

fn to_catalog_entry_dto(
    entry: crate::services::model_price_sync::ModelPriceCatalogEntry,
) -> ModelPriceCatalogEntryDto {
    ModelPriceCatalogEntryDto {
        key: entry.key,
        provider: entry.provider,
        model: entry.model,
        name: entry.name,
        common: entry.common,
        release_date: entry.release_date,
        input_price: entry.input_price,
        output_price: entry.output_price,
        cache_creation_price: entry.cache_creation_price,
        cache_read_price: entry.cache_read_price,
    }
}

fn to_sync_result_dto(
    result: crate::services::model_price_sync::ModelPriceSyncResult,
) -> ModelPriceSyncResultDto {
    ModelPriceSyncResultDto {
        state: to_sync_state_dto(result.state),
        imported_count: result.imported_count,
        skipped_count: result.skipped_count,
    }
}

#[cfg(test)]
mod model_price_sync_error_tests {
    use super::*;

    #[test]
    fn external_sync_failure_is_not_reported_as_runtime_unavailable() {
        let command_error =
            model_price_sync_command_error(ModelPriceSyncError::ExternalUnavailable {
                upstream_status: Some(503),
            });

        assert_eq!(
            command_error.code,
            error::CommandErrorCode::ExternalUnavailable
        );
        assert!(command_error.retryable);
        assert!(!command_error.message.contains("desktop runtime"));
        assert!(matches!(
            command_error.details,
            Some(error::PublicErrorDetails::External {
                provider: Some(ref provider),
                upstream_status: Some(503),
            }) if provider == "models.dev"
        ));
    }
}

#[tauri::command]
pub async fn resolve_station_key_pricing_context(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<ResolvedPricingContextDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "resolve_station_key_pricing_context",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let (station_key_id, requested_model, request_kind) =
                PricingContextInputDto::parse(input)?.into_parts();
            facade
                .resolve_station_key_pricing_context(
                    &station_key_id,
                    &requested_model,
                    request_kind,
                )
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_balance_snapshots(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_balance_snapshots",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_balance_snapshots(PageLimit::new(200).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_current_station_balance_snapshots(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_current_station_balance_snapshots",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            EmptyInputDto::parse(input)?;
            facade
                .list_balance_snapshots(PageLimit::new(200).expect("bounded limit"))
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn list_balance_snapshots_for_station(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "list_balance_snapshots_for_station",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = CollectorStationIdInputDto::parse(input)?;
            facade
                .list_balance_snapshots_for_station(&input.station_id)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}

#[tauri::command]
pub async fn upsert_balance_snapshot(
    facade: State<'_, PricingCommandFacade>,
    input: Value,

    runtime_context_registry: tauri::State<
        '_,
        crate::ipc::dto::runtime_context::RuntimeContextRegistry,
    >,
    runtime_context: Option<serde_json::Value>,
) -> Result<BalanceSnapshotDto, error::CommandError> {
    correlation::in_command_scope_with_runtime_context(
        "upsert_balance_snapshot",
        runtime_context_registry.inner(),
        runtime_context,
        async {
            let input = UpsertBalanceSnapshotInputDto::parse(input)?.into_domain();
            facade
                .upsert_balance_snapshot(input)
                .await
                .map_err(super::public_command_application_error)
        },
    )
    .await
}
