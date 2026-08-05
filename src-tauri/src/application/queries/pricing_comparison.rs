use super::read_model_revision::load_asset_revision;
use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    models::{
        routing_read_models::PricingComparisonReadModel,
        shared_capabilities::PricingComparisonWorkspace,
    },
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

    /// Loads the pricing display model from one read session.  Consumers join monitoring
    /// overlays using `group_identity_hashes`; they never reconstruct group identity locally.
    pub(crate) async fn load_read_model(
        &self,
        limit: PageLimit,
    ) -> Result<PricingComparisonReadModel, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let rows = self
            .store
            .load_comparison_workspace(&mut read, limit.get())
            .await?;
        let domain_revision = load_asset_revision(&mut read).await?;
        let workspace = workspace_from_rows(rows);
        let mut identities = workspace
            .station_keys
            .iter()
            .filter_map(|key| {
                key.group_binding_id
                    .as_deref()
                    .or(key.group_id_hash.as_deref())
            })
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
            .filter_map(crate::models::routing_read_models::group_identity_hash)
            .collect::<Vec<_>>();
        identities.sort();
        identities.dedup();
        Ok(PricingComparisonReadModel {
            schema_version: crate::models::routing_read_models::ASSET_READ_MODEL_SCHEMA_VERSION,
            generated_at_ms: chrono::Utc::now().timestamp_millis(),
            domain_revision,
            workspace,
            group_identity_hashes: identities,
        })
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
