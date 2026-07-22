use sqlx::{Executor, Row, Sqlite, SqliteConnection};

use crate::{
    models::{
        credentials::{StationCredentials, StationSessionCredentialKind},
        group_facts::UpdateStationKeyGroupBindingInput,
        remote_keys::{RemoteKeyMatchStatus, RemoteStationKey},
        routing::{StationKeyCapabilities, UpdateStationKeyCapabilitiesInput},
        secrets::mask_secret,
        station_keys::{KeyPoolItem, StationKey},
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
};

#[derive(Debug, Clone)]
pub(crate) struct EncryptedSecretRow {
    pub(crate) id: String,
    pub(crate) scope: String,
    pub(crate) owner_id: String,
    pub(crate) kind: String,
    pub(crate) masked_value: String,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
    pub(crate) now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct NewStationKeyRow {
    pub(crate) id: String,
    pub(crate) station_id: String,
    pub(crate) name: String,
    pub(crate) encrypted_secret: Option<EncryptedSecretRow>,
    pub(crate) enabled: bool,
    pub(crate) priority: Option<i64>,
    pub(crate) max_concurrency: Option<i64>,
    pub(crate) load_factor: Option<i64>,
    pub(crate) schedulable: Option<bool>,
    pub(crate) group_name: Option<String>,
    pub(crate) tier_label: Option<String>,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) rate_multiplier: Option<f64>,
    pub(crate) manual_rate_multiplier: Option<f64>,
    pub(crate) manual_rate_updated_at: Option<String>,
    pub(crate) rate_source: Option<String>,
    pub(crate) balance_scope: Option<String>,
    pub(crate) note: Option<String>,
    pub(crate) now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StationKeyPatch {
    pub(crate) id: String,
    pub(crate) station_id: String,
    pub(crate) name: String,
    pub(crate) encrypted_secret: Option<EncryptedSecretRow>,
    pub(crate) enabled: bool,
    pub(crate) priority: i64,
    pub(crate) max_concurrency: i64,
    pub(crate) load_factor: Option<i64>,
    pub(crate) schedulable: bool,
    pub(crate) group_name: Option<String>,
    pub(crate) tier_label: Option<String>,
    pub(crate) group_binding_id: Option<String>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) rate_multiplier: Option<f64>,
    pub(crate) manual_rate_multiplier: Option<Option<f64>>,
    pub(crate) manual_rate_updated_at: Option<String>,
    pub(crate) rate_source: Option<String>,
    pub(crate) balance_scope: Option<String>,
    pub(crate) status: String,
    pub(crate) note: Option<String>,
    pub(crate) now: String,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "credential integration targets construct remote-key rows through different application boundaries"
    )
)]
pub(crate) struct NewRemoteStationKeyRow {
    pub(crate) id: String,
    pub(crate) station_id: String,
    pub(crate) remote_key_id_hash: Option<String>,
    pub(crate) remote_key_name: Option<String>,
    pub(crate) api_key_masked: Option<String>,
    pub(crate) api_key_fingerprint: Option<String>,
    pub(crate) group_id_hash: Option<String>,
    pub(crate) group_name: Option<String>,
    pub(crate) tier_label: Option<String>,
    pub(crate) rate_multiplier: Option<f64>,
    pub(crate) rate_source: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) last_used_at: Option<String>,
    pub(crate) raw_source: String,
    pub(crate) collected_at: String,
    pub(crate) now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StoredEncryptedSecret {
    pub(crate) id: String,
    pub(crate) scope: String,
    pub(crate) owner_id: String,
    pub(crate) kind: String,
    pub(crate) masked_value: String,
    pub(crate) ciphertext: Vec<u8>,
    pub(crate) nonce: Vec<u8>,
}

#[derive(Debug, Clone)]
pub(crate) struct StationCredentialPatch {
    pub(crate) station_id: String,
    pub(crate) login_username: Option<String>,
    pub(crate) remember_password: bool,
    pub(crate) password_secret: Option<EncryptedSecretRow>,
    pub(crate) now: String,
}

#[derive(Debug, Clone)]
pub(crate) struct StationSessionPatch {
    pub(crate) station_id: String,
    pub(crate) access_token_secret: Option<EncryptedSecretRow>,
    pub(crate) refresh_token_secret: Option<EncryptedSecretRow>,
    pub(crate) cookie_secret: Option<EncryptedSecretRow>,
    pub(crate) newapi_user_id: Option<String>,
    pub(crate) token_expires_at: Option<String>,
    pub(crate) session_expires_at: Option<String>,
    pub(crate) session_source: String,
    pub(crate) now: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CredentialStore;

impl CredentialStore {
    pub(crate) async fn assert_station_endpoint_revision(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        expected_revision: i64,
    ) -> Result<(), PersistenceError> {
        let revision =
            sqlx::query_scalar::<_, i64>("SELECT endpoint_revision FROM stations WHERE id = ?1")
                .bind(station_id)
                .fetch_optional(write.connection())
                .await?
                .ok_or(sqlx::Error::RowNotFound)?;
        if revision != expected_revision {
            return Err(PersistenceError::StaleRevision);
        }
        Ok(())
    }

    pub(crate) async fn station_type(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<String, PersistenceError> {
        sqlx::query_scalar("SELECT station_type FROM stations WHERE id = ?1")
            .bind(station_id)
            .fetch_optional(read.connection())
            .await?
            .ok_or(PersistenceError::NotFound)
    }

    pub(crate) async fn station_credentials(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<StationCredentials, PersistenceError> {
        ensure_station_exists(read.connection(), station_id).await?;
        station_credentials(read.connection(), station_id).await
    }

    pub(crate) async fn station_key_secret(
        &self,
        read: &mut ReadSession,
        station_key_id: &str,
    ) -> Result<StoredEncryptedSecret, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT s.id, s.scope, s.owner_id, s.kind, s.masked_value, s.ciphertext, s.nonce
            FROM station_keys k
            JOIN secrets s ON s.id = k.api_key_secret_id
            WHERE k.id = ?1
            "#,
        )
        .bind(station_key_id)
        .fetch_optional(read.connection())
        .await?;
        row.map(row_to_stored_secret)
            .ok_or(PersistenceError::NotFound)
    }

    pub(crate) async fn station_credential_secret(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        kind: &str,
    ) -> Result<Option<StoredEncryptedSecret>, PersistenceError> {
        let secret_column = station_credential_secret_column(kind)?;
        let query = format!(
            r#"
            SELECT s.id, s.scope, s.owner_id, s.kind, s.masked_value, s.ciphertext, s.nonce
            FROM station_credentials c
            JOIN secrets s ON s.id = c.{secret_column}
            WHERE c.station_id = ?1
            "#
        );
        sqlx::query(&query)
            .bind(station_id)
            .fetch_optional(read.connection())
            .await
            .map(|row| row.map(row_to_stored_secret))
            .map_err(Into::into)
    }

    pub(crate) async fn update_station_credentials(
        &self,
        write: &mut WriteSession,
        patch: StationCredentialPatch,
    ) -> Result<StationCredentials, PersistenceError> {
        ensure_station_exists(write.connection(), &patch.station_id).await?;
        let existing_secret_id = station_credential_secret_id(
            write.connection(),
            &patch.station_id,
            "login_password_secret_id",
        )
        .await?;
        let password_secret_id = if patch.remember_password {
            match patch.password_secret.as_ref() {
                Some(secret) => Some(upsert_secret(write.connection(), secret).await?),
                None => existing_secret_id.clone(),
            }
        } else {
            None
        };
        sqlx::query(
            r#"
            INSERT INTO station_credentials (
                station_id, login_username, login_password, login_password_secret_id,
                remember_password, login_status, login_error, session_status,
                session_source, created_at, updated_at
            ) VALUES (?1, ?2, NULL, ?3, ?4, 'saved', NULL, 'none', 'none', ?5, ?5)
            ON CONFLICT(station_id) DO UPDATE SET
                login_username = excluded.login_username,
                login_password = NULL,
                login_password_secret_id = excluded.login_password_secret_id,
                remember_password = excluded.remember_password,
                login_status = 'saved',
                login_error = NULL,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&patch.station_id)
        .bind(normalize_optional_string(patch.login_username))
        .bind(&password_secret_id)
        .bind(bool_to_i64(patch.remember_password))
        .bind(&patch.now)
        .execute(write.connection())
        .await?;
        if existing_secret_id != password_secret_id {
            delete_unreferenced_secret(write.connection(), existing_secret_id.as_deref()).await?;
        }
        station_credentials(write.connection(), &patch.station_id).await
    }

