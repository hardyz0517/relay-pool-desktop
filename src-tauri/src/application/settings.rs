use std::sync::{Arc, RwLock};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};

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
    data_directories: Arc<RwLock<DataDirectoryProjection>>,
}

#[derive(Clone)]
struct DataDirectoryProjection {
    active: String,
    pending: Option<String>,
}

impl SettingsService {
    const INSECURE_LOCAL_KEY_PLACEHOLDER: &'static str = "sk-local-pool-change-me";

    pub(crate) fn new(
        runtime: PersistenceHandle,
        clock: Arc<dyn Clock>,
        data_dir: String,
        pending_data_dir: Option<String>,
    ) -> Self {
        Self {
            runtime,
            clock,
            store: SettingsStore::new(),
            data_directories: Arc::new(RwLock::new(DataDirectoryProjection {
                active: data_dir,
                pending: pending_data_dir,
            })),
        }
    }

    pub(crate) async fn load(&self) -> Result<AppSettings, ApplicationError> {
        let projection = self.data_directory_projection()?;
        let mut read = self.runtime.begin_read().await?;
        self.store
            .load(&mut read, &projection.active, projection.pending)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn repair_legacy_settings(&self) -> Result<u64, ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.repair_legacy_settings(write).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) fn set_data_directory_projection(
        &self,
        active: String,
        pending: Option<String>,
    ) -> Result<(), ApplicationError> {
        let mut projection = self
            .data_directories
            .write()
            .map_err(|_| ApplicationError::Internal)?;
        projection.active = active;
        projection.pending = pending;
        Ok(())
    }

    pub(crate) async fn ensure_local_access_key(&self) -> Result<String, ApplicationError> {
        let mut random = [0_u8; 32];
        OsRng.fill_bytes(&mut random);
        let generated = format!("sk-local-{}", URL_SAFE_NO_PAD.encode(random));
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .ensure_local_access_key(
                            write,
                            &generated,
                            Self::INSECURE_LOCAL_KEY_PLACEHOLDER,
                            &now,
                        )
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_local_access_key(
        &self,
        value: String,
    ) -> Result<AppSettings, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        let projection = self.data_directory_projection()?;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .update_local_access_key(
                            write,
                            &value,
                            &now,
                            &projection.active,
                            projection.pending,
                        )
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn set_local_proxy_start_on_launch(
        &self,
        enabled: bool,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .set_local_proxy_start_on_launch(write, enabled, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update(
        &self,
        input: UpdateSettingsInput,
    ) -> Result<AppSettings, ApplicationError> {
        let store = self.store;
        let update = SettingsUpdate {
            now: self.now_ms_string(),
            input,
        };
        let projection = self.data_directory_projection()?;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .update(write, update, &projection.active, projection.pending)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    #[allow(
        dead_code,
        reason = "exercised by the released-schema differential integration contract"
    )]
    pub(crate) async fn import_known_legacy_settings(
        &self,
        values: Vec<(String, String)>,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
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

    fn data_directory_projection(&self) -> Result<DataDirectoryProjection, ApplicationError> {
        self.data_directories
            .read()
            .map(|projection| projection.clone())
            .map_err(|_| ApplicationError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::clock::SystemClock, persistence::runtime::PersistenceRuntime};

    #[tokio::test]
    async fn ensure_local_access_key_replaces_placeholder_once_under_concurrency() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        let service = Arc::new(SettingsService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            temp.path().display().to_string(),
            None,
        ));

        let (first, second) = tokio::join!(
            service.ensure_local_access_key(),
            service.ensure_local_access_key()
        );
        let first = first.expect("first key");
        let second = second.expect("second key");

        assert_eq!(first, second);
        assert!(first.starts_with("sk-local-"));
        assert_ne!(first, SettingsService::INSECURE_LOCAL_KEY_PLACEHOLDER);
        drop(service);
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn legacy_tray_behavior_is_read_compatibly_and_repaired_transactionally() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("legacy-settings.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE settings SET value = 'minimize-to-tray' WHERE key = 'tray_behavior'",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("seed legacy setting");
        let service = SettingsService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            temp.path().display().to_string(),
            None,
        );

        let compatible = service.load().await.expect("compatible settings load");
        assert_eq!(compatible.tray_behavior, "minimize_to_tray");
        assert_eq!(
            service
                .repair_legacy_settings()
                .await
                .expect("repair legacy settings"),
            1
        );
        assert_eq!(
            service
                .repair_legacy_settings()
                .await
                .expect("idempotent repair"),
            0
        );

        let mut read = runtime.begin_read().await.expect("read repaired setting");
        let persisted: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'tray_behavior'")
                .fetch_one(read.connection())
                .await
                .expect("persisted tray behavior");
        assert_eq!(persisted, "minimize_to_tray");
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }
}
