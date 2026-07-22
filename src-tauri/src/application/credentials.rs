use std::{fmt, sync::Arc};

use zeroize::Zeroizing;

use crate::{
    application::{clock::Clock, error::ApplicationError, ids::IdGenerator},
    models::{
        credentials::{
            token_is_fresh, PersistStationSessionInput, ResolvedSession, SessionResolveStatus,
            StationCredentials, StationSessionCredentialKind, UpdateStationCredentialsInput,
            UpdateStationSessionInput,
        },
        group_facts::UpdateStationKeyGroupBindingInput,
        remote_keys::{RemoteKeyCapability, RemoteKeyMatchStatus, RemoteStationKey},
        routing::{StationKeyCapabilities, UpdateStationKeyCapabilitiesInput},
        shared_capabilities::{
            SaveStationKeyMode, SaveStationKeyWithDefaultsInput, SaveStationKeyWithDefaultsResult,
            StationKeyGroupSelectionKind,
        },
        station_keys::{CreateStationKeyInput, KeyPoolItem, StationKey, UpdateStationKeyInput},
    },
    persistence::{
        runtime::PersistenceHandle,
        stores::credential_store::{
            CredentialStore, EncryptedSecretRow, NewRemoteStationKeyRow, NewStationKeyRow,
            StationCredentialPatch, StationKeyPatch, StationSessionPatch, StoredEncryptedSecret,
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
#[allow(
    dead_code,
    reason = "returned by the released-schema differential integration contract"
)]
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

enum PreparedRemoteLocalKey {
    Create(NewStationKeyRow),
    Update(StationKeyPatch),
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

    pub(crate) async fn save_station_key_with_defaults(
        &self,
        input: SaveStationKeyWithDefaultsInput,
    ) -> Result<SaveStationKeyWithDefaultsResult, ApplicationError> {
        validate_group_selection(&input)?;
        let mode = input.mode.clone();
        let key_id = match &mode {
            SaveStationKeyMode::Create => self.ids.next_id(),
            SaveStationKeyMode::Update => input
                .id
                .as_deref()
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(ToString::to_string)
                .ok_or(ApplicationError::ConstraintViolation)?,
        };
        let now = self.now_ms_string();
        let plaintext = input
            .api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToString::to_string);
        if mode == SaveStationKeyMode::Create && plaintext.is_none() {
            return Err(ApplicationError::ConstraintViolation);
        }
        let encrypted_secret = plaintext
            .map(SecretBytes::from)
            .map(|secret| self.encrypt_station_key_secret(&key_id, secret, &now))
            .transpose()?
            .flatten();
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let mut station_key = match &mode {
                        SaveStationKeyMode::Create => {
                            let created = store
                                .insert_station_key(
                                    write,
                                    NewStationKeyRow {
                                        id: key_id.clone(),
                                        station_id: input.station_id.clone(),
                                        name: input.name.clone(),
                                        encrypted_secret,
                                        enabled: input.enabled,
                                        priority: input.priority,
                                        max_concurrency: None,
                                        load_factor: None,
                                        schedulable: input.schedulable,
                                        group_name: None,
                                        tier_label: input.tier_label.clone(),
                                        group_binding_id: None,
                                        group_id_hash: None,
                                        rate_multiplier: None,
                                        manual_rate_multiplier: None,
                                        manual_rate_updated_at: None,
                                        rate_source: None,
                                        balance_scope: input.balance_scope.clone(),
                                        note: input.note.clone(),
                                        now: now.clone(),
                                    },
                                )
                                .await?;
                            if let Some(status) = input.status.clone() {
                                store
                                    .update_station_key(
                                        write,
                                        station_key_patch_from_existing(
                                            &created,
                                            None,
                                            created.priority,
                                            created.schedulable,
                                            status,
                                            input.note.clone(),
                                            now.clone(),
                                        ),
                                    )
                                    .await?
                            } else {
                                created
                            }
                        }
                        SaveStationKeyMode::Update => {
                            let existing = store
                                .station_key_by_id_for_write(write, &key_id)
                                .await?;
                            let patch = StationKeyPatch {
                                id: existing.id.clone(),
                                station_id: input.station_id.clone(),
                                name: input.name.clone(),
                                encrypted_secret,
                                enabled: input.enabled,
                                priority: input.priority.unwrap_or(existing.priority),
                                max_concurrency: existing.max_concurrency,
                                load_factor: existing.load_factor,
                                schedulable: input.schedulable.unwrap_or(existing.schedulable),
                                group_name: existing.group_name,
                                tier_label: input.tier_label.clone(),
                                group_binding_id: existing.group_binding_id,
                                group_id_hash: existing.group_id_hash,
                                rate_multiplier: existing.rate_multiplier,
                                manual_rate_multiplier: None,
                                manual_rate_updated_at: None,
                                rate_source: existing.rate_source,
                                balance_scope: input.balance_scope.clone().or(existing.balance_scope),
                                status: input.status.clone().unwrap_or(existing.status),
                                note: input.note.clone(),
                                now: now.clone(),
                            };
                            store.update_station_key(write, patch).await?
                        }
                    };
                    station_key = match input.group_selection.kind {
                        StationKeyGroupSelectionKind::Keep => station_key,
                        StationKeyGroupSelectionKind::Clear => {
                            store
                                .clear_station_key_group_binding(write, &station_key.id, &now)
                                .await?
                        }
                        StationKeyGroupSelectionKind::Set => {
                            store
                                .update_station_key_group_binding(
                                    write,
                                    UpdateStationKeyGroupBindingInput {
                                        station_key_id: station_key.id.clone(),
                                        group_binding_id: input
                                            .group_selection
                                            .group_binding_id
                                            .clone()
                                            .ok_or(
                                                crate::persistence::error::PersistenceError::ConstraintViolation,
                                            )?,
                                    },
                                    &now,
                                )
                                .await?
                        }
                    };
                    let capabilities = match (mode, input.capabilities) {
                        (SaveStationKeyMode::Create, capabilities) => {
                            let mut capabilities = capabilities.unwrap_or_else(|| {
                                default_station_key_capabilities_input(station_key.id.clone())
                            });
                            capabilities.station_key_id = station_key.id.clone();
                            store
                                .update_station_key_capabilities(write, capabilities, &now)
                                .await?
                        }
                        (SaveStationKeyMode::Update, Some(mut capabilities)) => {
                            capabilities.station_key_id = station_key.id.clone();
                            store
                                .update_station_key_capabilities(write, capabilities, &now)
                                .await?
                        }
                        (SaveStationKeyMode::Update, None) => {
                            store
                                .station_key_capabilities_for_write(write, &station_key.id)
                                .await?
                        }
                    };
                    Ok(SaveStationKeyWithDefaultsResult {
                        station_key,
                        capabilities,
                        message: "Station Key saved".to_string(),
                    })
                })
            })
            .await
            .map_err(Into::into)
    }

    #[allow(
        dead_code,
        reason = "exercised by the released-schema differential integration contract"
    )]
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

    pub(crate) async fn delete_station_key(
        &self,
        station_key_id: String,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.delete_station_key(write, &station_key_id).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn reorder_key_pool(
        &self,
        station_key_ids: Vec<String>,
    ) -> Result<Vec<KeyPoolItem>, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move { store.reorder_key_pool(write, &station_key_ids, &now).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_station_key_group_binding(
        &self,
        input: UpdateStationKeyGroupBindingInput,
    ) -> Result<StationKey, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .update_station_key_group_binding(write, input, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn list_key_pool_items(&self) -> Result<Vec<KeyPoolItem>, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        store
            .list_key_pool_items(&mut read)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_station_credentials(
        &self,
        station_id: String,
    ) -> Result<StationCredentials, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        store
            .station_credentials(&mut read, &station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn resolve_station_key_secret(
        &self,
        station_key_id: String,
    ) -> Result<SecretBytes, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        let secret = store.station_key_secret(&mut read, &station_key_id).await?;
        self.decrypt_stored_secret(secret).map_err(Into::into)
    }

    pub(crate) async fn get_station_login_password(
        &self,
        station_id: String,
    ) -> Result<Option<SecretBytes>, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        let secret = store
            .station_credential_secret(&mut read, &station_id, "login_password")
            .await?;
        secret
            .map(|secret| self.decrypt_stored_secret(secret))
            .transpose()
            .map_err(Into::into)
    }

    pub(crate) async fn resolve_station_session(
        &self,
        station_id: String,
        now_ms: i64,
    ) -> Result<ResolvedSession, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        let credentials = store.station_credentials(&mut read, &station_id).await?;
        let access_token = self.decrypt_optional_stored_secret(
            store
                .station_credential_secret(&mut read, &station_id, "access_token")
                .await?,
        )?;
        let refresh_token = self.decrypt_optional_stored_secret(
            store
                .station_credential_secret(&mut read, &station_id, "refresh_token")
                .await?,
        )?;
        let cookie = self.decrypt_optional_stored_secret(
            store
                .station_credential_secret(&mut read, &station_id, "cookie")
                .await?,
        )?;
        let access_token = secret_string(access_token)?;
        let refresh_token = secret_string(refresh_token)?;
        let cookie = secret_string(cookie)?;
        let password_login_available = credentials
            .login_username
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
            && credentials.password_present;
        let access_token_ready = access_token.is_some()
            && (token_is_fresh(credentials.token_expires_at.as_deref(), now_ms)
                || (credentials.token_expires_at.is_none()
                    && matches!(
                        credentials.session_source.as_str(),
                        "manual_token" | "webview_capture"
                    )));
        let cookie_ready = cookie.is_some()
            && credentials
                .newapi_user_id
                .as_deref()
                .is_some_and(|value| !value.trim().is_empty());
        if access_token_ready || cookie_ready {
            return Ok(ResolvedSession {
                status: SessionResolveStatus::Ready,
                access_token,
                refresh_token,
                cookie,
                newapi_user_id: credentials.newapi_user_id,
                message: None,
            });
        }
        if refresh_token.is_some() || password_login_available {
            return Ok(ResolvedSession {
                status: SessionResolveStatus::Ready,
                access_token,
                refresh_token,
                cookie,
                newapi_user_id: credentials.newapi_user_id,
                message: Some("session refresh or login is required".to_string()),
            });
        }
        Ok(ResolvedSession::manual_required(
            "no usable station session credentials",
        ))
    }

    pub(crate) async fn update_station_credentials(
        &self,
        input: UpdateStationCredentialsInput,
    ) -> Result<StationCredentials, ApplicationError> {
        let now = self.now_ms_string();
        let password_secret = if input.remember_password {
            input
                .login_password
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| {
                    self.encrypt_station_secret(&input.station_id, "login_password", value, &now)
                })
                .transpose()?
        } else {
            None
        };
        let patch = StationCredentialPatch {
            station_id: input.station_id,
            login_username: input.login_username,
            remember_password: input.remember_password,
            password_secret,
            now,
        };
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.update_station_credentials(write, patch).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_station_session(
        &self,
        input: UpdateStationSessionInput,
    ) -> Result<StationCredentials, ApplicationError> {
        self.persist_station_session(PersistStationSessionInput {
            station_id: input.station_id,
            access_token: input.access_token,
            refresh_token: input.refresh_token,
            cookie: input.cookie,
            newapi_user_id: input.newapi_user_id,
            session_expires_at: input.token_expires_at.clone(),
            token_expires_at: input.token_expires_at,
            session_source: "manual_token".to_string(),
        })
        .await
    }

    pub(crate) async fn update_station_session_if_revision(
        &self,
        input: UpdateStationSessionInput,
        expected_revision: i64,
    ) -> Result<StationCredentials, ApplicationError> {
        self.persist_station_session_if_revision(
            PersistStationSessionInput {
                station_id: input.station_id,
                access_token: input.access_token,
                refresh_token: input.refresh_token,
                cookie: input.cookie,
                newapi_user_id: input.newapi_user_id,
                session_expires_at: input.token_expires_at.clone(),
                token_expires_at: input.token_expires_at,
                session_source: "manual_token".to_string(),
            },
            expected_revision,
        )
        .await
    }

    pub(crate) async fn persist_station_session(
        &self,
        input: PersistStationSessionInput,
    ) -> Result<StationCredentials, ApplicationError> {
        let patch = self.build_station_session_patch(input)?;
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.update_station_session(write, patch).await })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn persist_station_session_if_revision(
        &self,
        input: PersistStationSessionInput,
        expected_revision: i64,
    ) -> Result<StationCredentials, ApplicationError> {
        let patch = self.build_station_session_patch(input)?;
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .update_station_session_if_revision(write, patch, expected_revision)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn invalidate_station_session_credential(
        &self,
        station_id: String,
        kind: StationSessionCredentialKind,
    ) -> Result<(), ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .invalidate_station_session_credential(write, &station_id, kind, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn clear_station_credentials(
        &self,
        station_id: String,
    ) -> Result<StationCredentials, ApplicationError> {
        let store = self.store;
        self.runtime
            .write(|write| {
                Box::pin(async move { store.clear_station_credentials(write, &station_id).await })
            })
            .await
            .map_err(Into::into)
    }

    #[allow(
        dead_code,
        reason = "exercised by the released-schema differential integration contract"
    )]
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

    pub(crate) async fn replace_remote_station_keys_and_metadata(
        &self,
        station_id: String,
        expected_endpoint_revision: i64,
        remote_keys: Vec<RemoteStationKey>,
        station_key_updates: Vec<UpdateStationKeyInput>,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        if station_key_updates
            .iter()
            .any(|input| input.api_key.is_some() || input.station_id != station_id)
        {
            return Err(ApplicationError::ConstraintViolation);
        }
        let now = self.now_ms_string();
        let patches = station_key_updates
            .into_iter()
            .map(|input| station_key_patch(input, None, now.clone()))
            .collect::<Vec<_>>();
        let store = self.store;
        self.runtime
            .write(move |write| {
                Box::pin(async move {
                    store
                        .assert_station_endpoint_revision(
                            write,
                            &station_id,
                            expected_endpoint_revision,
                        )
                        .await?;
                    for patch in patches {
                        store.update_station_key(write, patch).await?;
                    }
                    store
                        .replace_remote_station_keys(write, &station_id, &remote_keys, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn save_remote_station_key_with_local(
        &self,
        mut remote_key: RemoteStationKey,
        expected_endpoint_revision: i64,
        matched_station_key_update: Option<UpdateStationKeyInput>,
        new_group_binding_id: Option<String>,
        full_key: String,
    ) -> Result<(RemoteStationKey, StationKey), ApplicationError> {
        let full_key = full_key.trim().to_string();
        if full_key.is_empty() {
            return Err(ApplicationError::SecretValidationFailed);
        }
        let now = self.now_ms_string();
        let store = self.store;
        let prepared = if let Some(input) = matched_station_key_update {
            if input.api_key.is_some() || input.station_id != remote_key.station_id {
                return Err(ApplicationError::ConstraintViolation);
            }
            PreparedRemoteLocalKey::Update(station_key_patch(input, None, now.clone()))
        } else {
            let station_key_id = self.ids.next_id();
            let encrypted_secret = self.encrypt_station_key_secret(
                &station_key_id,
                SecretBytes::from(full_key),
                &now,
            )?;
            PreparedRemoteLocalKey::Create(NewStationKeyRow {
                id: station_key_id,
                station_id: remote_key.station_id.clone(),
                name: remote_key
                    .remote_key_name
                    .clone()
                    .unwrap_or_else(|| "远端 Key".to_string()),
                encrypted_secret,
                enabled: true,
                priority: None,
                max_concurrency: None,
                load_factor: None,
                schedulable: None,
                group_name: remote_key.group_name.clone(),
                tier_label: remote_key.tier_label.clone(),
                group_binding_id: new_group_binding_id,
                group_id_hash: remote_key.group_id_hash.clone(),
                rate_multiplier: remote_key.rate_multiplier,
                manual_rate_multiplier: None,
                manual_rate_updated_at: None,
                rate_source: remote_key.rate_source.clone(),
                balance_scope: None,
                note: Some("由远端站点创建并同步。".to_string()),
                now: now.clone(),
            })
        };
        self.runtime
            .write(move |write| {
                Box::pin(async move {
                    store
                        .assert_station_endpoint_revision(
                            write,
                            &remote_key.station_id,
                            expected_endpoint_revision,
                        )
                        .await?;
                    let created = matches!(&prepared, PreparedRemoteLocalKey::Create(_));
                    let station_key = match prepared {
                        PreparedRemoteLocalKey::Create(row) => {
                            store.insert_station_key(write, row).await?
                        }
                        PreparedRemoteLocalKey::Update(patch) => {
                            store.update_station_key(write, patch).await?
                        }
                    };
                    remote_key.matched_station_key_id = Some(station_key.id.clone());
                    if created {
                        remote_key.match_status = RemoteKeyMatchStatus::Matched;
                        remote_key.match_confidence = 1.0;
                    }
                    remote_key.collected_at = now.clone();
                    let remote_key = store
                        .save_remote_station_key_snapshot(write, &remote_key, &now)
                        .await?;
                    Ok((remote_key, station_key))
                })
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

    pub(crate) async fn list_remote_station_keys(
        &self,
        station_id: String,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        store
            .list_remote_station_keys(&mut read, &station_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_remote_key_capability(
        &self,
        station_id: String,
    ) -> Result<RemoteKeyCapability, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        let station_type = store.station_type(&mut read, &station_id).await?;
        let supported = matches!(station_type.as_str(), "sub2api" | "newapi");
        Ok(RemoteKeyCapability {
            station_id,
            station_type: station_type.clone(),
            can_list_remote_keys: supported,
            can_create_remote_key: supported,
            can_read_groups: supported,
            requires_manual_session: supported,
            unsupported_reason: (!supported)
                .then(|| format!("remote key management is not supported for {station_type}")),
        })
    }

    pub(crate) async fn unbind_remote_station_key(
        &self,
        remote_key_id: String,
        station_id: String,
    ) -> Result<Vec<RemoteStationKey>, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .unbind_remote_station_key(write, &remote_key_id, &station_id, &now)
                        .await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn get_station_key_capabilities(
        &self,
        station_key_id: String,
    ) -> Result<StationKeyCapabilities, ApplicationError> {
        let store = self.store;
        let mut read = self.runtime.begin_read().await?;
        store
            .station_key_capabilities(&mut read, &station_key_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn update_station_key_capabilities(
        &self,
        input: UpdateStationKeyCapabilitiesInput,
    ) -> Result<StationKeyCapabilities, ApplicationError> {
        let store = self.store;
        let now = self.now_ms_string();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    store
                        .update_station_key_capabilities(write, input, &now)
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

    fn encrypt_optional_station_secret(
        &self,
        station_id: &str,
        kind: &str,
        plaintext: Option<String>,
        now: &str,
    ) -> Result<Option<EncryptedSecretRow>, CredentialError> {
        plaintext
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|value| self.encrypt_station_secret(station_id, kind, &value, now))
            .transpose()
    }

    fn build_station_session_patch(
        &self,
        input: PersistStationSessionInput,
    ) -> Result<StationSessionPatch, CredentialError> {
        let now = self.now_ms_string();
        let access_token_secret = self.encrypt_optional_station_secret(
            &input.station_id,
            "access_token",
            input.access_token,
            &now,
        )?;
        let refresh_token_secret = self.encrypt_optional_station_secret(
            &input.station_id,
            "refresh_token",
            input.refresh_token,
            &now,
        )?;
        let cookie_secret =
            self.encrypt_optional_station_secret(&input.station_id, "cookie", input.cookie, &now)?;
        Ok(StationSessionPatch {
            station_id: input.station_id,
            access_token_secret,
            refresh_token_secret,
            cookie_secret,
            newapi_user_id: input.newapi_user_id,
            token_expires_at: input.token_expires_at,
            session_expires_at: input.session_expires_at,
            session_source: input.session_source,
            now,
        })
    }

    fn encrypt_station_secret(
        &self,
        station_id: &str,
        kind: &str,
        plaintext: &str,
        now: &str,
    ) -> Result<EncryptedSecretRow, CredentialError> {
        let secret_ref = SecretRef {
            id: self.ids.next_id(),
            scope: "station_credentials".to_string(),
            owner_id: station_id.to_string(),
            kind: kind.to_string(),
        };
        let encrypted_secret = self
            .vault
            .encrypt(&secret_ref.aad(), SecretBytes::from(plaintext.to_string()))?;
        Ok(encrypted_secret_row(
            secret_ref,
            encrypted_secret,
            now.to_string(),
        ))
    }

    fn decrypt_optional_stored_secret(
        &self,
        secret: Option<StoredEncryptedSecret>,
    ) -> Result<Option<SecretBytes>, CredentialError> {
        secret
            .map(|secret| self.decrypt_stored_secret(secret))
            .transpose()
    }

    fn decrypt_stored_secret(
        &self,
        secret: StoredEncryptedSecret,
    ) -> Result<SecretBytes, CredentialError> {
        let secret_ref = SecretRef {
            id: secret.id,
            scope: secret.scope,
            owner_id: secret.owner_id,
            kind: secret.kind,
        };
        self.vault.decrypt(
            &secret_ref.aad(),
            &EncryptedSecret {
                ciphertext: secret.ciphertext,
                nonce: secret.nonce,
                masked_value: secret.masked_value,
            },
        )
    }

    fn now_ms_string(&self) -> String {
        self.clock.now_utc().timestamp_millis().to_string()
    }
}

fn station_key_patch(
    input: UpdateStationKeyInput,
    encrypted_secret: Option<EncryptedSecretRow>,
    now: String,
) -> StationKeyPatch {
    let manual_rate_updated_at = input
        .manual_rate_multiplier
        .as_ref()
        .and_then(|value| value.as_ref())
        .map(|_| now.clone());
    StationKeyPatch {
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
        manual_rate_updated_at,
        rate_source: input.rate_source,
        balance_scope: input.balance_scope,
        status: input.status,
        note: input.note,
        now,
    }
}

fn secret_string(secret: Option<SecretBytes>) -> Result<Option<String>, CredentialError> {
    secret
        .map(|secret| {
            String::from_utf8(secret.as_bytes().to_vec())
                .map_err(|_| CredentialError::SecretValidationFailed)
        })
        .transpose()
}

fn validate_group_selection(
    input: &SaveStationKeyWithDefaultsInput,
) -> Result<(), ApplicationError> {
    match input.group_selection.kind {
        StationKeyGroupSelectionKind::Keep if input.mode == SaveStationKeyMode::Create => {
            Err(ApplicationError::ConstraintViolation)
        }
        StationKeyGroupSelectionKind::Set
            if input
                .group_selection
                .group_binding_id
                .as_deref()
                .is_none_or(|id| id.trim().is_empty()) =>
        {
            Err(ApplicationError::ConstraintViolation)
        }
        _ => Ok(()),
    }
}

fn station_key_patch_from_existing(
    existing: &StationKey,
    encrypted_secret: Option<EncryptedSecretRow>,
    priority: i64,
    schedulable: bool,
    status: String,
    note: Option<String>,
    now: String,
) -> StationKeyPatch {
    StationKeyPatch {
        id: existing.id.clone(),
        station_id: existing.station_id.clone(),
        name: existing.name.clone(),
        encrypted_secret,
        enabled: existing.enabled,
        priority,
        max_concurrency: existing.max_concurrency,
        load_factor: existing.load_factor,
        schedulable,
        group_name: existing.group_name.clone(),
        tier_label: existing.tier_label.clone(),
        group_binding_id: existing.group_binding_id.clone(),
        group_id_hash: existing.group_id_hash.clone(),
        rate_multiplier: existing.rate_multiplier,
        manual_rate_multiplier: None,
        manual_rate_updated_at: None,
        rate_source: existing.rate_source.clone(),
        balance_scope: existing.balance_scope.clone(),
        status,
        note,
        now,
    }
}

fn default_station_key_capabilities_input(
    station_key_id: String,
) -> UpdateStationKeyCapabilitiesInput {
    UpdateStationKeyCapabilitiesInput {
        station_key_id,
        supports_chat_completions: true,
        supports_responses: true,
        supports_embeddings: true,
        supports_stream: true,
        supports_tools: true,
        supports_vision: true,
        supports_reasoning: true,
        model_allowlist: Vec::new(),
        model_blocklist: Vec::new(),
        preferred_models: Vec::new(),
        only_use_as_backup: false,
        routing_tags: Vec::new(),
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