    pub(crate) async fn update_station_session(
        &self,
        write: &mut WriteSession,
        patch: StationSessionPatch,
    ) -> Result<StationCredentials, PersistenceError> {
        ensure_station_exists(write.connection(), &patch.station_id).await?;
        let existing = station_session_secret_ids(write.connection(), &patch.station_id).await?;
        let access_token_secret_id = upsert_or_existing_secret(
            write.connection(),
            patch.access_token_secret.as_ref(),
            existing.0.as_deref(),
        )
        .await?;
        let refresh_token_secret_id = upsert_or_existing_secret(
            write.connection(),
            patch.refresh_token_secret.as_ref(),
            existing.1.as_deref(),
        )
        .await?;
        let cookie_secret_id = upsert_or_existing_secret(
            write.connection(),
            patch.cookie_secret.as_ref(),
            existing.2.as_deref(),
        )
        .await?;
        let has_session = access_token_secret_id.is_some()
            || refresh_token_secret_id.is_some()
            || cookie_secret_id.is_some();
        sqlx::query(
            r#"
            INSERT INTO station_credentials (
                station_id, remember_password, login_status, session_status,
                session_expires_at, access_token_secret_id, refresh_token_secret_id,
                cookie_secret_id, newapi_user_id, token_expires_at, token_refreshed_at,
                session_source, created_at, updated_at
            ) VALUES (?1, 0, 'saved', ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
            ON CONFLICT(station_id) DO UPDATE SET
                session_status = excluded.session_status,
                session_expires_at = excluded.session_expires_at,
                access_token_secret_id = excluded.access_token_secret_id,
                refresh_token_secret_id = excluded.refresh_token_secret_id,
                cookie_secret_id = excluded.cookie_secret_id,
                newapi_user_id = excluded.newapi_user_id,
                token_expires_at = excluded.token_expires_at,
                token_refreshed_at = excluded.token_refreshed_at,
                session_source = excluded.session_source,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&patch.station_id)
        .bind(if has_session {
            "valid"
        } else {
            "manual_required"
        })
        .bind(normalize_optional_string(patch.session_expires_at))
        .bind(access_token_secret_id)
        .bind(refresh_token_secret_id)
        .bind(cookie_secret_id)
        .bind(normalize_optional_string(patch.newapi_user_id))
        .bind(normalize_optional_string(patch.token_expires_at))
        .bind(&patch.now)
        .bind(normalize_required_string(
            patch.session_source,
            "manual_token",
        ))
        .bind(&patch.now)
        .execute(write.connection())
        .await?;
        station_credentials(write.connection(), &patch.station_id).await
    }

    pub(crate) async fn update_station_session_if_revision(
        &self,
        write: &mut WriteSession,
        patch: StationSessionPatch,
        expected_revision: i64,
    ) -> Result<StationCredentials, PersistenceError> {
        let actual_revision =
            sqlx::query_scalar::<_, i64>("SELECT endpoint_revision FROM stations WHERE id = ?1")
                .bind(&patch.station_id)
                .fetch_optional(write.connection())
                .await?
                .ok_or(PersistenceError::NotFound)?;
        if actual_revision != expected_revision {
            return Err(PersistenceError::StaleRevision);
        }
        self.update_station_session(write, patch).await
    }

    pub(crate) async fn invalidate_station_session_credential(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        kind: StationSessionCredentialKind,
        now: &str,
    ) -> Result<(), PersistenceError> {
        ensure_station_exists(write.connection(), station_id).await?;
        let column = match kind {
            StationSessionCredentialKind::AccessToken => "access_token_secret_id",
            StationSessionCredentialKind::RefreshToken => "refresh_token_secret_id",
            StationSessionCredentialKind::Cookie => "cookie_secret_id",
        };
        let secret_id =
            station_credential_secret_id(write.connection(), station_id, column).await?;
        let query = format!(
            r#"
            UPDATE station_credentials
            SET {column} = NULL,
                session_status = CASE
                    WHEN access_token_secret_id IS NULL
                     AND refresh_token_secret_id IS NULL
                     AND cookie_secret_id IS NULL
                    THEN 'manual_required'
                    ELSE session_status
                END,
                updated_at = ?1
            WHERE station_id = ?2
            "#
        );
        sqlx::query(&query)
            .bind(now)
            .bind(station_id)
            .execute(write.connection())
            .await?;
        refresh_station_session_status(write.connection(), station_id, now).await?;
        delete_unreferenced_secret(write.connection(), secret_id.as_deref()).await
    }

    pub(crate) async fn clear_station_credentials(
        &self,
        write: &mut WriteSession,
        station_id: &str,
    ) -> Result<StationCredentials, PersistenceError> {
        ensure_station_exists(write.connection(), station_id).await?;
        let secret_ids = station_all_credential_secret_ids(write.connection(), station_id).await?;
        sqlx::query("DELETE FROM station_credentials WHERE station_id = ?1")
            .bind(station_id)
            .execute(write.connection())
            .await?;
        for secret_id in secret_ids.iter().flatten() {
            delete_unreferenced_secret(write.connection(), Some(secret_id)).await?;
        }
        station_credentials(write.connection(), station_id).await
    }

    pub(crate) async fn delete_station_key(
        &self,
        write: &mut WriteSession,
        station_key_id: &str,
    ) -> Result<(), PersistenceError> {
        let station_id = station_id_for_key(write.connection(), station_key_id).await?;
        let secret_id = station_key_secret_id(write.connection(), station_key_id).await?;
        sqlx::query(
            r#"
            UPDATE remote_station_keys
            SET match_status = 'unbound', matched_station_key_id = NULL,
                match_confidence = 0.0
            WHERE matched_station_key_id = ?1
            "#,
        )
        .bind(station_key_id)
        .execute(write.connection())
        .await?;
        let result = sqlx::query("DELETE FROM station_keys WHERE id = ?1")
            .bind(station_key_id)
            .execute(write.connection())
            .await?;
        if result.rows_affected() == 0 {
            return Err(PersistenceError::NotFound);
        }
        normalize_station_key_priorities(write.connection(), &station_id).await?;
        delete_unreferenced_secret(write.connection(), secret_id.as_deref()).await?;
        Ok(())
    }

    pub(crate) async fn list_station_keys(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Vec<StationKey>, PersistenceError> {
        list_station_keys(read.connection(), station_id).await
    }

    pub(crate) async fn list_key_pool_items(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<KeyPoolItem>, PersistenceError> {
        list_key_pool_items(read.connection()).await
    }

    pub(crate) async fn insert_station_key(
        &self,
        write: &mut WriteSession,
        row: NewStationKeyRow,
    ) -> Result<StationKey, PersistenceError> {
        validate_station_key_fields(&row.name, row.max_concurrency.unwrap_or(3))?;
        let priority = match row.priority {
            Some(priority) => priority,
            None => next_station_key_priority(write.connection(), &row.station_id).await?,
        };
        let secret_id = if let Some(secret) = row.encrypted_secret {
            Some(upsert_secret(write.connection(), &secret).await?)
        } else {
            None
        };
        sqlx::query(
            r#"
            INSERT INTO station_keys (
                id, station_id, name, api_key, api_key_secret_id, enabled, priority,
                max_concurrency, load_factor, schedulable, group_name, tier_label,
                group_binding_id, group_id_hash, rate_multiplier, manual_rate_multiplier,
                manual_rate_updated_at, rate_source, balance_scope, status, note,
                created_at, updated_at
            ) VALUES (?1, ?2, ?3, '', ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                ?14, ?15, ?16, ?17, ?18, 'unchecked', ?19, ?20, ?21)
            "#,
        )
        .bind(&row.id)
        .bind(&row.station_id)
        .bind(row.name.trim())
        .bind(&secret_id)
        .bind(bool_to_i64(row.enabled))
        .bind(priority)
        .bind(row.max_concurrency.unwrap_or(3))
        .bind(row.load_factor)
        .bind(bool_to_i64(row.schedulable.unwrap_or(true)))
        .bind(normalize_optional_string(row.group_name))
        .bind(normalize_optional_string(row.tier_label))
        .bind(normalize_optional_string(row.group_binding_id))
        .bind(normalize_optional_string(row.group_id_hash))
        .bind(row.rate_multiplier)
        .bind(row.manual_rate_multiplier)
        .bind(row.manual_rate_updated_at)
        .bind(normalize_optional_string(row.rate_source))
        .bind(normalize_optional_string(row.balance_scope))
        .bind(normalize_optional_string(row.note))
        .bind(&row.now)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        station_key_by_id(write.connection(), &row.id).await
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "credential integration targets exercise secret replacement through the application service"
        )
    )]
    pub(crate) async fn replace_station_key_secret(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        station_key_id: &str,
        secret: EncryptedSecretRow,
    ) -> Result<(String, StationKey), PersistenceError> {
        let secret_id = upsert_secret(write.connection(), &secret).await?;
        let updated = sqlx::query(
            r#"
            UPDATE station_keys
            SET api_key = '',
                api_key_secret_id = ?1,
                updated_at = ?2
            WHERE id = ?3 AND station_id = ?4
            "#,
        )
        .bind(&secret_id)
        .bind(&secret.now)
        .bind(station_key_id)
        .bind(station_id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::NotFound);
        }
        let station_key = station_key_by_id(write.connection(), station_key_id).await?;
        Ok((secret_id, station_key))
    }

