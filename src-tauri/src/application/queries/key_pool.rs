use super::read_model_revision::load_asset_revision;
use crate::{
    application::{error::ApplicationError, pagination::PageLimit},
    models::routing_read_models::{
        KeyPoolReadModel, ReadModelEnvelope, ReadModelPage, ASSET_READ_MODEL_SCHEMA_VERSION,
    },
    persistence::{runtime::PersistenceHandle, stores::credential_store::CredentialStore},
};

#[derive(Clone)]
pub(crate) struct KeyPoolQuery {
    runtime: PersistenceHandle,
    credentials: CredentialStore,
}
impl KeyPoolQuery {
    pub(crate) fn new(runtime: PersistenceHandle) -> Self {
        Self {
            runtime,
            credentials: CredentialStore,
        }
    }
    pub(crate) async fn load(
        &self,
        limit: PageLimit,
    ) -> Result<ReadModelEnvelope<KeyPoolReadModel>, ApplicationError> {
        let limit = limit.get();
        let mut read = self.runtime.begin_read().await?;
        let domain_revision = load_asset_revision(&mut read).await?;
        let mut rows = self.credentials.list_key_pool_items(&mut read).await?;
        rows.truncate(limit as usize);
        let returned = rows.len() as u32;
        Ok(ReadModelEnvelope {
            schema_version: ASSET_READ_MODEL_SCHEMA_VERSION,
            generated_at_ms: chrono::Utc::now().timestamp_millis(),
            domain_revision,
            page: ReadModelPage {
                limit,
                returned,
                next_cursor: None,
            },
            data: KeyPoolReadModel { rows },
        })
    }
}
