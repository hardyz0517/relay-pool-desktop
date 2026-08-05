use super::read_model_revision::load_asset_revision;
use crate::{
    application::{error::ApplicationError, queries::station_assets::server_group_identity_hash},
    models::routing_read_models::{
        ReadModelEnvelope, ReadModelPage, StationDetailReadModel, ASSET_READ_MODEL_SCHEMA_VERSION,
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::{credential_store::CredentialStore, station_catalog::StationCatalogStore},
    },
};

#[derive(Clone)]
pub(crate) struct StationDetailQuery {
    runtime: PersistenceHandle,
    stations: StationCatalogStore,
    credentials: CredentialStore,
}

impl StationDetailQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            stations: StationCatalogStore,
            credentials: CredentialStore,
        }
    }
    pub(crate) async fn load(
        &self,
        station_id: &str,
    ) -> Result<ReadModelEnvelope<StationDetailReadModel>, ApplicationError> {
        if station_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        let domain_revision = load_asset_revision(&mut read).await?;
        let station = self.stations.get(&mut read, station_id).await?;
        // Use the same projected key rows as the pool page.  This keeps health, capabilities,
        // balance and endpoint status owned by one backend query instead of rebuilding them here.
        let keys = self
            .credentials
            .list_key_pool_items(&mut read)
            .await?
            .into_iter()
            .filter(|key| key.station_id == station_id)
            .collect::<Vec<_>>();
        let group_identity_hashes = keys.iter().filter_map(server_group_identity_hash).collect();
        Ok(ReadModelEnvelope {
            schema_version: ASSET_READ_MODEL_SCHEMA_VERSION,
            generated_at_ms: chrono::Utc::now().timestamp_millis(),
            domain_revision,
            page: ReadModelPage {
                limit: keys.len() as u32,
                returned: keys.len() as u32,
                next_cursor: None,
            },
            data: StationDetailReadModel {
                station,
                keys,
                group_identity_hashes,
            },
        })
    }
}