    pub(crate) async fn update_station_key(
        &self,
        write: &mut WriteSession,
        patch: StationKeyPatch,
    ) -> Result<StationKey, PersistenceError> {
        validate_station_key_fields(&patch.name, patch.max_concurrency)?;
        let secret_id = if let Some(secret) = patch.encrypted_secret {
            Some(upsert_secret(write.connection(), &secret).await?)
        } else {
            station_key_secret_id(write.connection(), &patch.id).await?
        };
        let manual_rate_multiplier = match patch.manual_rate_multiplier {
            Some(value) => value,
            None => existing_manual_rate_multiplier(write.connection(), &patch.id).await?,
        };
        let manual_rate_updated_at = if patch.manual_rate_updated_at.is_some() {
            patch.manual_rate_updated_at
        } else if patch.manual_rate_multiplier.is_some() {
            None
        } else {
            existing_manual_rate_updated_at(write.connection(), &patch.id).await?
        };
        let updated = sqlx::query(
            r#"
            UPDATE station_keys
            SET name = ?1,
                api_key = '',
                api_key_secret_id = ?2,
                enabled = ?3,
                priority = ?4,
                max_concurrency = ?5,
                load_factor = ?6,
                schedulable = ?7,
                group_name = ?8,
                tier_label = ?9,
                group_binding_id = ?10,
                group_id_hash = ?11,
                rate_multiplier = ?12,
                manual_rate_multiplier = ?13,
                manual_rate_updated_at = ?14,
                rate_source = ?15,
                balance_scope = ?16,
                status = ?17,
                note = ?18,
                updated_at = ?19
            WHERE id = ?20 AND station_id = ?21
            "#,
        )
        .bind(patch.name.trim())
        .bind(secret_id)
        .bind(bool_to_i64(patch.enabled))
        .bind(patch.priority)
        .bind(patch.max_concurrency)
        .bind(patch.load_factor)
        .bind(bool_to_i64(patch.schedulable))
        .bind(normalize_optional_string(patch.group_name))
        .bind(normalize_optional_string(patch.tier_label))
        .bind(normalize_optional_string(patch.group_binding_id))
        .bind(normalize_optional_string(patch.group_id_hash))
        .bind(patch.rate_multiplier)
        .bind(manual_rate_multiplier)
        .bind(manual_rate_updated_at)
        .bind(normalize_optional_string(patch.rate_source))
        .bind(normalize_optional_string(patch.balance_scope))
        .bind(patch.status.trim())
        .bind(normalize_optional_string(patch.note))
        .bind(&patch.now)
        .bind(&patch.id)
        .bind(&patch.station_id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::NotFound);
        }
        station_key_by_id(write.connection(), &patch.id).await
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "credential integration targets query keys through different application projections"
        )
    )]
    pub(crate) async fn station_key_by_id(
        &self,
        read: &mut ReadSession,
        station_key_id: &str,
    ) -> Result<StationKey, PersistenceError> {
        station_key_by_id(read.connection(), station_key_id).await
    }

    pub(crate) async fn station_key_by_id_for_write(
        &self,
        write: &mut WriteSession,
        station_key_id: &str,
    ) -> Result<StationKey, PersistenceError> {
        station_key_by_id(write.connection(), station_key_id).await
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "credential integration targets verify station-scoped secret ownership"
        )
    )]
    pub(crate) async fn station_key_secret_id_for_station(
        &self,
        read: &mut ReadSession,
        station_id: &str,
        station_key_id: &str,
    ) -> Result<Option<String>, PersistenceError> {
        station_key_secret_id_for_station(read.connection(), station_id, station_key_id).await
    }

    pub(crate) async fn reorder_station_keys(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        station_key_ids: &[String],
        now: &str,
    ) -> Result<Vec<StationKey>, PersistenceError> {
        if station_key_ids.is_empty() {
            return Err(PersistenceError::NotFound);
        }
        for (index, id) in station_key_ids.iter().enumerate() {
            let updated = sqlx::query(
                "UPDATE station_keys SET priority = ?1, updated_at = ?2 WHERE id = ?3 AND station_id = ?4",
            )
            .bind(index as i64)
            .bind(now)
            .bind(id)
            .bind(station_id)
            .execute(write.connection())
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(PersistenceError::NotFound);
            }
        }
        list_station_keys(write.connection(), station_id).await
    }

    pub(crate) async fn reorder_key_pool(
        &self,
        write: &mut WriteSession,
        station_key_ids: &[String],
        now: &str,
    ) -> Result<Vec<KeyPoolItem>, PersistenceError> {
        if station_key_ids.is_empty() {
            return Err(PersistenceError::ConstraintViolation);
        }
        for (index, id) in station_key_ids.iter().enumerate() {
            let updated = sqlx::query(
                r#"
                UPDATE station_keys
                SET priority = ?1, routing_order = ?1, updated_at = ?2
                WHERE id = ?3
                "#,
            )
            .bind(index as i64)
            .bind(now)
            .bind(id)
            .execute(write.connection())
            .await?
            .rows_affected();
            if updated == 0 {
                return Err(PersistenceError::NotFound);
            }
        }
        list_key_pool_items(write.connection()).await
    }

    pub(crate) async fn update_station_key_group_binding(
        &self,
        write: &mut WriteSession,
        input: UpdateStationKeyGroupBindingInput,
        now: &str,
    ) -> Result<StationKey, PersistenceError> {
        let key_station_id = station_id_for_key(write.connection(), &input.station_key_id).await?;
        let binding = sqlx::query(
            r#"
            SELECT station_id, station_key_id, group_key_hash, group_name,
                   effective_rate_multiplier
            FROM station_group_bindings
            WHERE id = ?1
            "#,
        )
        .bind(&input.group_binding_id)
        .fetch_optional(write.connection())
        .await?
        .ok_or(PersistenceError::NotFound)?;
        let binding_station_id: String = binding.get("station_id");
        let bound_station_key_id: Option<String> = binding.get("station_key_id");
        if binding_station_id != key_station_id
            || bound_station_key_id
                .as_deref()
                .is_some_and(|id| id != input.station_key_id)
        {
            return Err(PersistenceError::ConstraintViolation);
        }
        sqlx::query(
            r#"
            UPDATE station_keys
            SET group_binding_id = ?1,
                group_id_hash = ?2,
                group_name = ?3,
                rate_multiplier = ?4,
                rate_source = 'manual',
                rate_collected_at = ?5,
                balance_scope = COALESCE(balance_scope, 'station'),
                updated_at = ?5
            WHERE id = ?6
            "#,
        )
        .bind(&input.group_binding_id)
        .bind(binding.get::<String, _>("group_key_hash"))
        .bind(binding.get::<String, _>("group_name"))
        .bind(binding.get::<Option<f64>, _>("effective_rate_multiplier"))
        .bind(now)
        .bind(&input.station_key_id)
        .execute(write.connection())
        .await?;
        station_key_by_id(write.connection(), &input.station_key_id).await
    }

    pub(crate) async fn clear_station_key_group_binding(
        &self,
        write: &mut WriteSession,
        station_key_id: &str,
        now: &str,
    ) -> Result<StationKey, PersistenceError> {
        let updated = sqlx::query(
            r#"
            UPDATE station_keys
            SET group_binding_id = NULL,
                group_id_hash = NULL,
                group_name = NULL,
                rate_multiplier = NULL,
                rate_source = NULL,
                rate_collected_at = NULL,
                updated_at = ?1
            WHERE id = ?2
            "#,
        )
        .bind(now)
        .bind(station_key_id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::NotFound);
        }
        station_key_by_id(write.connection(), station_key_id).await
    }

    #[cfg_attr(
        test,
        allow(
            dead_code,
            reason = "credential integration targets exercise remote-key upsert through the application service"
        )
    )]
    pub(crate) async fn upsert_remote_station_key(
        &self,
        write: &mut WriteSession,
        row: NewRemoteStationKeyRow,
    ) -> Result<RemoteStationKey, PersistenceError> {
        sqlx::query(
            r#"
            INSERT INTO remote_station_keys (
                id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
                api_key_fingerprint, group_id_hash, group_name, tier_label, rate_multiplier,
                rate_source, created_at, last_used_at, raw_source, match_status,
                matched_station_key_id, match_confidence, collected_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
                'unbound', NULL, 0.0, ?15, ?16)
            ON CONFLICT(station_id, remote_key_id_hash) DO UPDATE SET
                remote_key_name = excluded.remote_key_name,
                api_key_masked = excluded.api_key_masked,
                api_key_fingerprint = excluded.api_key_fingerprint,
                group_id_hash = excluded.group_id_hash,
                group_name = excluded.group_name,
                tier_label = excluded.tier_label,
                rate_multiplier = excluded.rate_multiplier,
                rate_source = excluded.rate_source,
                created_at = excluded.created_at,
                last_used_at = excluded.last_used_at,
                raw_source = excluded.raw_source,
                collected_at = excluded.collected_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&row.id)
        .bind(&row.station_id)
        .bind(&row.remote_key_id_hash)
        .bind(normalize_optional_string(row.remote_key_name))
        .bind(normalize_optional_string(row.api_key_masked))
        .bind(normalize_optional_string(row.api_key_fingerprint))
        .bind(normalize_optional_string(row.group_id_hash))
        .bind(normalize_optional_string(row.group_name))
        .bind(normalize_optional_string(row.tier_label))
        .bind(row.rate_multiplier)
        .bind(normalize_optional_string(row.rate_source))
        .bind(row.created_at)
        .bind(row.last_used_at)
        .bind(row.raw_source)
        .bind(&row.collected_at)
        .bind(&row.now)
        .execute(write.connection())
        .await?;
        remote_station_key_by_identity(
            write.connection(),
            &row.station_id,
            row.remote_key_id_hash.as_deref(),
            &row.id,
        )
        .await
    }

    pub(crate) async fn replace_remote_station_keys(
        &self,
        write: &mut WriteSession,
        station_id: &str,
        keys: &[RemoteStationKey],
        now: &str,
    ) -> Result<Vec<RemoteStationKey>, PersistenceError> {
        ensure_station_exists(write.connection(), station_id).await?;
        for key in keys {
            validate_remote_station_key(write.connection(), station_id, key).await?;
        }
        sqlx::query("DELETE FROM remote_station_keys WHERE station_id = ?1")
            .bind(station_id)
            .execute(write.connection())
            .await?;
        for key in keys {
            insert_remote_station_key_snapshot(write.connection(), key, now).await?;
        }
        list_remote_station_keys(write.connection(), station_id).await
    }

    pub(crate) async fn save_remote_station_key_snapshot(
        &self,
        write: &mut WriteSession,
        key: &RemoteStationKey,
        now: &str,
    ) -> Result<RemoteStationKey, PersistenceError> {
        validate_remote_station_key(write.connection(), &key.station_id, key).await?;
        sqlx::query(
            r#"
            DELETE FROM remote_station_keys
            WHERE station_id = ?1
              AND (id = ?2 OR (?3 IS NOT NULL AND remote_key_id_hash = ?3))
            "#,
        )
        .bind(&key.station_id)
        .bind(&key.id)
        .bind(&key.remote_key_id_hash)
        .execute(write.connection())
        .await?;
        insert_remote_station_key_snapshot(write.connection(), key, now).await?;
        remote_station_key_by_id(write.connection(), &key.id).await
    }

    pub(crate) async fn list_remote_station_keys(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Vec<RemoteStationKey>, PersistenceError> {
        ensure_station_exists(read.connection(), station_id).await?;
        list_remote_station_keys(read.connection(), station_id).await
    }

    pub(crate) async fn bind_remote_station_key(
        &self,
        write: &mut WriteSession,
        remote_key_id: &str,
        station_key_id: &str,
        now: &str,
    ) -> Result<Vec<RemoteStationKey>, PersistenceError> {
        let updated = sqlx::query(
            r#"
            UPDATE remote_station_keys
            SET matched_station_key_id = ?1,
                match_status = 'matched',
                match_confidence = 1.0,
                updated_at = ?2
            WHERE id = ?3
              AND station_id = (SELECT station_id FROM station_keys WHERE id = ?1)
            "#,
        )
        .bind(station_key_id)
        .bind(now)
        .bind(remote_key_id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::NotFound);
        }
        let station_id = station_id_for_key(write.connection(), station_key_id).await?;
        list_remote_station_keys(write.connection(), &station_id).await
    }

    pub(crate) async fn unbind_remote_station_key(
        &self,
        write: &mut WriteSession,
        remote_key_id: &str,
        station_id: &str,
        now: &str,
    ) -> Result<Vec<RemoteStationKey>, PersistenceError> {
        ensure_station_exists(write.connection(), station_id).await?;
        let updated = sqlx::query(
            r#"
            UPDATE remote_station_keys
            SET match_status = 'unbound',
                matched_station_key_id = NULL,
                match_confidence = 0.0,
                updated_at = ?1
            WHERE id = ?2 AND station_id = ?3
            "#,
        )
        .bind(now)
        .bind(remote_key_id)
        .bind(station_id)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::NotFound);
        }
        list_remote_station_keys(write.connection(), station_id).await
    }

    pub(crate) async fn station_key_capabilities(
        &self,
        read: &mut ReadSession,
        station_key_id: &str,
    ) -> Result<StationKeyCapabilities, PersistenceError> {
        ensure_station_key_exists(read.connection(), station_key_id).await?;
        station_key_capabilities(read.connection(), station_key_id).await
    }

    pub(crate) async fn station_key_capabilities_for_write(
        &self,
        write: &mut WriteSession,
        station_key_id: &str,
    ) -> Result<StationKeyCapabilities, PersistenceError> {
        ensure_station_key_exists(write.connection(), station_key_id).await?;
        station_key_capabilities(write.connection(), station_key_id).await
    }

    pub(crate) async fn update_station_key_capabilities(
        &self,
        write: &mut WriteSession,
        input: UpdateStationKeyCapabilitiesInput,
        now: &str,
    ) -> Result<StationKeyCapabilities, PersistenceError> {
        ensure_station_key_exists(write.connection(), &input.station_key_id).await?;
        let model_allowlist = serialize_string_list(&input.model_allowlist)?;
        let model_blocklist = serialize_string_list(&input.model_blocklist)?;
        let preferred_models = serialize_string_list(&input.preferred_models)?;
        let routing_tags = serialize_string_list(&input.routing_tags)?;
        sqlx::query(
            r#"
            INSERT INTO station_key_capabilities (
                station_key_id, supports_chat_completions, supports_responses,
                supports_embeddings, supports_stream, supports_tools, supports_vision,
                supports_reasoning, model_allowlist_json, model_blocklist_json,
                preferred_models_json, only_use_as_backup, routing_tags_json, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ON CONFLICT(station_key_id) DO UPDATE SET
                supports_chat_completions = excluded.supports_chat_completions,
                supports_responses = excluded.supports_responses,
                supports_embeddings = excluded.supports_embeddings,
                supports_stream = excluded.supports_stream,
                supports_tools = excluded.supports_tools,
                supports_vision = excluded.supports_vision,
                supports_reasoning = excluded.supports_reasoning,
                model_allowlist_json = excluded.model_allowlist_json,
                model_blocklist_json = excluded.model_blocklist_json,
                preferred_models_json = excluded.preferred_models_json,
                only_use_as_backup = excluded.only_use_as_backup,
                routing_tags_json = excluded.routing_tags_json,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&input.station_key_id)
        .bind(bool_to_i64(input.supports_chat_completions))
        .bind(bool_to_i64(input.supports_responses))
        .bind(bool_to_i64(input.supports_embeddings))
        .bind(bool_to_i64(input.supports_stream))
        .bind(bool_to_i64(input.supports_tools))
        .bind(bool_to_i64(input.supports_vision))
        .bind(bool_to_i64(input.supports_reasoning))
        .bind(model_allowlist)
        .bind(model_blocklist)
        .bind(preferred_models)
        .bind(bool_to_i64(input.only_use_as_backup))
        .bind(routing_tags)
        .bind(now)
        .execute(write.connection())
        .await?;
        station_key_capabilities(write.connection(), &input.station_key_id).await
    }
}

