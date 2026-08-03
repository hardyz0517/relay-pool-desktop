use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use sha2::{Digest, Sha256};

use crate::{
    application::{
        clock::Clock,
        credentials::{CredentialError, CredentialVault, EncryptedSecret, SecretBytes, SecretRef},
        error::ApplicationError,
        ids::IdGenerator,
    },
    models::{
        credentials::{
            PersistStationSessionInput, ResolvedSession, SessionResolveStatus, StationCredentials,
        },
        group_facts::{StationGroupBinding, BINDING_KIND_STATION_GROUP, BINDING_STATUS_AVAILABLE},
        provider_drafts::{
            CommitProviderDraftInput, CreateProviderDraftInput, PatchProviderDraftInput,
            ProviderDraft, ProviderDraftPayload, ProviderDraftPreview,
        },
        station_keys::StationKey,
        stations::{CreateStationInput, Station},
    },
    persistence::{
        error::PersistenceError,
        runtime::PersistenceHandle,
        stores::{
            collector_store::{CollectorStore, StationGroupBindingWrite},
            credential_store::{
                CredentialStore, EncryptedSecretRow, NewStationKeyRow, StationCredentialPatch,
                StationSessionPatch, StoredEncryptedSecret,
            },
            provider_draft_store::{
                draft_key_secret_kind, NewProviderDraftRow, ProviderDraftStore,
            },
            station_catalog::{NewStationRow, StationCatalogStore},
        },
    },
};

const DRAFT_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1_000;
const DRAFT_SECRET_SCOPE: &str = "provider_draft";

#[derive(Clone)]
pub(crate) struct ProviderDraftService {
    runtime: PersistenceHandle,
    vault: Arc<dyn CredentialVault>,
    clock: Arc<dyn Clock>,
    ids: Arc<dyn IdGenerator>,
    drafts: ProviderDraftStore,
    stations: StationCatalogStore,
    credentials: CredentialStore,
    collectors: CollectorStore,
}

