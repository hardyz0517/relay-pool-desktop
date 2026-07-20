use serde::Serialize;

use crate::{
    application::{
        error::ApplicationError, pagination::PageLimit, pricing::PricingService,
        queries::pricing_comparison::PricingComparisonQuery,
    },
    models::{
        pricing::{
            BalanceSnapshot, ModelBasePrice, PricingRule, UpsertBalanceSnapshotInput,
            UpsertModelBasePriceInput, UpsertPricingRuleInput,
        },
        shared_capabilities::PricingComparisonWorkspace,
    },
};

const DEFAULT_PAGE_LIMIT: u32 = 200;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PricingCommandError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) async fn list_pricing_rules(
    service: &PricingService,
    limit: Option<u32>,
) -> Result<Vec<PricingRule>, PricingCommandError> {
    service
        .list_pricing_rules(page_limit(limit)?)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn list_model_base_prices(
    service: &PricingService,
    limit: Option<u32>,
) -> Result<Vec<ModelBasePrice>, PricingCommandError> {
    service
        .list_model_base_prices(page_limit(limit)?)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn latest_station_balances(
    service: &PricingService,
    limit: Option<u32>,
) -> Result<Vec<BalanceSnapshot>, PricingCommandError> {
    service
        .latest_station_balances(page_limit(limit)?)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn load_pricing_comparison_workspace(
    query: &PricingComparisonQuery,
    limit: Option<u32>,
) -> Result<PricingComparisonWorkspace, PricingCommandError> {
    query
        .load(page_limit(limit)?)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn upsert_pricing_rule(
    service: &PricingService,
    input: UpsertPricingRuleInput,
) -> Result<PricingRule, PricingCommandError> {
    service
        .upsert_pricing_rule(input)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn delete_pricing_rule(
    service: &PricingService,
    id: String,
) -> Result<(), PricingCommandError> {
    service
        .delete_pricing_rule(id)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn upsert_model_base_price(
    service: &PricingService,
    input: UpsertModelBasePriceInput,
) -> Result<ModelBasePrice, PricingCommandError> {
    service
        .upsert_model_base_price(input)
        .await
        .map_err(pricing_command_error)
}

pub(crate) async fn upsert_balance_snapshot(
    service: &PricingService,
    input: UpsertBalanceSnapshotInput,
) -> Result<BalanceSnapshot, PricingCommandError> {
    service
        .upsert_balance_snapshot(input)
        .await
        .map_err(pricing_command_error)
}

fn page_limit(value: Option<u32>) -> Result<PageLimit, PricingCommandError> {
    PageLimit::new(value.unwrap_or(DEFAULT_PAGE_LIMIT)).map_err(pricing_command_error)
}

fn pricing_command_error(error: ApplicationError) -> PricingCommandError {
    match error {
        ApplicationError::NotFound => PricingCommandError {
            code: "not_found",
            message: "not found",
        },
        ApplicationError::ConstraintViolation | ApplicationError::Conflict => PricingCommandError {
            code: "invalid_request",
            message: "invalid request",
        },
        ApplicationError::Busy => PricingCommandError {
            code: "busy",
            message: "resource busy",
        },
        ApplicationError::Unavailable => PricingCommandError {
            code: "unavailable",
            message: "persistence unavailable",
        },
        _ => PricingCommandError {
            code: "internal",
            message: "internal failure",
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unbounded_command_page() {
        assert_eq!(
            page_limit(Some(0)).expect_err("zero").code,
            "invalid_request"
        );
        assert_eq!(
            page_limit(Some(501)).expect_err("oversized").code,
            "invalid_request"
        );
    }
}