async fn validate_remote_station_key(
    connection: &mut SqliteConnection,
    station_id: &str,
    key: &RemoteStationKey,
) -> Result<(), PersistenceError> {
    if key.station_id != station_id
        || !key.match_confidence.is_finite()
        || !(0.0..=1.0).contains(&key.match_confidence)
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    if let Some(station_key_id) = key.matched_station_key_id.as_deref() {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM station_keys WHERE id = ?1 AND station_id = ?2",
        )
        .bind(station_key_id)
        .bind(station_id)
        .fetch_one(&mut *connection)
        .await?;
        if exists == 0 {
            return Err(PersistenceError::ConstraintViolation);
        }
    }
    Ok(())
}

async fn insert_remote_station_key_snapshot(
    connection: &mut SqliteConnection,
    key: &RemoteStationKey,
    now: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        r#"
        INSERT INTO remote_station_keys (
            id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
            api_key_fingerprint, group_id_hash, group_name, tier_label, rate_multiplier,
            rate_source, created_at, last_used_at, raw_source, match_status,
            matched_station_key_id, match_confidence, collected_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14,
            ?15, ?16, ?17, ?18, ?19)
        "#,
    )
    .bind(&key.id)
    .bind(&key.station_id)
    .bind(&key.remote_key_id_hash)
    .bind(normalize_optional_string(key.remote_key_name.clone()))
    .bind(normalize_optional_string(key.api_key_masked.clone()))
    .bind(normalize_optional_string(key.api_key_fingerprint.clone()))
    .bind(normalize_optional_string(key.group_id_hash.clone()))
    .bind(normalize_optional_string(key.group_name.clone()))
    .bind(normalize_optional_string(key.tier_label.clone()))
    .bind(key.rate_multiplier)
    .bind(normalize_optional_string(key.rate_source.clone()))
    .bind(&key.created_at)
    .bind(&key.last_used_at)
    .bind(&key.raw_source)
    .bind(key.match_status.as_str())
    .bind(&key.matched_station_key_id)
    .bind(key.match_confidence)
    .bind(&key.collected_at)
    .bind(now)
    .execute(connection)
    .await?;
    Ok(())
}

