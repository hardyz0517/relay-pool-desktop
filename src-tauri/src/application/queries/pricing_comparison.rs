use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    models::shared_capabilities::PricingComparisonWorkspace,
    persistence::{
        runtime::PersistenceHandle,
        stores::pricing_store::{PricingComparisonRows, PricingStore},
    },
};

#[derive(Clone)]
pub(crate) struct PricingComparisonQuery {
    runtime: PersistenceHandle,
    store: PricingStore,
}

impl PricingComparisonQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            store: PricingStore,
        }
    }

    pub(crate) async fn load(
        &self,
        limit: PageLimit,
    ) -> Result<PricingComparisonWorkspace, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let rows = self
            .store
            .load_comparison_workspace(&mut read, limit.get())
            .await?;
        Ok(workspace_from_rows(rows))
    }
}

fn workspace_from_rows(rows: PricingComparisonRows) -> PricingComparisonWorkspace {
    PricingComparisonWorkspace {
        stations: rows.stations,
        station_keys: rows.station_keys,
        group_bindings: rows.group_bindings,
        group_rates: rows.group_rates,
        pricing_rules: rows.pricing_rules,
        developer_mode_enabled: rows.developer_mode_enabled,
    }
}
