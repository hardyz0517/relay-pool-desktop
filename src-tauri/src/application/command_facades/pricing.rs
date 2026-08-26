use std::sync::Arc;

use crate::{
    application::{
        error::ApplicationError, pagination::PageLimit, pricing::PricingService,
        queries::pricing_comparison::PricingComparisonQuery,
    },
    models::{
        pricing::{
            BalanceSnapshot, ModelBasePrice, RequestKind, ResolvedPricingContext,
            UpsertBalanceSnapshotInput, UpsertModelBasePriceInput,
        },
        pricing_group_monitoring::{
            PricingGroupMonitorStatusInput, PricingGroupMonitorStatusWorkspace,
        },
        shared_capabilities::PricingComparisonWorkspace,
    },
    services::model_price_sync::{
        ModelPriceCatalogEntry, ModelPriceSyncConfig, ModelPriceSyncError, ModelPriceSyncResult,
        ModelPriceSyncService, ModelPriceSyncState,
    },
};

#[derive(Clone)]
pub(crate) struct PricingCommandFacade {
    pricing: Arc<PricingService>,
    pricing_comparison: Arc<PricingComparisonQuery>,
    pricing_group_monitor_status: Arc<
        crate::application::queries::pricing_group_monitor_status::PricingGroupMonitorStatusQuery,
    >,
    model_price_sync: Arc<ModelPriceSyncService>,
}

impl PricingCommandFacade {
    pub(crate) fn new(
        pricing: Arc<PricingService>,
        pricing_comparison: Arc<PricingComparisonQuery>,
        pricing_group_monitor_status: Arc<
            crate::application::queries::pricing_group_monitor_status::PricingGroupMonitorStatusQuery,
        >,
        model_price_sync: Arc<ModelPriceSyncService>,
    ) -> Self {
        Self {
            pricing,
            pricing_comparison,
            pricing_group_monitor_status,
            model_price_sync,
        }
    }

    pub(crate) fn get_model_price_sync_state(
        &self,
    ) -> Result<ModelPriceSyncState, ApplicationError> {
        self.model_price_sync.state()
    }

    pub(crate) fn list_model_price_sync_catalog(
        &self,
    ) -> Result<Vec<ModelPriceCatalogEntry>, ApplicationError> {
        self.model_price_sync.catalog_entries()
    }

    pub(crate) async fn save_model_price_sync_config(
        &self,
        config: ModelPriceSyncConfig,
    ) -> Result<ModelPriceSyncState, ApplicationError> {
        self.model_price_sync.save_config(config).await
    }

    pub(crate) async fn sync_model_prices(
        &self,
        force: bool,
    ) -> Result<ModelPriceSyncResult, ModelPriceSyncError> {
        self.model_price_sync.sync(force).await
    }

    pub(crate) async fn reload_model_price_catalog(
        &self,
    ) -> Result<ModelPriceSyncState, ApplicationError> {
        self.model_price_sync.reload().await
    }

    pub(crate) fn open_model_price_catalog_directory(&self) -> Result<(), ApplicationError> {
        self.model_price_sync.open_catalog_directory()
    }

    pub(crate) fn model_price_sync_service(&self) -> Arc<ModelPriceSyncService> {
        Arc::clone(&self.model_price_sync)
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
        self.model_price_sync.upsert_local_price(input).await
    }

    pub(crate) async fn delete_model_base_price(&self, id: String) -> Result<(), ApplicationError> {
        self.model_price_sync.delete_local_price(id).await
    }

    pub(crate) async fn reset_model_base_prices_to_builtins(
        &self,
        limit: PageLimit,
    ) -> Result<Vec<ModelBasePrice>, ApplicationError> {
        let _ = limit;
        self.model_price_sync.reset_to_builtins().await
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

    pub(crate) async fn load_pricing_comparison_workspace(
        &self,
        limit: PageLimit,
    ) -> Result<PricingComparisonWorkspace, ApplicationError> {
        self.pricing_comparison.load(limit).await
    }

    pub(crate) async fn load_pricing_group_monitor_status(
        &self,
        input: PricingGroupMonitorStatusInput,
    ) -> Result<PricingGroupMonitorStatusWorkspace, ApplicationError> {
        self.pricing_group_monitor_status.load(input).await
    }
}