async fn ensure_station_exists(
    connection: &mut SqliteConnection,
    station_id: &str,
) -> Result<(), PersistenceError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM stations WHERE id = ?1")
        .bind(station_id)
        .fetch_one(connection)
        .await?;
    if exists == 0 {
        return Err(PersistenceError::NotFound);
    }
    Ok(())
}

async fn ensure_station_key_exists(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<(), PersistenceError> {
    let exists = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM station_keys WHERE id = ?1")
        .bind(station_key_id)
        .fetch_one(connection)
        .await?;
    if exists == 0 {
        return Err(PersistenceError::NotFound);
    }
    Ok(())
}

async fn station_credentials<'e, E>(
    executor: E,
    station_id: &str,
) -> Result<StationCredentials, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query(
        r#"
        SELECT station_id, login_username, login_password, login_password_secret_id,
               access_token_secret_id, refresh_token_secret_id, cookie_secret_id,
               remember_password, login_status, login_error, last_login_at,
               session_status, session_expires_at, newapi_user_id, token_expires_at,
               token_refreshed_at, session_source, updated_at
        FROM station_credentials
        WHERE station_id = ?1
        "#,
    )
    .bind(station_id)
    .fetch_optional(executor)
    .await?;
    Ok(row
        .map(row_to_station_credentials)
        .unwrap_or_else(|| default_station_credentials(station_id)))
}

fn row_to_station_credentials(row: sqlx::sqlite::SqliteRow) -> StationCredentials {
    let login_password: Option<String> = row.get("login_password");
    StationCredentials {
        station_id: row.get("station_id"),
        login_username: row.get("login_username"),
        password_present: row
            .get::<Option<String>, _>("login_password_secret_id")
            .is_some()
            || login_password.is_some_and(|value| !value.trim().is_empty()),
        access_token_present: row
            .get::<Option<String>, _>("access_token_secret_id")
            .is_some(),
        refresh_token_present: row
            .get::<Option<String>, _>("refresh_token_secret_id")
            .is_some(),
        cookie_present: row.get::<Option<String>, _>("cookie_secret_id").is_some(),
        remember_password: i64_to_bool(row.get("remember_password")),
        login_status: row.get("login_status"),
        login_error: row.get("login_error"),
        last_login_at: row.get("last_login_at"),
        session_status: row.get("session_status"),
        session_expires_at: row.get("session_expires_at"),
        newapi_user_id: row.get("newapi_user_id"),
        token_expires_at: row.get("token_expires_at"),
        token_refreshed_at: row.get("token_refreshed_at"),
        session_source: row.get("session_source"),
        updated_at: row.get("updated_at"),
    }
}

fn default_station_credentials(station_id: &str) -> StationCredentials {
    StationCredentials {
        station_id: station_id.to_string(),
        login_username: None,
        password_present: false,
        access_token_present: false,
        refresh_token_present: false,
        cookie_present: false,
        remember_password: false,
        login_status: "unknown".to_string(),
        login_error: None,
        last_login_at: None,
        session_status: "none".to_string(),
        session_expires_at: None,
        newapi_user_id: None,
        token_expires_at: None,
        token_refreshed_at: None,
        session_source: "none".to_string(),
        updated_at: None,
    }
}

fn station_credential_secret_column(kind: &str) -> Result<&'static str, PersistenceError> {
    match kind {
        "login_password" => Ok("login_password_secret_id"),
        "access_token" => Ok("access_token_secret_id"),
        "refresh_token" => Ok("refresh_token_secret_id"),
        "cookie" => Ok("cookie_secret_id"),
        _ => Err(PersistenceError::ConstraintViolation),
    }
}

async fn station_credential_secret_id(
    connection: &mut SqliteConnection,
    station_id: &str,
    column: &str,
) -> Result<Option<String>, PersistenceError> {
    let query = format!("SELECT {column} FROM station_credentials WHERE station_id = ?1");
    Ok(sqlx::query(&query)
        .bind(station_id)
        .fetch_optional(connection)
        .await?
        .and_then(|row| row.get(column)))
}

async fn station_session_secret_ids(
    connection: &mut SqliteConnection,
    station_id: &str,
) -> Result<(Option<String>, Option<String>, Option<String>), PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT access_token_secret_id, refresh_token_secret_id, cookie_secret_id
        FROM station_credentials
        WHERE station_id = ?1
        "#,
    )
    .bind(station_id)
    .fetch_optional(connection)
    .await?;
    Ok(row
        .map(|row| {
            (
                row.get("access_token_secret_id"),
                row.get("refresh_token_secret_id"),
                row.get("cookie_secret_id"),
            )
        })
        .unwrap_or((None, None, None)))
}

async fn refresh_station_session_status(
    connection: &mut SqliteConnection,
    station_id: &str,
    now: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        r#"
        UPDATE station_credentials
        SET session_status = CASE
                WHEN access_token_secret_id IS NOT NULL
                  OR refresh_token_secret_id IS NOT NULL
                  OR cookie_secret_id IS NOT NULL
                THEN 'valid'
                ELSE 'manual_required'
            END,
            updated_at = ?1
        WHERE station_id = ?2
        "#,
    )
    .bind(now)
    .bind(station_id)
    .execute(connection)
    .await?;
    Ok(())
}

