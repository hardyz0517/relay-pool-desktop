use std::sync::Arc;

use crate::{
    application::{error::ApplicationError, pagination::PageLimit, pricing::PricingService},
    models::pricing::{
        BalanceSnapshot, ModelBasePrice, PricingRule, RequestKind, ResolvedPricingContext,
        UpsertBalanceSnapshotInput, UpsertModelBasePriceInput, UpsertPricingRuleInput,
    },
};

#[derive(Clone)]
pub(crate) struct PricingCommandFacade {
    pricing: Arc<PricingService>,
}

impl PricingCommandFacade {
    pub(crate) fn new(pricing: Arc<PricingService>) -> Self {
        Self { pricing }
    }

    pub(crate) async fn list_pricing_rules(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<PricingRule>, ApplicationError> {
        self.pricing.list_pricing_rules(limit).await
    }

    pub(crate) async fn list_model_base_prices(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ModelBasePrice>, ApplicationError> {
        self.pricing.list_model_base_prices(limit).await
    }

    pub(crate) async fn upsert_model_base_price(
        &self,
        input: UpsertModelBasePriceInput,
    ) -> Result<ModelBasePrice, ApplicationError> {
        self.pricing.upsert_model_base_price(input).await
    }

    pub(crate) async fn reset_model_base_prices_to_builtins(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ModelBasePrice>, ApplicationError> {
        self.pricing
            .reset_model_base_prices_to_builtins(limit)
            .await
    }

    pub(crate) async fn upsert_pricing_rule(
        &self,
        input: UpsertPricingRuleInput,
    ) -> Result<PricingRule, ApplicationError> {
        self.pricing.upsert_pricing_rule(input).await
    }

    pub(crate) async fn delete_pricing_rule(&self, id: String) -> Result<(), ApplicationError> {
        self.pricing.delete_pricing_rule(id).await
    }

    pub(crate) async fn resolve_station_key_pricing_context(
        &self,
        station_key_id: &str,
        requested_model: &str,
        request_kind: RequestKind,
    ) -> Result<ResolvedPricingContext, ApplicationError> {
        self.pricing
            .resolve_station_key_pricing_context(
                station_key_id,
                requested_model,
                Some(request_kind),
            )
            .await
    }

    pub(crate) async fn list_balance_snapshots(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<BalanceSnapshot>, ApplicationError> {
        self.pricing.latest_station_balances(limit).await
    }

    pub(crate) async fn upsert_balance_snapshot(
        &self,
        input: UpsertBalanceSnapshotInput,
    ) -> Result<BalanceSnapshot, ApplicationError> {
        self.pricing.upsert_balance_snapshot(input).await
    }
}
