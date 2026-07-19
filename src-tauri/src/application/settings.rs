use std::sync::Arc;

use crate::{
    application::{clock::Clock, error::ApplicationError},
    models::settings::{AppSettings, UpdateSettingsInput},
    persistence::{
        runtime::PersistenceHandle,
        stores::settings_store::{SettingsStore, SettingsUpdate},
    },
};

#[derive(Clone)]
pub(crate) struct SettingsService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    store: SettingsStore,
}

impl SettingsService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        data_dir: String,
        pending_data_dir: Option<String>,
    ) -> Self {
        Self {
            runtime,
            clock,
            store: SettingsStore::new(data_dir, pending_data_dir),
        }
    }

    pub(crate) async fn load(&self) -> Result<AppSettings, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.store.load(&mut read).await.map_err(Into::into)
    }

    pub(crate) async fn update_local_access_key(
        &self,
        value: String,
    ) -> Result<AppSettings, ApplicationError> {
        let store = self.store.clone();
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move { store.update_local_access_key(write, &value, &now).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update(
        &self,
        input: UpdateSettingsInput,
    ) -> Result<AppSettings, ApplicationError> {
        let store = self.store.clone();
        let update = SettingsUpdate {
            now: self.now_ms_string(),
            input,
        };
        self.runtime
            .write(|write| Box::pin(async move { store.update(write, update).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn import_known_legacy_settings(
        &self,
        values: Vec<(String, String)>,
    ) -> Result<(), ApplicationError> {
        let store = self.store.clone();
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .import_known_legacy_settings(write, &values, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    fn now_ms_string(&self) -> String {
        self.clock.now_utc().timestamp_millis().to_string()
    }
}