impl ProviderDraftService {
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
            drafts: ProviderDraftStore,
            stations: StationCatalogStore,
            credentials: CredentialStore,
            collectors: CollectorStore,
        }
    }

    pub(crate) async fn create_or_resume(
        &self,
        input: CreateProviderDraftInput,
    ) -> Result<ProviderDraft, ApplicationError> {
        validate_draft_payload(&input.payload)?;
        let now = self.now_ms();
        let is_create_draft = input.base_station_id.is_none();
        if is_create_draft {
            let mut read = self.runtime.begin_read().await?;
            if let Some(existing) = self.drafts.latest_active_create(&mut read, now).await? {
                return Ok(existing);
            }
        }
        let row = NewProviderDraftRow {
            id: self.ids.next_id(),
            base_station_id: input.base_station_id,
            payload: input.payload,
            now: now.to_string(),
            expires_at: (now + DRAFT_TTL_MS).to_string(),
        };
        let drafts = self.drafts;
        let result = self
            .runtime
            .write(|write| {
                Box::pin(async move {
                    drafts.delete_expired(write, now).await?;
                    drafts.insert(write, row).await
                })
            })
            .await;
        match result {
            Ok(draft) => Ok(draft),
            Err(PersistenceError::ConstraintViolation) if is_create_draft => {
                let mut read = self.runtime.begin_read().await?;
                self.drafts
                    .latest_active_create(&mut read, now)
                    .await?
                    .ok_or(ApplicationError::ConstraintViolation)
            }
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) async fn get(&self, draft_id: String) -> Result<ProviderDraft, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        self.drafts
            .get(&mut read, &draft_id)
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn patch(
        &self,
        input: PatchProviderDraftInput,
    ) -> Result<ProviderDraft, ApplicationError> {
        validate_draft_payload(&input.payload)?;
        let now = self.now_ms();
        let expires_at = (now + DRAFT_TTL_MS).to_string();
        let encrypted_station_api_key = self.prepare_secret_patch(
            &input.draft_id,
            "station_api_key",
            input.station_api_key.as_deref(),
            now,
        )?;
        let encrypted_login_password = self.prepare_secret_patch(
            &input.draft_id,
            "login_password",
            input.login_password.as_deref(),
            now,
        )?;
        let mut key_secret_patches = Vec::with_capacity(input.key_api_keys.len());
        for secret in &input.key_api_keys {
            validate_identifier(&secret.client_id)?;
            let kind = draft_key_secret_kind(&secret.client_id);
            key_secret_patches.push((
                kind.clone(),
                self.prepare_secret_patch(&input.draft_id, &kind, Some(&secret.api_key), now)?,
            ));
        }
        let drafts = self.drafts;
        let draft_id = input.draft_id;
        let expected_revision = input.expected_revision;
        let payload = input.payload;
        let retained_key_client_ids = payload
            .keys
            .iter()
            .map(|key| key.client_id.clone())
            .collect::<HashSet<_>>();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    drafts
                        .patch_payload(
                            write,
                            &draft_id,
                            expected_revision,
                            &payload,
                            &now.to_string(),
                            &expires_at,
                        )
                        .await?;
                    apply_secret_patch(
                        drafts,
                        write,
                        &draft_id,
                        "station_api_key",
                        encrypted_station_api_key,
                    )
                    .await?;
                    apply_secret_patch(
                        drafts,
                        write,
                        &draft_id,
                        "login_password",
                        encrypted_login_password,
                    )
                    .await?;
                    for (kind, patch) in key_secret_patches {
                        apply_secret_patch(drafts, write, &draft_id, &kind, patch).await?;
                    }
                    drafts
                        .delete_key_secrets_not_in(write, &draft_id, &retained_key_client_ids)
                        .await?;
                    drafts.get_for_write(write, &draft_id).await
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn discard(&self, draft_id: String) -> Result<(), ApplicationError> {
        let drafts = self.drafts;
        self.runtime
            .write(|write| Box::pin(async move { drafts.discard(write, &draft_id).await }))
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn store_preview(
        &self,
        preview: ProviderDraftPreview,
    ) -> Result<ProviderDraftPreview, ApplicationError> {
        let current = self.get(preview.draft_id.clone()).await?;
        if current.state != "active"
            || runtime_fingerprint(&current.payload) != preview.runtime_fingerprint
        {
            return Err(ApplicationError::StaleRevision);
        }
        let drafts = self.drafts;
        let now = self.now_ms().to_string();
        let stored = preview.clone();
        self.runtime
            .write(|write| {
                Box::pin(async move { drafts.upsert_preview(write, &stored, &now).await })
            })
            .await?;
        Ok(preview)
    }

    pub(crate) async fn commit(
        &self,
        input: CommitProviderDraftInput,
    ) -> Result<Station, ApplicationError> {
        validate_identifier(&input.commit_key)?;
        let draft = self.get(input.draft_id.clone()).await?;
        if draft.state == "committed" {
            let mut read = self.runtime.begin_read().await?;
            let station_id = self
                .drafts
                .committed_station_for_key(&mut read, &draft.id, &input.commit_key)
                .await?
                .ok_or(ApplicationError::StaleRevision)?;
            return self
                .stations
                .get(&mut read, &station_id)
                .await
                .map_err(Into::into);
        }
        if draft.base_station_id.is_some() || draft.revision != input.expected_revision {
            return Err(ApplicationError::StaleRevision);
        }
        validate_committable_payload(&draft.payload)?;

        let now = self.now_ms().to_string();
        let station_id = self.ids.next_id();
        let station_api_key = self.secret_text(&draft.id, "station_api_key").await?;
        let login_password = self.secret_text(&draft.id, "login_password").await?;
        let access_token = self.secret_text(&draft.id, "access_token").await?;
        let refresh_token = self.secret_text(&draft.id, "refresh_token").await?;
        let cookie = self.secret_text(&draft.id, "cookie").await?;
        let newapi_user_id = self.secret_text(&draft.id, "newapi_user_id").await?;
        let token_expires_at = self.secret_text(&draft.id, "token_expires_at").await?;
        let session_expires_at = self.secret_text(&draft.id, "session_expires_at").await?;

        let mut key_plaintexts = HashMap::new();
        for key in &draft.payload.keys {
            if let Some(value) = self
                .secret_text(&draft.id, &draft_key_secret_kind(&key.client_id))
                .await?
            {
                key_plaintexts.insert(key.client_id.clone(), value);
            }
        }
        if let Some(value) = station_api_key {
            key_plaintexts.insert("__station_default__".to_string(), value);
        }

        let login_password_secret = login_password
            .as_deref()
            .map(|value| {
                self.encrypt_secret_for(
                    "station_credentials",
                    &station_id,
                    "login_password",
                    value,
                    &now,
                )
            })
            .transpose()?;
        let session_patch = StationSessionPatch {
            station_id: station_id.clone(),
            access_token_secret: access_token
                .as_deref()
                .map(|value| {
                    self.encrypt_secret_for(
                        "station_credentials",
                        &station_id,
                        "access_token",
                        value,
                        &now,
                    )
                })
                .transpose()?,
            refresh_token_secret: refresh_token
                .as_deref()
                .map(|value| {
                    self.encrypt_secret_for(
                        "station_credentials",
                        &station_id,
                        "refresh_token",
                        value,
                        &now,
                    )
                })
                .transpose()?,
            cookie_secret: cookie
                .as_deref()
                .map(|value| {
                    self.encrypt_secret_for(
                        "station_credentials",
                        &station_id,
                        "cookie",
                        value,
                        &now,
                    )
                })
                .transpose()?,
            newapi_user_id,
            token_expires_at,
            session_expires_at,
            session_source: "draft_authorization".to_string(),
            now: now.clone(),
        };

        let station_row = NewStationRow {
            id: station_id.clone(),
            now: now.clone(),
            input: CreateStationInput {
                name: draft.payload.name.clone(),
                station_type: draft.payload.station_type.clone(),
                website_url: draft.payload.website_url.clone(),
                api_base_url: draft.payload.api_base_url.clone(),
                api_key: String::new(),
                collector_proxy_mode: draft.payload.collector_proxy_mode.clone(),
                collector_proxy_url: draft.payload.collector_proxy_url.clone(),
                enabled: draft.payload.enabled,
                credit_per_cny: draft.payload.credit_per_cny,
                low_balance_threshold_cny: draft.payload.low_balance_threshold_cny,
                collection_interval_minutes: draft.payload.collection_interval_minutes,
                note: draft.payload.note.clone(),
            },
        };
        let group_storage_ids = draft
            .payload
            .groups
            .iter()
            .map(|group| (group.client_id.clone(), self.ids.next_id()))
            .collect::<HashMap<_, _>>();

        let mut key_rows = Vec::new();
        for (priority, key) in draft.payload.keys.iter().enumerate() {
            let encrypted_secret = key_plaintexts
                .get(&key.client_id)
                .map(|value| {
                    let key_id = self.ids.next_id();
                    self.encrypt_secret_for("station_key", &key_id, "api_key", value, &now)
                        .map(|secret| (key_id, secret))
                })
                .transpose()?;
            let Some((key_id, encrypted_secret)) = encrypted_secret else {
                continue;
            };
            key_rows.push((
                key.clone(),
                NewStationKeyRow {
                    id: key_id,
                    station_id: station_id.clone(),
                    name: key.name.clone(),
                    encrypted_secret: Some(encrypted_secret),
                    enabled: key.enabled,
                    priority: Some(priority as i64),
                    max_concurrency: Some(3),
                    load_factor: None,
                    schedulable: Some(true),
                    group_name: key.group_name.clone(),
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: key.group_id_hash.clone(),
                    rate_multiplier: key.rate_multiplier,
                    manual_rate_multiplier: key.rate_multiplier,
                    manual_rate_updated_at: key.rate_multiplier.map(|_| now.clone()),
                    rate_source: key.rate_multiplier.map(|_| "manual".to_string()),
                    balance_scope: Some("station_key".to_string()),
                    note: key.note.clone(),
                    now: now.clone(),
                },
            ));
        }
        if let Some(value) = key_plaintexts.get("__station_default__") {
            let key_id = self.ids.next_id();
            key_rows.push((
                crate::models::provider_drafts::ProviderDraftKey {
                    client_id: "__station_default__".to_string(),
                    name: "Default Key".to_string(),
                    enabled: true,
                    group_client_id: None,
                    group_id_hash: None,
                    group_name: None,
                    rate_multiplier: None,
                    note: None,
                },
                NewStationKeyRow {
                    id: key_id.clone(),
                    station_id: station_id.clone(),
                    name: "Default Key".to_string(),
                    encrypted_secret: Some(self.encrypt_secret_for(
                        "station_key",
                        &key_id,
                        "api_key",
                        value,
                        &now,
                    )?),
                    enabled: true,
                    priority: Some(0),
                    max_concurrency: Some(3),
                    load_factor: None,
                    schedulable: Some(true),
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    manual_rate_multiplier: None,
                    manual_rate_updated_at: None,
                    rate_source: None,
                    balance_scope: Some("station_key".to_string()),
                    note: None,
                    now: now.clone(),
                },
            ));
        }

        let drafts = self.drafts;
        let stations = self.stations;
        let collectors = self.collectors;
        let credentials = self.credentials;
        let draft_id = draft.id.clone();
        let draft_groups = draft.payload.groups.clone();
        let login_username = draft.payload.login_username.clone();
        let remember_password = draft.payload.remember_password;
        let expected_revision = input.expected_revision;
        let commit_key = input.commit_key;
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let station = stations.insert(write, station_row).await?;
                    let mut group_ids = HashMap::new();
                    for group in draft_groups {
                        let group_key_hash = if group.group_key_hash.trim().is_empty() {
                            format!("manual:{:x}", Sha256::digest(group.group_name.trim().to_lowercase().as_bytes()))
                        } else {
                            group.group_key_hash.clone()
                        };
                        let stored = collectors
                            .upsert_station_group_binding(
                                write,
                                &StationGroupBindingWrite {
                                    id: group_storage_ids
                                        .get(&group.client_id)
                                        .cloned()
                                        .ok_or(crate::persistence::error::PersistenceError::InvariantViolation(
                                            "provider draft group storage identity is missing".to_string(),
                                        ))?,
                                    station_id: station_id.clone(),
                                    station_key_id: None,
                                    binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                                    parent_group_binding_id: None,
                                    group_key_hash,
                                    group_id_hash: group.group_id_hash.clone(),
                                    group_name: group.group_name.clone(),
                                    binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                                    default_rate_multiplier: (group.source == "remote").then_some(group.rate_multiplier).flatten(),
                                    user_rate_multiplier: (group.source == "manual").then_some(group.rate_multiplier).flatten(),
                                    effective_rate_multiplier: group.rate_multiplier,
                                    inferred_group_category: group.inferred_group_category.clone(),
                                    group_category_override: group.group_category_override.clone(),
                                    rate_source: Some(if group.source == "remote" { "draft_preview" } else { "manual" }.to_string()),
                                    confidence: if group.source == "remote" { 0.95 } else { 1.0 },
                                    last_seen_at: (group.source == "remote").then(|| now.clone()),
                                    raw_json_redacted: None,
                                    now: now.clone(),
                                },
                            )
                            .await?;
                        group_ids.insert(group.client_id, stored.binding.id);
                    }
                    for (key, mut row) in key_rows {
                        row.group_binding_id = key
                            .group_client_id
                            .as_ref()
                            .and_then(|client_id| group_ids.get(client_id).cloned());
                        credentials.insert_station_key(write, row).await?;
                    }
                    credentials
                        .update_station_credentials(
                            write,
                            StationCredentialPatch {
                                station_id: station_id.clone(),
                                login_username,
                                remember_password,
                                password_secret: login_password_secret,
                                now: now.clone(),
                            },
                        )
                        .await?;
                    if session_patch.access_token_secret.is_some()
                        || session_patch.refresh_token_secret.is_some()
                        || session_patch.cookie_secret.is_some()
                    {
                        credentials.update_station_session(write, session_patch).await?;
                    }
                    drafts
                        .mark_committed(
                            write,
                            &draft_id,
                            expected_revision,
                            &commit_key,
                            &station_id,
                            &now,
                        )
                        .await?;
                    drafts.delete_all_secrets(write, &draft_id).await?;
                    Ok(station)
                })
            })
            .await
            .map_err(Into::into)
    }

    pub(crate) async fn station_projection(
        &self,
        draft_id: &str,
    ) -> Result<Station, ApplicationError> {
        let draft = self.get(draft_id.to_string()).await?;
        if draft.state != "active" {
            return Err(ApplicationError::NotFound);
        }
        let now = draft.updated_at.clone();
        Ok(Station {
            id: draft.id,
            name: draft.payload.name,
            station_type: draft.payload.station_type,
            website_url: draft.payload.website_url,
            api_base_url: draft.payload.api_base_url,
            endpoint_revision: draft.revision,
            collector_proxy_mode: draft.payload.collector_proxy_mode,
            collector_proxy_url: draft.payload.collector_proxy_url,
            api_key_masked: String::new(),
            api_key_present: draft.station_api_key_present,
            key_count: draft.payload.keys.len() as i64,
            enabled: draft.payload.enabled,
            priority: 0,
            credit_per_cny: draft.payload.credit_per_cny,
            balance_raw: None,
            balance_cny: None,
            low_balance_threshold_cny: draft.payload.low_balance_threshold_cny,
            collection_interval_minutes: draft.payload.collection_interval_minutes,
            status: "unchecked".to_string(),
            latency_ms: None,
            last_checked_at: None,
            last_pricing_fetched_at: None,
            note: draft.payload.note,
            created_at: draft.created_at,
            updated_at: now,
        })
    }

    pub(crate) async fn credentials_projection(
        &self,
        draft_id: &str,
    ) -> Result<StationCredentials, ApplicationError> {
        let draft = self.get(draft_id.to_string()).await?;
        Ok(StationCredentials {
            station_id: draft.id,
            login_username: draft.payload.login_username,
            password_present: draft.login_password_present,
            access_token_present: self.secret_present(draft_id, "access_token").await?,
            refresh_token_present: self.secret_present(draft_id, "refresh_token").await?,
            cookie_present: self.secret_present(draft_id, "cookie").await?,
            remember_password: draft.payload.remember_password,
            login_status: "unknown".to_string(),
            login_error: None,
            last_login_at: None,
            session_status: if self.secret_present(draft_id, "cookie").await? {
                "ready".to_string()
            } else {
                "none".to_string()
            },
            session_expires_at: self.secret_text(draft_id, "session_expires_at").await?,
            newapi_user_id: self.secret_text(draft_id, "newapi_user_id").await?,
            token_expires_at: self.secret_text(draft_id, "token_expires_at").await?,
            token_refreshed_at: None,
            session_source: "draft".to_string(),
            updated_at: Some(draft.updated_at),
        })
    }

    pub(crate) async fn login_password(
        &self,
        draft_id: &str,
    ) -> Result<Option<String>, ApplicationError> {
        self.secret_text(draft_id, "login_password").await
    }

    pub(crate) async fn resolve_session(
        &self,
        draft_id: &str,
    ) -> Result<ResolvedSession, ApplicationError> {
        let access_token = self.secret_text(draft_id, "access_token").await?;
        let refresh_token = self.secret_text(draft_id, "refresh_token").await?;
        let cookie = self.secret_text(draft_id, "cookie").await?;
        let newapi_user_id = self.secret_text(draft_id, "newapi_user_id").await?;
        let ready = access_token.is_some() || cookie.is_some();
        Ok(ResolvedSession {
            status: if ready {
                SessionResolveStatus::Ready
            } else {
                SessionResolveStatus::ManualRequired
            },
            access_token,
            refresh_token,
            cookie,
            newapi_user_id,
            message: (!ready).then(|| "draft authorization is required".to_string()),
        })
    }

    pub(crate) async fn persist_session(
        &self,
        input: PersistStationSessionInput,
        expected_revision: i64,
    ) -> Result<StationCredentials, ApplicationError> {
        let draft = self.get(input.station_id.clone()).await?;
        if draft.state != "active" || draft.revision != expected_revision {
            return Err(ApplicationError::StaleRevision);
        }
        let now = self.now_ms();
        let fields = [
            ("access_token", input.access_token),
            ("refresh_token", input.refresh_token),
            ("cookie", input.cookie),
            ("newapi_user_id", input.newapi_user_id),
            ("token_expires_at", input.token_expires_at),
            ("session_expires_at", input.session_expires_at),
        ];
        let mut patches = Vec::new();
        for (kind, value) in fields {
            patches.push((
                kind.to_string(),
                self.prepare_secret_patch(&draft.id, kind, value.as_deref(), now)?,
            ));
        }
        let drafts = self.drafts;
        let draft_id = draft.id.clone();
        self.runtime
            .write(|write| {
                Box::pin(async move {
                    let current = drafts.get_for_write(write, &draft_id).await?;
                    if current.state != "active" || current.revision != expected_revision {
                        return Err(crate::persistence::error::PersistenceError::StaleRevision);
                    }
                    for (kind, patch) in patches {
                        apply_secret_patch(drafts, write, &draft_id, &kind, patch).await?;
                    }
                    Ok(())
                })
            })
            .await?;
        self.credentials_projection(&draft.id).await
    }

    pub(crate) async fn list_keys(
        &self,
        draft_id: &str,
    ) -> Result<Vec<StationKey>, ApplicationError> {
        let draft = self.get(draft_id.to_string()).await?;
        Ok(draft
            .payload
            .keys
            .into_iter()
            .enumerate()
            .map(|(priority, key)| StationKey {
                id: key.client_id.clone(),
                station_id: draft.id.clone(),
                name: key.name,
                api_key_masked: String::new(),
                api_key_present: draft.key_api_key_client_ids.contains(&key.client_id),
                enabled: key.enabled,
                priority: priority as i64,
                max_concurrency: 3,
                load_factor: None,
                schedulable: true,
                group_name: key.group_name,
                tier_label: None,
                group_binding_id: key.group_client_id,
                group_id_hash: key.group_id_hash,
                rate_multiplier: key.rate_multiplier,
                manual_rate_multiplier: key.rate_multiplier,
                manual_rate_updated_at: None,
                rate_source: key.rate_multiplier.map(|_| "manual".to_string()),
                rate_collected_at: None,
                balance_scope: Some("station_key".to_string()),
                status: "unchecked".to_string(),
                last_checked_at: None,
                last_used_at: None,
                note: key.note,
                created_at: draft.created_at.clone(),
                updated_at: draft.updated_at.clone(),
            })
            .collect())
    }

    pub(crate) async fn key_secret(
        &self,
        draft_id: &str,
        key_client_id: &str,
    ) -> Result<String, ApplicationError> {
        self.secret_text(draft_id, &draft_key_secret_kind(key_client_id))
            .await?
            .ok_or(ApplicationError::NotFound)
    }

    pub(crate) async fn list_groups(
        &self,
        draft_id: &str,
    ) -> Result<Vec<StationGroupBinding>, ApplicationError> {
        let draft = self.get(draft_id.to_string()).await?;
        Ok(draft
            .payload
            .groups
            .into_iter()
            .map(|group| StationGroupBinding {
                id: group.client_id,
                station_id: draft.id.clone(),
                station_key_id: None,
                binding_kind: BINDING_KIND_STATION_GROUP.to_string(),
                parent_group_binding_id: None,
                group_key_hash: group.group_key_hash,
                group_id_hash: group.group_id_hash,
                group_name: group.group_name,
                binding_status: BINDING_STATUS_AVAILABLE.to_string(),
                default_rate_multiplier: (group.source == "remote")
                    .then_some(group.rate_multiplier)
                    .flatten(),
                user_rate_multiplier: (group.source == "manual")
                    .then_some(group.rate_multiplier)
                    .flatten(),
                effective_rate_multiplier: group.rate_multiplier,
                inferred_group_category: group.inferred_group_category,
                group_category_override: group.group_category_override,
                rate_source: Some(group.source),
                confidence: 1.0,
                last_seen_at: None,
                last_checked_at: None,
                last_rate_changed_at: None,
                raw_json_redacted: None,
                created_at: draft.created_at.clone(),
                updated_at: draft.updated_at.clone(),
            })
            .collect())
    }

    pub(crate) fn runtime_fingerprint(payload: &ProviderDraftPayload) -> String {
        runtime_fingerprint(payload)
    }

    async fn secret_present(&self, draft_id: &str, kind: &str) -> Result<bool, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        Ok(self
            .drafts
            .secret(&mut read, draft_id, kind)
            .await?
            .is_some())
    }

    async fn secret_text(
        &self,
        draft_id: &str,
        kind: &str,
    ) -> Result<Option<String>, ApplicationError> {
        let mut read = self.runtime.begin_read().await?;
        let stored = self.drafts.secret(&mut read, draft_id, kind).await?;
        stored.map(|secret| self.decrypt_secret(secret)).transpose()
    }

    fn decrypt_secret(&self, stored: StoredEncryptedSecret) -> Result<String, ApplicationError> {
        let secret_ref = SecretRef {
            id: stored.id,
            scope: stored.scope,
            owner_id: stored.owner_id,
            kind: stored.kind,
        };
        let aad = crate::application::credentials::secret_aad(
            &secret_ref.scope,
            &secret_ref.owner_id,
            &secret_ref.kind,
            stored.encryption_version,
        );
        let encrypted = EncryptedSecret {
            ciphertext: stored.ciphertext,
            nonce: stored.nonce,
            masked_value: stored.masked_value,
            key_id: stored.key_id,
            encryption_version: stored.encryption_version,
            value_hash: stored.value_hash,
        };
        let plaintext = self.vault.decrypt(
            &aad,
            &encrypted.key_id,
            encrypted.encryption_version,
            &encrypted,
        )?;
        String::from_utf8(plaintext.as_bytes().to_vec()).map_err(|_| ApplicationError::Internal)
    }

    fn prepare_secret_patch(
        &self,
        draft_id: &str,
        kind: &str,
        value: Option<&str>,
        now: i64,
    ) -> Result<Option<Option<EncryptedSecretRow>>, CredentialError> {
        let Some(value) = value else {
            return Ok(None);
        };
        let value = value.trim();
        if value.is_empty() {
            return Ok(Some(None));
        }
        let secret_ref = SecretRef {
            id: self.ids.next_id(),
            scope: DRAFT_SECRET_SCOPE.to_string(),
            owner_id: draft_id.to_string(),
            kind: kind.to_string(),
        };
        let encrypted = self
            .vault
            .encrypt(&secret_ref.aad(), SecretBytes::from(value.to_string()))?;
        Ok(Some(Some(EncryptedSecretRow {
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
        })))
    }

    fn encrypt_secret_for(
        &self,
        scope: &str,
        owner_id: &str,
        kind: &str,
        value: &str,
        now: &str,
    ) -> Result<EncryptedSecretRow, CredentialError> {
        let secret_ref = SecretRef {
            id: self.ids.next_id(),
            scope: scope.to_string(),
            owner_id: owner_id.to_string(),
            kind: kind.to_string(),
        };
        let encrypted = self
            .vault
            .encrypt(&secret_ref.aad(), SecretBytes::from(value.to_string()))?;
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

    fn now_ms(&self) -> i64 {
        self.clock.now_utc().timestamp_millis()
    }
}