async fn station_all_credential_secret_ids(
    connection: &mut SqliteConnection,
    station_id: &str,
) -> Result<[Option<String>; 4], PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT login_password_secret_id, access_token_secret_id,
               refresh_token_secret_id, cookie_secret_id
        FROM station_credentials
        WHERE station_id = ?1
        "#,
    )
    .bind(station_id)
    .fetch_optional(connection)
    .await?;
    Ok(row
        .map(|row| {
            [
                row.get("login_password_secret_id"),
                row.get("access_token_secret_id"),
                row.get("refresh_token_secret_id"),
                row.get("cookie_secret_id"),
            ]
        })
        .unwrap_or([None, None, None, None]))
}

async fn upsert_or_existing_secret(
    connection: &mut SqliteConnection,
    secret: Option<&EncryptedSecretRow>,
    existing_secret_id: Option<&str>,
) -> Result<Option<String>, PersistenceError> {
    match secret {
        Some(secret) => upsert_secret(connection, secret).await.map(Some),
        None => Ok(existing_secret_id.map(ToString::to_string)),
    }
}

async fn delete_unreferenced_secret(
    connection: &mut SqliteConnection,
    secret_id: Option<&str>,
) -> Result<(), PersistenceError> {
    let Some(secret_id) = secret_id else {
        return Ok(());
    };
    sqlx::query(
        r#"
        DELETE FROM secrets
        WHERE id = ?1
          AND NOT EXISTS (
                SELECT 1 FROM stations WHERE api_key_secret_id = ?1
                UNION ALL SELECT 1 FROM station_keys WHERE api_key_secret_id = ?1
                UNION ALL SELECT 1 FROM station_credentials WHERE login_password_secret_id = ?1
                UNION ALL SELECT 1 FROM station_credentials WHERE access_token_secret_id = ?1
                UNION ALL SELECT 1 FROM station_credentials WHERE refresh_token_secret_id = ?1
                UNION ALL SELECT 1 FROM station_credentials WHERE cookie_secret_id = ?1
          )
        "#,
    )
    .bind(secret_id)
    .execute(connection)
    .await?;
    Ok(())
}

fn row_to_stored_secret(row: sqlx::sqlite::SqliteRow) -> StoredEncryptedSecret {
    StoredEncryptedSecret {
        id: row.get("id"),
        scope: row.get("scope"),
        owner_id: row.get("owner_id"),
        kind: row.get("kind"),
        masked_value: row.get("masked_value"),
        ciphertext: row.get("ciphertext"),
        nonce: row.get("nonce"),
    }
}

async fn upsert_secret(
    connection: &mut SqliteConnection,
    secret: &EncryptedSecretRow,
) -> Result<String, PersistenceError> {
    sqlx::query(
        r#"
        INSERT INTO secrets (
            id, scope, owner_id, kind, masked_value, ciphertext, nonce, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        ON CONFLICT(scope, owner_id, kind) DO UPDATE SET
            masked_value = excluded.masked_value,
            ciphertext = excluded.ciphertext,
            nonce = excluded.nonce,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(&secret.id)
    .bind(&secret.scope)
    .bind(&secret.owner_id)
    .bind(&secret.kind)
    .bind(&secret.masked_value)
    .bind(&secret.ciphertext)
    .bind(&secret.nonce)
    .bind(&secret.now)
    .bind(&secret.now)
    .execute(&mut *connection)
    .await?;
    secret_row_id(connection, &secret.scope, &secret.owner_id, &secret.kind).await
}

async fn secret_row_id(
    connection: &mut SqliteConnection,
    scope: &str,
    owner_id: &str,
    kind: &str,
) -> Result<String, PersistenceError> {
    let row =
        sqlx::query("SELECT id FROM secrets WHERE scope = ?1 AND owner_id = ?2 AND kind = ?3")
            .bind(scope)
            .bind(owner_id)
            .bind(kind)
            .fetch_one(connection)
            .await?;
    Ok(row.get("id"))
}

async fn station_key_by_id(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<StationKey, PersistenceError> {
    let row = sqlx::query(
        r#"
        SELECT id, station_id, name, api_key, api_key_secret_id,
               (SELECT masked_value FROM secrets WHERE secrets.id = station_keys.api_key_secret_id) AS api_key_masked,
               enabled, priority, max_concurrency, load_factor, schedulable,
               group_name, tier_label, group_binding_id, group_id_hash,
               rate_multiplier, manual_rate_multiplier, manual_rate_updated_at,
               rate_source, rate_collected_at, balance_scope, status,
               last_checked_at, last_used_at, note, created_at, updated_at
        FROM station_keys
        WHERE id = ?1
        "#,
    )
    .bind(station_key_id)
    .fetch_optional(connection)
    .await?;
    row.map(row_to_station_key)
        .transpose()?
        .ok_or(PersistenceError::NotFound)
}

async fn list_station_keys<'e, E>(
    executor: E,
    station_id: &str,
) -> Result<Vec<StationKey>, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        r#"
        SELECT id, station_id, name, api_key, api_key_secret_id,
               (SELECT masked_value FROM secrets WHERE secrets.id = station_keys.api_key_secret_id) AS api_key_masked,
               enabled, priority, max_concurrency, load_factor, schedulable,
               group_name, tier_label, group_binding_id, group_id_hash,
               rate_multiplier, manual_rate_multiplier, manual_rate_updated_at,
               rate_source, rate_collected_at, balance_scope, status,
               last_checked_at, last_used_at, note, created_at, updated_at
        FROM station_keys
        WHERE station_id = ?1
        ORDER BY priority ASC, created_at ASC, id ASC
        "#,
    )
    .bind(station_id)
    .fetch_all(executor)
    .await?;
    rows.into_iter().map(row_to_station_key).collect()
}

async fn list_key_pool_items<'e, E>(executor: E) -> Result<Vec<KeyPoolItem>, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        r#"
        SELECT k.id, k.station_id, s.name AS station_name, s.station_type,
               s.api_base_url AS station_api_base_url,
               s.endpoint_revision AS station_endpoint_revision,
               s.upstream_api_format AS station_upstream_api_format,
               k.name, k.api_key, sec.masked_value AS api_key_masked,
               k.api_key_secret_id, k.enabled, k.priority, k.max_concurrency,
               k.load_factor, k.schedulable, k.group_name, k.tier_label,
               k.group_binding_id, k.group_id_hash, k.rate_multiplier,
               k.manual_rate_multiplier, k.manual_rate_updated_at, k.rate_source,
               k.rate_collected_at, k.balance_scope, k.status, k.last_checked_at,
               k.last_used_at, k.note, k.created_at, k.updated_at,
               COALESCE(c.supports_chat_completions, 1) AS supports_chat_completions,
               COALESCE(c.supports_responses, 1) AS supports_responses,
               COALESCE(c.supports_embeddings, 1) AS supports_embeddings,
               COALESCE(c.supports_stream, 1) AS supports_stream,
               COALESCE(c.supports_tools, 1) AS supports_tools,
               COALESCE(c.supports_vision, 1) AS supports_vision,
               COALESCE(c.supports_reasoning, 1) AS supports_reasoning,
               COALESCE(c.model_allowlist_json, '[]') AS model_allowlist_json,
               COALESCE(c.model_blocklist_json, '[]') AS model_blocklist_json,
               COALESCE(c.preferred_models_json, '[]') AS preferred_models_json,
               COALESCE(c.only_use_as_backup, 0) AS only_use_as_backup,
               h.cooldown_until, COALESCE(h.success_count, 0) AS success_count,
               COALESCE(h.failure_count, 0) AS failure_count, h.avg_latency_ms,
               COALESCE(h.consecutive_failures, 0) AS consecutive_failures,
               h.last_error_summary,
               COALESCE(eh.status, 'unchecked') AS endpoint_ping_status,
               eh.latency_ms AS endpoint_ping_ms,
               eh.checked_at AS endpoint_ping_checked_at,
               eh.error_summary AS endpoint_ping_error
        FROM station_keys k
        JOIN stations s ON s.id = k.station_id
        LEFT JOIN secrets sec ON sec.id = k.api_key_secret_id
        LEFT JOIN station_key_capabilities c ON c.station_key_id = k.id
        LEFT JOIN station_key_health h
               ON h.station_key_id = k.id
              AND h.endpoint_revision = s.endpoint_revision
        LEFT JOIN station_endpoint_health eh
               ON eh.station_id = s.id
              AND eh.endpoint_revision = s.endpoint_revision
        ORDER BY COALESCE(k.routing_order, k.priority) ASC,
                 k.priority ASC, k.created_at ASC, k.id ASC
        "#,
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter().map(row_to_key_pool_item).collect()
}

