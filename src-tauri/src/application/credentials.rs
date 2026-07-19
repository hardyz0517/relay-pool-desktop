use std::{fmt, sync::Arc};

use zeroize::Zeroizing;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator},
    models::{
        remote_keys::RemoteStationKey,
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::credential_store::{
            CredentialStore, EncryptedSecretRow, NewRemoteStationKeyRow, NewStationKeyRow,
            StationKeyPatch,
        },
    },
};

pub(crate) struct SecretBytes(Zeroizing<Vec<u8>>);

impl SecretBytes {
    pub(crate) fn as_bytes(&self) -> &[u8] {
        self.0.as_slice()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for SecretBytes {
    fn from(value: Vec<u8>) -> Self {
        Self(Zeroizing::new(value))
    }
}

impl From<String> for SecretBytes {
    fn from(value: String) -> Self {
        Self::from(value.into_bytes())
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SecretBytes")
            .field("len", &self.0.len())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EncryptedSecret {
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
    pub(crate) masked_value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SecretRef {
    pub(crate) id: String,
    pub(crate) scope: String,
    pub(crate) owner_id: String,
    pub(crate) kind: String,
}

impl SecretRef {
    pub(crate) fn aad(&self) -> String {
        secret_aad(&self.scope, &self.owner_id, &self.kind)
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum CredentialError {
    #[error("secret validation failed")]
    SecretValidationFailed,
    #[error("internal failure")]
    Internal,
}

impl From<CredentialError> for ApplicationError {
    fn from(error: CredentialError) -> Self {
        match error {
            CredentialError::SecretValidationFailed => Self::SecretValidationFailed,
            CredentialError::Internal => Self::Internal,
        }
    }
}

pub(crate) trait CredentialVault: Send + Sync {
    fn encrypt(
        &self,
        aad: &str,
        plaintext: SecretBytes,
    ) -> Result<EncryptedSecret, CredentialError>;

    #[allow(dead_code)]
    fn decrypt(
        &self,
        aad: &str,
        encrypted: &EncryptedSecret,
    ) -> Result<SecretBytes, CredentialError>;
}

#[derive(Debug, Clone)]
pub(crate) struct SavedSecretRef {
    pub(crate) secret_ref: SecretRef,
    pub(crate) station_key: StationKey,
}

#[derive(Clone)]
pub(crate) struct CredentialService {
    runtime: PersistenceHandle,
    vault: Arc<dyn CredentialVault>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    store: CredentialStore,
}

impl CredentialService {
    pub(crate) fn new(
        runtime: PersistenceHandle,
        vault: Arc<dyn CredentialVault>,
        clock: Arc<dyn Clock>,
        ids: Arc<dyn IdGenerator>,
    ) -> Self {
        Self {
            runtime,
            vault,
            clock,
            ids,
            store: CredentialStore,
        }
    }

    pub(crate) async fn list_station_keys(
        &self,
        station_id: String,
    ) -> Result<Vec<StationKey>, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        store
            .list_station_keys(&mut read, &station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn create_station_key(
        &self,
        input: CreateStationKeyInput,
    ) -> Result<StationKey, ApplicationError> {
        let key_id = self.ids.next_id();
        let now = self.now_ms_string();
        let secret = SecretBytes::from(input.api_key);
        let encrypted_secret = self.encrypt_station_key_secret(&key_id, secret, &now)?;
        let row = NewStationKeyRow {
            id: key_id,
            station_id: input.station_id,
            name: input.name,
            encrypted_secret,
            enabled: input.enabled,
            priority: input.priority,
            max_concurrency: input.max_concurrency,
            load_factor: input.load_factor,
            schedulable: input.schedulable,
            group_name: input.group_name,
            tier_label: input.tier_label,
            group_binding_id: input.group_binding_id,
            group_id_hash: input.group_id_hash,
            rate_multiplier: input.rate_multiplier,
            manual_rate_multiplier: input.manual_rate_multiplier,
            manual_rate_updated_at: input.manual_rate_multiplier.map(|_| now.clone()),
            rate_source: input.rate_source,
            balance_scope: input.balance_scope,
            note: input.note,
            now,
        };
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.insert_station_key(write, row).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_station_key(
        &self,
        input: UpdateStationKeyInput,
    ) -> Result<StationKey, ApplicationError> {
        let now = self.now_ms_string();
        let encrypted_secret = input
            .api_key
            .map(SecretBytes::from)
            .filter(|secret| !secret.is_empty())
            .map(|secret| self.encrypt_station_key_secret(&input.id, secret, &now))
            .transpose()?
            .flatten();
        let patch = StationKeyPatch {
            id: input.id,
            station_id: input.station_id,
            name: input.name,
            encrypted_secret,
            enabled: input.enabled,
            priority: input.priority,
            max_concurrency: input.max_concurrency,
            load_factor: input.load_factor,
            schedulable: input.schedulable,
            group_name: input.group_name,
            tier_label: input.tier_label,
            group_binding_id: input.group_binding_id,
            group_id_hash: input.group_id_hash,
            rate_multiplier: input.rate_multiplier,
            manual_rate_multiplier: input.manual_rate_multiplier,
            manual_rate_updated_at: input.manual_rate_multiplier.flatten().map(|_| now.clone()),
            rate_source: input.rate_source,
            balance_scope: input.balance_scope,
            status: input.status,
            note: input.note,
            now,
        };
        let store = self.store;
        self.runtime
            .write(|write| Box::pin(async move { store.update_station_key(write, patch).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn replace_station_key_secret(
        &self,
        station_id: &str,
        station_key_id: &str,
        plaintext: SecretBytes,
    ) -> Result<SavedSecretRef, ApplicationError> {
        let store = self.store;
        if plaintext.is_empty() {
            let mut read = self.runtime.begin_read().await?;
            let secret_id = store
                .station_key_secret_id_for_station(&mut read, station_id, station_key_id)
                .await?
                .ok_or(ApplicationError::NotFound)?;
            let station_key = store.station_key_by_id(&mut read, station_key_id).await?;
            let secret_ref = SecretRef {
                id: secret_id,
                scope: "station_key".to_string(),
                owner_id: station_key.id.clone(),
                kind: "api_key".to_string(),
            };
            return Ok(SavedSecretRef {
                secret_ref,
                station_key,
            });
        }

        let now = self.now_ms_string();
        let mut secret_ref = station_key_secret_ref(&self.ids.next_id(), station_key_id);
        let encrypted_secret = self.vault.encrypt(&secret_ref.aad(), plaintext)?;
        let row = encrypted_secret_row(secret_ref.clone(), encrypted_secret, now);
        let station_id = station_id.to_string();
        let station_key_id = station_key_id.to_string();
        let (actual_secret_id, station_key) = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .replace_station_key_secret(write, &station_id, &station_key_id, row)
                        .await
                })
            })
            .await?;
        secret_ref.id = actual_secret_id;
        Ok(SavedSecretRef {
            secret_ref,
            station_key,
        })
    }

    pub(crate) async fn reorder_station_keys(
        &self,
        station_id: String,
        station_key_ids: Vec<String>,
    ) -> Result<Vec<StationKey>, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .reorder_station_keys(write, &station_id, &station_key_ids, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn upsert_remote_station_key(
        &self,
        row: NewRemoteStationKeyRow,
    ) -> Result<RemoteStationKey, ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.upsert_remote_station_key(write, row).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn bind_remote_station_key(
        &self,
        remote_key_id: String,
        station_key_id: String,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .bind_remote_station_key(write, &remote_key_id, &station_key_id, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    fn encrypt_station_key_secret(
        &self,
        station_key_id: &str,
        plaintext: SecretBytes,
        now: &str,
    ) -> Result<Option<EncryptedSecretRow>, CredentialError> {
        if plaintext.is_empty() {
            return Ok(None);
        }
        let secret_ref = station_key_secret_ref(&self.ids.next_id(), station_key_id);
        let encrypted_secret = self.vault.encrypt(&secret_ref.aad(), plaintext)?;
        Ok(Some(encrypted_secret_row(
            secret_ref,
            encrypted_secret,
            now.to_string(),
        )))
    }

    fn now_ms_string(&self) -> String {
        self.clock.now_utc().timestamp_millis().to_string()
    }
}

fn station_key_secret_ref(secret_id: &str, station_key_id: &str) -> SecretRef {
    SecretRef {
        id: secret_id.to_string(),
        scope: "station_key".to_string(),
        owner_id: station_key_id.to_string(),
        kind: "api_key".to_string(),
    }
}

fn encrypted_secret_row(
    secret_ref: SecretRef,
    encrypted_secret: EncryptedSecret,
    now: String,
) -> EncryptedSecretRow {
    EncryptedSecretRow {
        id: secret_ref.id,
        scope: secret_ref.scope,
        owner_id: secret_ref.owner_id,
        kind: secret_ref.kind,
        masked_value: encrypted_secret.masked_value,
        ciphertext: encrypted_secret.ciphertext,
        nonce: encrypted_secret.nonce,
        now,
    }
}

pub(crate) fn secret_aad(scope: &str, owner_id: &str, kind: &str) -> String {
    format!("{scope}:{owner_id}:{kind}")
}
