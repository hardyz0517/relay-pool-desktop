use std::sync::{Arc, RwLock};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::{rngs::OsRng, RngCore};

use crate::{
    application::{
        clock::Clock,
        credentials::{CredentialVault, EncryptedSecret, SecretBytes, SecretRef},
        error::ApplicationError,
        ids::IdGenerator,
    },
    models::settings::{AppSettings, UpdateSettingsInput},
    persistence::{
        runtime::PersistenceHandle,
        stores::{
            credential_store::{EncryptedSecretRow, StoredEncryptedSecret},
            settings_store::{SettingsStore, SettingsUpdate},
        },
    },
};

#[derive(Clone)]
pub(crate) struct SettingsService {
    runtime: PersistenceHandle,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    vault: Arc<dyn CredentialVault>,
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
        ids: Arc<dyn IdGenerator>,
        vault: Arc<dyn CredentialVault>,
        data_dir: String,
        pending_data_dir: Option<String>,
    ) -> Self {
        Self {
            runtime,
            clock,
            ids,
            vault,
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
        let generated = generate_local_access_key();
        let store = self.store;
        let now = self.now_ms_string();
        let vault = self.vault.clone();
        let ids = self.ids.clone();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    if let Some(secret) = store.local_access_key_secret_for_write(write).await? {
                        return decrypt_local_access_key(&*vault, secret);
                    }
                    let legacy = store.local_access_key_setting_value(write).await?;
                    if !legacy.trim().is_empty() && legacy != Self::INSECURE_LOCAL_KEY_PLACEHOLDER {
                        return Err(
                            crate::persistence::error::PersistenceError::InvariantViolation(
                                "local access key secret is missing after encrypted-secret baseline"
                                    .to_string(),
                            ),
                        );
                    }
                    let value = generated;
                    let row = encrypt_local_access_key(&*vault, &*ids, &value, &now)?;
                    store.upsert_local_access_key_secret(write, &row).await?;
                    Ok(value)
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
        let vault = self.vault.clone();
        let ids = self.ids.clone();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let local_key = value.trim();
                    if local_key.is_empty() {
                        return Err(
                            crate::persistence::error::PersistenceError::ConstraintViolation,
                        );
                    }
                    let row = encrypt_local_access_key(&*vault, &*ids, local_key, &now)?;
                    store.upsert_local_access_key_secret(write, &row).await?;
                    Ok(())
                })
            })
            .await
            .map_err(ApplicationError::from)?;
        self.load().await
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

pub(crate) fn generate_local_access_key() -> String {
    let mut random = [0_u8; 32];
    OsRng.fill_bytes(&mut random);
    format!("sk-local-{}", URL_SAFE_NO_PAD.encode(random))
}

fn local_access_key_ref(ids: &dyn IdGenerator) -> SecretRef {
    SecretRef {
        id: ids.next_id(),
        scope: "settings".to_string(),
        owner_id: "local_key".to_string(),
        kind: "local_access_key".to_string(),
    }
}

fn encrypt_local_access_key(
    vault: &dyn CredentialVault,
    ids: &dyn IdGenerator,
    value: &str,
    now: &str,
) -> Result<EncryptedSecretRow, crate::persistence::error::PersistenceError> {
    let secret_ref = local_access_key_ref(ids);
    let encrypted = vault
        .encrypt(&secret_ref.aad(), SecretBytes::from(value.to_string()))
        .map_err(|_| crate::persistence::error::PersistenceError::DatabaseFailed)?;
    Ok(EncryptedSecretRow {
        id: secret_ref.id,
        scope: secret_ref.scope,
        owner_id: secret_ref.owner_id,
        kind: secret_ref.kind,
        masked_value: encrypted.masked_value,
        ciphertext: encrypted.ciphertext,
        nonce: encrypted.nonce,
        key_id: encrypted.key_id,
        encryption_version: encrypted.encryption_version,
        value_hash: encrypted.value_hash,
        now: now.to_string(),
    })
}

fn decrypt_local_access_key(
    vault: &dyn CredentialVault,
    secret: StoredEncryptedSecret,
) -> Result<String, crate::persistence::error::PersistenceError> {
    let secret_ref = SecretRef {
        id: secret.id,
        scope: secret.scope,
        owner_id: secret.owner_id,
        kind: secret.kind,
    };
    let encrypted = EncryptedSecret {
        ciphertext: secret.ciphertext,
        nonce: secret.nonce,
        masked_value: secret.masked_value,
        key_id: secret.key_id,
        encryption_version: secret.encryption_version,
        value_hash: secret.value_hash,
    };
    let decrypted = vault
        .decrypt(
            &secret_ref.aad(),
            &encrypted.key_id,
            encrypted.encryption_version,
            &encrypted,
        )
        .map_err(|_| crate::persistence::error::PersistenceError::DatabaseFailed)?;
    String::from_utf8(decrypted.as_bytes().to_vec())
        .map_err(|_| crate::persistence::error::PersistenceError::DatabaseFailed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator},
        persistence::runtime::PersistenceRuntime,
        services::secrets::vault::DataKeyVault,
    };

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
            Arc::new(UuidV7Generator),
            Arc::new(DataKeyVault::for_test([9; 32])),
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
    async fn ensure_local_access_key_rejects_unmigrated_plaintext_after_baseline() {
        let temp = tempfile::tempdir().expect("tempdir");
        let path = temp.path().join("settings.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&path)
            .await
            .expect("runtime");
        runtime
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE settings SET value = 'sk-legacy-local-canary' WHERE key = 'local_key'",
                    )
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("seed unmigrated plaintext");
        let service = SettingsService::new(
            runtime.handle(),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
            Arc::new(DataKeyVault::for_test([9; 32])),
            temp.path().display().to_string(),
            None,
        );

        let error = service
            .ensure_local_access_key()
            .await
            .expect_err("unmigrated local key must not be imported on hot path");

        drop(error);
        assert_eq!(
            query_local_key_plaintext(&runtime).await,
            "sk-legacy-local-canary"
        );
        runtime.close().await.expect("close persistence runtime");
    }

    #[tokio::test]
    async fn legacy_tray_behavior_is_read_compatibly_without_hot_path_repair() {
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
            Arc::new(UuidV7Generator),
            Arc::new(DataKeyVault::for_test([9; 32])),
            temp.path().display().to_string(),
            None,
        );

        let compatible = service.load().await.expect("compatible settings load");
        assert_eq!(compatible.tray_behavior, "minimize_to_tray");

        let mut read = runtime.begin_read().await.expect("read repaired setting");
        let persisted: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'tray_behavior'")
                .fetch_one(read.connection())
                .await
                .expect("persisted tray behavior");
        assert_eq!(persisted, "minimize-to-tray");
        drop(read);
        runtime.close().await.expect("close persistence runtime");
    }

    async fn query_local_key_plaintext(runtime: &PersistenceRuntime) -> String {
        let mut read = runtime.begin_read().await.expect("read local key");
        let value = sqlx::query_scalar("SELECT value FROM settings WHERE key = 'local_key'")
            .fetch_one(read.connection())
            .await
            .expect("local key");
        drop(read);
        value
    }
}
