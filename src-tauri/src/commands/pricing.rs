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
            PricingRuleIdInputDto, UpsertModelBasePriceInputDto, UpsertPricingRuleInputDto,
        },
        pricing_reads::{
            ModelBasePriceDto, PricingContextInputDto, PricingRuleDto, ResolvedPricingContextDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
};

#[tauri::command]
pub async fn list_pricing_rules(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<PricingRuleDto>, error::CommandError> {
    correlation::in_command_scope("list_pricing_rules", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_pricing_rules(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_model_base_prices(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<ModelBasePriceDto>, error::CommandError> {
    correlation::in_command_scope("list_model_base_prices", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_model_base_prices(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_model_base_price(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<ModelBasePriceDto, error::CommandError> {
    correlation::in_command_scope("upsert_model_base_price", async {
        let input = UpsertModelBasePriceInputDto::parse(input)?.into_domain();
        facade
            .upsert_model_base_price(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reset_model_base_prices_to_builtins(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<ModelBasePriceDto>, error::CommandError> {
    correlation::in_command_scope("reset_model_base_prices_to_builtins", async {
        EmptyInputDto::parse(input)?;
        facade
            .reset_model_base_prices_to_builtins(PageLimit::new(500).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_pricing_rule(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<PricingRuleDto, error::CommandError> {
    correlation::in_command_scope("upsert_pricing_rule", async {
        let input = UpsertPricingRuleInputDto::parse(input)?.into_domain();
        facade
            .upsert_pricing_rule(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_pricing_rule(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_pricing_rule", async {
        let input = PricingRuleIdInputDto::parse(input)?;
        facade
            .delete_pricing_rule(input.id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn resolve_station_key_pricing_context(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<ResolvedPricingContextDto, error::CommandError> {
    correlation::in_command_scope("resolve_station_key_pricing_context", async {
        let (station_key_id, requested_model, request_kind) =
            PricingContextInputDto::parse(input)?.into_parts();
        facade
            .resolve_station_key_pricing_context(&station_key_id, &requested_model, request_kind)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_balance_snapshots(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_balance_snapshots", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_balance_snapshots(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_current_station_balance_snapshots(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_current_station_balance_snapshots", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_balance_snapshots(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_balance_snapshots_for_station(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_balance_snapshots_for_station", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        facade
            .list_balance_snapshots_for_station(&input.station_id)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_balance_snapshot(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<BalanceSnapshotDto, error::CommandError> {
    correlation::in_command_scope("upsert_balance_snapshot", async {
        let input = UpsertBalanceSnapshotInputDto::parse(input)?.into_domain();
        facade
            .upsert_balance_snapshot(input)
            .await
            .map_err(super::public_command_application_error)
    })
    .await
}
