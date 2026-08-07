use crate::{
    application::error::ApplicationError,
    models::station_keys::KeyPoolItem,
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
    pub(crate) async fn load_all(&self) -> Result<Vec<KeyPoolItem>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.credentials
            .list_key_pool_items(&mut read)
            .await
            .map_err(Into::into)
    }
}