async fn apply_secret_patch(
    store: ProviderDraftStore,
    write: &mut crate::persistence::WriteSession,
    draft_id: &str,
    kind: &str,
    patch: Option<Option<EncryptedSecretRow>>,
) -> Result<(), crate::persistence::error::PersistenceError> {
    match patch {
        None => Ok(()),
        Some(Some(secret)) => store.upsert_secret(write, secret).await,
        Some(None) => store.delete_secret(write, draft_id, kind).await,
    }
}

fn validate_draft_payload(payload: &ProviderDraftPayload) -> Result<(), ApplicationError> {
    if !payload.credit_per_cny.is_finite()
        || payload.credit_per_cny <= 0.0
        || payload.collection_interval_minutes == 0
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    for group in &payload.groups {
        validate_identifier(&group.client_id)?;
        if group.group_name.trim().is_empty()
            || group
                .rate_multiplier
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(ApplicationError::ConstraintViolation);
        }
    }
    for key in &payload.keys {
        validate_identifier(&key.client_id)?;
        if key.name.trim().is_empty()
            || key
                .rate_multiplier
                .is_some_and(|value| !value.is_finite() || value < 0.0)
        {
            return Err(ApplicationError::ConstraintViolation);
        }
    }
    Ok(())
}

fn validate_committable_payload(payload: &ProviderDraftPayload) -> Result<(), ApplicationError> {
    validate_draft_payload(payload)?;
    if payload.name.trim().is_empty()
        || payload.website_url.trim().is_empty()
        || payload.api_base_url.trim().is_empty()
    {
        return Err(ApplicationError::ConstraintViolation);
    }
    Ok(())
}

