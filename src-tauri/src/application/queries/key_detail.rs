use super::read_model_revision::load_asset_revision;
use crate::{
    application::{error::ApplicationError, queries::station_assets::server_group_identity_hash},
    models::routing_read_models::{
        KeyDetailReadModel, ReadModelEnvelope, ReadModelPage, ASSET_READ_MODEL_SCHEMA_VERSION,
    },
    persistence::{runtime::PersistenceHandle, stores::credential_store::CredentialStore},
};

#[derive(Clone)]
pub(crate) struct KeyDetailQuery {
    runtime: PersistenceHandle,
    credentials: CredentialStore,
}
impl KeyDetailQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            credentials: CredentialStore,
        }
    }
    pub(crate) async fn load(
        &self,
        station_key_id: &str,
    ) -> Result<ReadModelEnvelope<KeyDetailReadModel>, ApplicationError> {
        if station_key_id.trim().is_empty() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let mut read = self.runtime.begin_read().await?;
        let domain_revision = load_asset_revision(&mut read).await?;
        let item = self
            .credentials
            .list_key_pool_items(&mut read)
            .await?
            .into_iter()
            .find(|item| item.id == station_key_id)
            .ok_or(ApplicationError::NotFound)?;
        Ok(ReadModelEnvelope {
            schema_version: ASSET_READ_MODEL_SCHEMA_VERSION,
            generated_at_ms: chrono::Utc::now().timestamp_millis(),
            domain_revision,
            page: ReadModelPage {
                limit: 1,
                returned: 1,
                next_cursor: None,
            },
            data: KeyDetailReadModel {
                group_identity_hash: server_group_identity_hash(&item),
                item,
            },
        })
    }
}