fn row_to_key_pool_item(row: sqlx::sqlite::SqliteRow) -> Result<KeyPoolItem, PersistenceError> {
    let api_key: String = row.get("api_key");
    let api_key_secret_id: Option<String> = row.get("api_key_secret_id");
    let supports_chat = i64_to_bool(row.get("supports_chat_completions"));
    let supports_responses = i64_to_bool(row.get("supports_responses"));
    let supports_embeddings = i64_to_bool(row.get("supports_embeddings"));
    let supports_stream = i64_to_bool(row.get("supports_stream"));
    let supports_tools = i64_to_bool(row.get("supports_tools"));
    let supports_vision = i64_to_bool(row.get("supports_vision"));
    let supports_reasoning = i64_to_bool(row.get("supports_reasoning"));
    let allowlist = parse_json_string_list(row.get("model_allowlist_json"));
    let blocklist = parse_json_string_list(row.get("model_blocklist_json"));
    let preferred_models = parse_json_string_list(row.get("preferred_models_json"));
    let success_count: i64 = row.get("success_count");
    let failure_count: i64 = row.get("failure_count");
    Ok(KeyPoolItem {
        id: row.get("id"),
        station_id: row.get("station_id"),
        station_name: row.get("station_name"),
        station_type: row.get("station_type"),
        station_api_base_url: row.get("station_api_base_url"),
        station_endpoint_revision: row.get("station_endpoint_revision"),
        station_upstream_api_format: row.get("station_upstream_api_format"),
        name: row.get("name"),
        api_key_masked: row
            .get::<Option<String>, _>("api_key_masked")
            .unwrap_or_else(|| mask_secret(&api_key)),
        api_key_present: api_key_secret_id.is_some() || !api_key.trim().is_empty(),
        enabled: i64_to_bool(row.get("enabled")),
        priority: row.get("priority"),
        max_concurrency: row.get("max_concurrency"),
        load_factor: row.get("load_factor"),
        schedulable: i64_to_bool(row.get("schedulable")),
        group_name: row.get("group_name"),
        tier_label: row.get("tier_label"),
        group_binding_id: row.get("group_binding_id"),
        group_id_hash: row.get("group_id_hash"),
        rate_multiplier: row.get("rate_multiplier"),
        manual_rate_multiplier: row.get("manual_rate_multiplier"),
        manual_rate_updated_at: row.get("manual_rate_updated_at"),
        rate_source: row.get("rate_source"),
        rate_collected_at: row.get("rate_collected_at"),
        balance_scope: row.get("balance_scope"),
        status: row.get("status"),
        last_checked_at: row.get("last_checked_at"),
        last_used_at: row.get("last_used_at"),
        note: row.get("note"),
        capability_summary: summarize_capabilities(
            supports_chat,
            supports_responses,
            supports_embeddings,
            supports_stream,
            supports_tools,
            supports_vision,
            supports_reasoning,
        ),
        model_scope_summary: summarize_model_scope(
            allowlist.len(),
            blocklist.len(),
            preferred_models.len(),
        ),
        only_use_as_backup: i64_to_bool(row.get("only_use_as_backup")),
        cooldown_until: row.get("cooldown_until"),
        success_rate: success_rate(success_count, failure_count),
        avg_latency_ms: row.get("avg_latency_ms"),
        consecutive_failures: row.get("consecutive_failures"),
        last_error_summary: row.get("last_error_summary"),
        endpoint_ping_status: row.get("endpoint_ping_status"),
        endpoint_ping_ms: row.get("endpoint_ping_ms"),
        endpoint_ping_checked_at: row.get("endpoint_ping_checked_at"),
        endpoint_ping_error: row.get("endpoint_ping_error"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

fn row_to_station_key(row: sqlx::sqlite::SqliteRow) -> Result<StationKey, PersistenceError> {
    let api_key: String = row.get("api_key");
    let api_key_secret_id: Option<String> = row.get("api_key_secret_id");
    let api_key_masked = row
        .get::<Option<String>, _>("api_key_masked")
        .unwrap_or_else(|| mask_secret(&api_key));
    Ok(StationKey {
        id: row.get("id"),
        station_id: row.get("station_id"),
        name: row.get("name"),
        api_key_masked,
        api_key_present: api_key_secret_id.is_some() || !api_key.trim().is_empty(),
        enabled: i64_to_bool(row.get("enabled")),
        priority: row.get("priority"),
        max_concurrency: row.get("max_concurrency"),
        load_factor: row.get("load_factor"),
        schedulable: i64_to_bool(row.get("schedulable")),
        group_name: row.get("group_name"),
        tier_label: row.get("tier_label"),
        group_binding_id: row.get("group_binding_id"),
        group_id_hash: row.get("group_id_hash"),
        rate_multiplier: row.get("rate_multiplier"),
        manual_rate_multiplier: row.get("manual_rate_multiplier"),
        manual_rate_updated_at: row.get("manual_rate_updated_at"),
        rate_source: row.get("rate_source"),
        rate_collected_at: row.get("rate_collected_at"),
        balance_scope: row.get("balance_scope"),
        status: row.get("status"),
        last_checked_at: row.get("last_checked_at"),
        last_used_at: row.get("last_used_at"),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

async fn station_key_secret_id(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<Option<String>, PersistenceError> {
    let row = sqlx::query("SELECT api_key_secret_id FROM station_keys WHERE id = ?1")
        .bind(station_key_id)
        .fetch_optional(connection)
        .await?;
    row.map(|row| row.get("api_key_secret_id"))
        .ok_or(PersistenceError::NotFound)
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "credential integration targets verify station-scoped secret ownership"
    )
)]
async fn station_key_secret_id_for_station(
    connection: &mut SqliteConnection,
    station_id: &str,
    station_key_id: &str,
) -> Result<Option<String>, PersistenceError> {
    let row =
        sqlx::query("SELECT api_key_secret_id FROM station_keys WHERE id = ?1 AND station_id = ?2")
            .bind(station_key_id)
            .bind(station_id)
            .fetch_optional(connection)
            .await?;
    row.map(|row| row.get("api_key_secret_id"))
        .ok_or(PersistenceError::NotFound)
}

async fn existing_manual_rate_multiplier(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<Option<f64>, PersistenceError> {
    let row = sqlx::query("SELECT manual_rate_multiplier FROM station_keys WHERE id = ?1")
        .bind(station_key_id)
        .fetch_optional(connection)
        .await?;
    row.map(|row| row.get("manual_rate_multiplier"))
        .ok_or(PersistenceError::NotFound)
}

async fn existing_manual_rate_updated_at(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<Option<String>, PersistenceError> {
    let row = sqlx::query("SELECT manual_rate_updated_at FROM station_keys WHERE id = ?1")
        .bind(station_key_id)
        .fetch_optional(connection)
        .await?;
    row.map(|row| row.get("manual_rate_updated_at"))
        .ok_or(PersistenceError::NotFound)
}

async fn next_station_key_priority(
    connection: &mut SqliteConnection,
    station_id: &str,
) -> Result<i64, PersistenceError> {
    let row = sqlx::query(
        "SELECT COALESCE(MAX(priority), -1) + 1 AS next_priority FROM station_keys WHERE station_id = ?1",
    )
    .bind(station_id)
    .fetch_one(connection)
    .await?;
    Ok(row.get("next_priority"))
}

async fn normalize_station_key_priorities(
    connection: &mut SqliteConnection,
    station_id: &str,
) -> Result<(), PersistenceError> {
    let ids = sqlx::query(
        r#"
        SELECT id FROM station_keys
        WHERE station_id = ?1
        ORDER BY priority ASC, created_at ASC, id ASC
        "#,
    )
    .bind(station_id)
    .fetch_all(&mut *connection)
    .await?
    .into_iter()
    .map(|row| row.get::<String, _>("id"))
    .collect::<Vec<_>>();
    for (priority, id) in ids.into_iter().enumerate() {
        sqlx::query("UPDATE station_keys SET priority = ?1 WHERE id = ?2")
            .bind(priority as i64)
            .bind(id)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

async fn station_id_for_key(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<String, PersistenceError> {
    let row = sqlx::query("SELECT station_id FROM station_keys WHERE id = ?1")
        .bind(station_key_id)
        .fetch_optional(connection)
        .await?;
    row.map(|row| row.get("station_id"))
        .ok_or(PersistenceError::NotFound)
}

async fn remote_station_key_by_id(
    connection: &mut SqliteConnection,
    remote_key_id: &str,
) -> Result<RemoteStationKey, PersistenceError> {
    let row = sqlx::query(remote_station_key_select("WHERE id = ?1").as_str())
        .bind(remote_key_id)
        .fetch_optional(connection)
        .await?;
    row.map(row_to_remote_station_key)
        .transpose()?
        .ok_or(PersistenceError::NotFound)
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "credential integration targets resolve remote-key identity through application upsert"
    )
)]
async fn remote_station_key_by_identity(
    connection: &mut SqliteConnection,
    station_id: &str,
    remote_key_id_hash: Option<&str>,
    fallback_id: &str,
) -> Result<RemoteStationKey, PersistenceError> {
    let row = match remote_key_id_hash {
        Some(remote_key_id_hash) => {
            let query =
                remote_station_key_select("WHERE station_id = ?1 AND remote_key_id_hash = ?2");
            sqlx::query(&query)
                .bind(station_id)
                .bind(remote_key_id_hash)
                .fetch_optional(&mut *connection)
                .await?
        }
        None => None,
    };
    match row {
        Some(row) => row_to_remote_station_key(row),
        None => remote_station_key_by_id(connection, fallback_id).await,
    }
}

async fn list_remote_station_keys<'e, E>(
    executor: E,
    station_id: &str,
) -> Result<Vec<RemoteStationKey>, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let query =
        remote_station_key_select("WHERE station_id = ?1 ORDER BY collected_at DESC, id ASC");
    let rows = sqlx::query(query.as_str())
        .bind(station_id)
        .fetch_all(executor)
        .await?;
    rows.into_iter().map(row_to_remote_station_key).collect()
}

fn remote_station_key_select(predicate: &str) -> String {
    format!(
        r#"
        SELECT id, station_id, remote_key_id_hash, remote_key_name, api_key_masked,
               api_key_fingerprint, group_id_hash, group_name, tier_label, rate_multiplier,
               rate_source, created_at, last_used_at, raw_source, match_status,
               matched_station_key_id, match_confidence, collected_at
        FROM remote_station_keys
        {predicate}
        "#
    )
}

fn row_to_remote_station_key(
    row: sqlx::sqlite::SqliteRow,
) -> Result<RemoteStationKey, PersistenceError> {
    let match_status: String = row.get("match_status");
    Ok(RemoteStationKey {
        id: row.get("id"),
        station_id: row.get("station_id"),
        remote_key_id_hash: row.get("remote_key_id_hash"),
        remote_key_name: row.get("remote_key_name"),
        api_key_masked: row.get("api_key_masked"),
        api_key_fingerprint: row.get("api_key_fingerprint"),
        group_id_hash: row.get("group_id_hash"),
        group_name: row.get("group_name"),
        tier_label: row.get("tier_label"),
        rate_multiplier: row.get("rate_multiplier"),
        rate_source: row.get("rate_source"),
        created_at: row.get("created_at"),
        last_used_at: row.get("last_used_at"),
        raw_source: row.get("raw_source"),
        match_status: RemoteKeyMatchStatus::from_str(&match_status),
        matched_station_key_id: row.get("matched_station_key_id"),
        match_confidence: row.get("match_confidence"),
        collected_at: row.get("collected_at"),
    })
}

async fn station_key_capabilities<'e, E>(
    executor: E,
    station_key_id: &str,
) -> Result<StationKeyCapabilities, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query(
        r#"
        SELECT station_key_id, supports_chat_completions, supports_responses,
               supports_embeddings, supports_stream, supports_tools, supports_vision,
               supports_reasoning, model_allowlist_json, model_blocklist_json,
               preferred_models_json, only_use_as_backup, routing_tags_json, updated_at
        FROM station_key_capabilities
        WHERE station_key_id = ?1
        "#,
    )
    .bind(station_key_id)
    .fetch_optional(executor)
    .await?;
    Ok(row
        .map(row_to_station_key_capabilities)
        .unwrap_or_else(|| default_station_key_capabilities(station_key_id)))
}

fn row_to_station_key_capabilities(row: sqlx::sqlite::SqliteRow) -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: row.get("station_key_id"),
        supports_chat_completions: i64_to_bool(row.get("supports_chat_completions")),
        supports_responses: i64_to_bool(row.get("supports_responses")),
        supports_embeddings: i64_to_bool(row.get("supports_embeddings")),
        supports_stream: i64_to_bool(row.get("supports_stream")),
        supports_tools: i64_to_bool(row.get("supports_tools")),
        supports_vision: i64_to_bool(row.get("supports_vision")),
        supports_reasoning: i64_to_bool(row.get("supports_reasoning")),
        model_allowlist: parse_json_string_list(row.get("model_allowlist_json")),
        model_blocklist: parse_json_string_list(row.get("model_blocklist_json")),
        preferred_models: parse_json_string_list(row.get("preferred_models_json")),
        only_use_as_backup: i64_to_bool(row.get("only_use_as_backup")),
        routing_tags: parse_json_string_list(row.get("routing_tags_json")),
        updated_at: row.get("updated_at"),
    }
}

fn default_station_key_capabilities(station_key_id: &str) -> StationKeyCapabilities {
    StationKeyCapabilities {
        station_key_id: station_key_id.to_string(),
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
        updated_at: "0".to_string(),
    }
}

fn validate_station_key_fields(name: &str, max_concurrency: i64) -> Result<(), PersistenceError> {
    if name.trim().is_empty() || max_concurrency <= 0 {
        return Err(PersistenceError::NotFound);
    }
    Ok(())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
}

fn normalize_required_string(value: String, fallback: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        fallback.to_string()
    } else {
        value.to_string()
    }
}

fn serialize_string_list(values: &[String]) -> Result<String, PersistenceError> {
    let normalized = values
        .iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect::<Vec<_>>();
    serde_json::to_string(&normalized)
        .map_err(|error| PersistenceError::InvariantViolation(error.to_string()))
}

fn parse_json_string_list(value: String) -> Vec<String> {
    serde_json::from_str(&value).unwrap_or_default()
}

fn summarize_capabilities(
    supports_chat: bool,
    supports_responses: bool,
    supports_embeddings: bool,
    supports_stream: bool,
    supports_tools: bool,
    supports_vision: bool,
    supports_reasoning: bool,
) -> Vec<String> {
    [
        (supports_chat, "Chat"),
        (supports_responses, "Responses"),
        (supports_embeddings, "Embeddings"),
        (supports_stream, "Stream"),
        (supports_tools, "Tools"),
        (supports_vision, "Vision"),
        (supports_reasoning, "Reasoning"),
    ]
    .into_iter()
    .filter(|(enabled, _)| *enabled)
    .map(|(_, label)| label.to_string())
    .collect()
}

fn summarize_model_scope(allowlist: usize, blocklist: usize, preferred: usize) -> String {
    let mut parts = Vec::new();
    if allowlist == 0 {
        parts.push("全部模型".to_string());
    } else {
        parts.push(format!("允许 {allowlist} 个模型"));
    }
    if blocklist > 0 {
        parts.push(format!("屏蔽 {blocklist} 个"));
    }
    if preferred > 0 {
        parts.push(format!("优先 {preferred} 个"));
    }
    parts.join("，")
}

fn success_rate(success_count: i64, failure_count: i64) -> Option<f64> {
    let total = success_count + failure_count;
    (total > 0).then(|| success_count as f64 / total as f64)
}

fn bool_to_i64(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

fn i64_to_bool(value: i64) -> bool {
    value != 0
}