fn validate_identifier(value: &str) -> Result<(), ApplicationError> {
    if value.trim().is_empty() || value.len() > 256 {
        Err(ApplicationError::ConstraintViolation)
    } else {
        Ok(())
    }
}

fn runtime_fingerprint(payload: &ProviderDraftPayload) -> String {
    let value = serde_json::json!({
        "stationType": payload.station_type,
        "websiteUrl": payload.website_url,
        "apiBaseUrl": payload.api_base_url,
        "collectorProxyMode": payload.collector_proxy_mode,
        "collectorProxyUrl": payload.collector_proxy_url,
    });
    format!("{:x}", Sha256::digest(value.to_string().as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        application::{clock::SystemClock, ids::UuidV7Generator},
        models::provider_drafts::{
            ProviderDraftGroup, ProviderDraftKey, ProviderDraftKeySecretInput,
        },
        persistence::runtime::PersistenceRuntime,
        services::secrets::vault::DataKeyVault,
    };

    fn payload(name: &str) -> ProviderDraftPayload {
        ProviderDraftPayload {
            name: name.to_string(),
            station_type: "newapi".to_string(),
            website_url: "https://draft.example.test".to_string(),
            api_base_url: "https://draft.example.test/v1".to_string(),
            collector_proxy_mode: "inherit".to_string(),
            collector_proxy_url: None,
            enabled: true,
            credit_per_cny: 1.0,
            low_balance_threshold_cny: None,
            collection_interval_minutes: 5,
            note: None,
            login_username: Some("draft@example.test".to_string()),
            remember_password: true,
            groups: vec![ProviderDraftGroup {
                client_id: "group-client-1".to_string(),
                group_key_hash: "remote:group-1".to_string(),
                group_id_hash: Some("group-id-hash".to_string()),
                group_name: "default".to_string(),
                rate_multiplier: Some(1.25),
                inferred_group_category: Some("standard".to_string()),
                group_category_override: None,
                source: "remote".to_string(),
            }],
            keys: vec![ProviderDraftKey {
                client_id: "key-client-1".to_string(),
                name: "Draft Key".to_string(),
                enabled: true,
                group_client_id: Some("group-client-1".to_string()),
                group_id_hash: Some("group-id-hash".to_string()),
                group_name: Some("default".to_string()),
                rate_multiplier: Some(1.25),
                note: None,
            }],
        }
    }

    async fn test_service() -> (tempfile::TempDir, PersistenceRuntime, ProviderDraftService) {
        let temp = tempfile::tempdir().expect("tempdir");
        let database_path = temp.path().join("provider-drafts.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&database_path)
            .await
            .expect("runtime");
        let service = ProviderDraftService::new(
            runtime.handle(),
            Arc::new(DataKeyVault::for_test([31; 32])),
            Arc::new(SystemClock),
            Arc::new(UuidV7Generator),
        );
        (temp, runtime, service)
    }

    async fn row_count(runtime: &PersistenceRuntime, table: &str) -> i64 {
        let mut read = runtime.handle().begin_read().await.expect("read");
        let query = format!("SELECT COUNT(*) FROM {table}");
        sqlx::query_scalar(&query)
            .fetch_one(read.connection())
            .await
            .expect("row count")
    }

    #[tokio::test]
    async fn draft_is_isolated_and_commit_is_atomic_and_idempotent() {
        let (_temp, runtime, service) = test_service().await;
        let initial = service
            .create_or_resume(CreateProviderDraftInput {
                base_station_id: None,
                payload: ProviderDraftPayload {
                    name: String::new(),
                    groups: Vec::new(),
                    keys: Vec::new(),
                    ..payload("")
                },
            })
            .await
            .expect("create incomplete draft");
        let api_key = "sk-provider-draft-plaintext-canary";
        let password = "provider-draft-password-canary";
        let patched = service
            .patch(PatchProviderDraftInput {
                draft_id: initial.id.clone(),
                expected_revision: initial.revision,
                payload: payload("Draft Provider"),
                station_api_key: None,
                login_password: Some(password.to_string()),
                key_api_keys: vec![ProviderDraftKeySecretInput {
                    client_id: "key-client-1".to_string(),
                    api_key: api_key.to_string(),
                }],
            })
            .await
            .expect("patch draft");

        assert_eq!(row_count(&runtime, "stations").await, 0);
        assert_eq!(row_count(&runtime, "station_group_bindings").await, 0);
        assert_eq!(row_count(&runtime, "change_events").await, 0);
        let mut read = runtime.handle().begin_read().await.expect("read payload");
        let payload_json: String =
            sqlx::query_scalar("SELECT payload_json FROM provider_drafts WHERE id = ?1")
                .bind(&patched.id)
                .fetch_one(read.connection())
                .await
                .expect("draft payload");
        assert!(!payload_json.contains(api_key));
        assert!(!payload_json.contains(password));
        drop(read);

        let commit_key = "provider-draft-commit-key".to_string();
        let station = service
            .commit(CommitProviderDraftInput {
                draft_id: patched.id.clone(),
                expected_revision: patched.revision,
                commit_key: commit_key.clone(),
            })
            .await
            .expect("commit draft");
        assert_eq!(row_count(&runtime, "stations").await, 1);
        assert_eq!(row_count(&runtime, "station_group_bindings").await, 1);
        assert_eq!(row_count(&runtime, "station_keys").await, 1);
        assert_eq!(row_count(&runtime, "change_events").await, 0);
        let mut read = runtime.handle().begin_read().await.expect("read secrets");
        let draft_secret_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM secrets WHERE scope = 'provider_draft' AND owner_id = ?1",
        )
        .bind(&patched.id)
        .fetch_one(read.connection())
        .await
        .expect("draft secret count");
        assert_eq!(draft_secret_count, 0);
        drop(read);

        let retried = service
            .commit(CommitProviderDraftInput {
                draft_id: patched.id.clone(),
                expected_revision: patched.revision,
                commit_key,
            })
            .await
            .expect("retry commit");
        assert_eq!(retried.id, station.id);
        assert!(matches!(
            service
                .commit(CommitProviderDraftInput {
                    draft_id: patched.id,
                    expected_revision: patched.revision,
                    commit_key: "different-commit-key".to_string(),
                })
                .await,
            Err(ApplicationError::StaleRevision)
        ));

        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn late_commit_key_conflict_rolls_back_formal_rows() {
        let (_temp, runtime, service) = test_service().await;
        let first = service
            .create_or_resume(CreateProviderDraftInput {
                base_station_id: None,
                payload: payload("First Provider"),
            })
            .await
            .expect("first draft");
        service
            .commit(CommitProviderDraftInput {
                draft_id: first.id,
                expected_revision: first.revision,
                commit_key: "shared-commit-key".to_string(),
            })
            .await
            .expect("first commit");

        let second = service
            .create_or_resume(CreateProviderDraftInput {
                base_station_id: None,
                payload: payload("Second Provider"),
            })
            .await
            .expect("second draft");
        assert!(matches!(
            service
                .commit(CommitProviderDraftInput {
                    draft_id: second.id.clone(),
                    expected_revision: second.revision,
                    commit_key: "shared-commit-key".to_string(),
                })
                .await,
            Err(ApplicationError::ConstraintViolation)
        ));
        assert_eq!(row_count(&runtime, "stations").await, 1);
        assert_eq!(row_count(&runtime, "station_group_bindings").await, 1);
        assert_eq!(row_count(&runtime, "station_keys").await, 0);
        assert_eq!(
            service.get(second.id).await.expect("active draft").state,
            "active"
        );

        runtime.close().await.expect("close runtime");
    }
}
