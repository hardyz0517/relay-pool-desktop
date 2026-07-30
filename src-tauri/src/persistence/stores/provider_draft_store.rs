use std::collections::HashSet;

use sqlx::Row;

use crate::{
    models::provider_drafts::{ProviderDraft, ProviderDraftPayload, ProviderDraftPreview},
    persistence::{
        error::PersistenceError,
        read_session::ReadSession,
        stores::credential_store::{EncryptedSecretRow, StoredEncryptedSecret},
        write_session::WriteSession,
    },
};

const DRAFT_SECRET_SCOPE: &str = "provider_draft";
const KEY_SECRET_PREFIX: &str = "key_api_key:";

#[derive(Debug, Clone)]
pub(crate) struct NewProviderDraftRow {
    pub(crate) id: String,
    pub(crate) base_station_id: Option<String>,
    pub(crate) payload: ProviderDraftPayload,
    pub(crate) now: String,
    pub(crate) expires_at: String,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ProviderDraftStore;

impl ProviderDraftStore {
    pub(crate) async fn latest_active_create(
        &self,
        read: &mut ReadSession,
        now_ms: i64,
    ) -> Result<Option<ProviderDraft>, PersistenceError> {
        let id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT id
            FROM provider_drafts
            WHERE state = 'active'
              AND base_station_id IS NULL
              AND CAST(expires_at AS INTEGER) > ?1
            ORDER BY updated_at DESC, id DESC
            LIMIT 1
            "#,
        )
        .bind(now_ms)
        .fetch_optional(read.connection())
        .await?;
        match id {
            Some(id) => self
                .get_from_connection(read.connection(), &id)
                .await
                .map(Some),
            None => Ok(None),
        }
    }

    pub(crate) async fn delete_expired(
        &self,
        write: &mut WriteSession,
        now_ms: i64,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            DELETE FROM secrets
            WHERE scope = ?1
              AND owner_id IN (
                  SELECT id FROM provider_drafts
                  WHERE state = 'active' AND CAST(expires_at AS INTEGER) <= ?2
              )
            "#,
        )
        .bind(DRAFT_SECRET_SCOPE)
        .bind(now_ms)
        .execute(write.connection())
        .await?;
        sqlx::query(
            "DELETE FROM provider_drafts WHERE state = 'active' AND CAST(expires_at AS INTEGER) <= ?1",
        )
        .bind(now_ms)
        .execute(write.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn get(
        &self,
        read: &mut ReadSession,
        draft_id: &str,
    ) -> Result<ProviderDraft, PersistenceError> {
        self.get_from_connection(read.connection(), draft_id).await
    }

    pub(crate) async fn get_for_write(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
    ) -> Result<ProviderDraft, PersistenceError> {
        self.get_from_connection(write.connection(), draft_id).await
    }

    pub(crate) async fn committed_station_for_key(
        &self,
        read: &mut ReadSession,
        draft_id: &str,
        commit_key: &str,
    ) -> Result<Option<String>, PersistenceError> {
        sqlx::query_scalar(
            r#"
            SELECT committed_station_id
            FROM provider_drafts
            WHERE id = ?1 AND state = 'committed' AND commit_key = ?2
            "#,
        )
        .bind(draft_id)
        .bind(commit_key)
        .fetch_optional(read.connection())
        .await
        .map_err(Into::into)
    }

    pub(crate) async fn insert(
        &self,
        write: &mut WriteSession,
        row: NewProviderDraftRow,
    ) -> Result<ProviderDraft, PersistenceError> {
        let payload_json = serialize_payload(&row.payload)?;
        sqlx::query(
            r#"
            INSERT INTO provider_drafts (
                id, base_station_id, revision, state, payload_schema_version,
                payload_json, commit_key, committed_station_id,
                created_at, updated_at, expires_at
            ) VALUES (?1, ?2, 1, 'active', 1, ?3, NULL, NULL, ?4, ?4, ?5)
            "#,
        )
        .bind(&row.id)
        .bind(&row.base_station_id)
        .bind(payload_json)
        .bind(&row.now)
        .bind(&row.expires_at)
        .execute(write.connection())
        .await?;
        self.get_from_connection(write.connection(), &row.id).await
    }

    pub(crate) async fn patch_payload(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
        expected_revision: i64,
        payload: &ProviderDraftPayload,
        now: &str,
        expires_at: &str,
    ) -> Result<(), PersistenceError> {
        let payload_json = serialize_payload(payload)?;
        let updated = sqlx::query(
            r#"
            UPDATE provider_drafts
            SET payload_json = ?1,
                revision = revision + 1,
                updated_at = ?2,
                expires_at = ?3
            WHERE id = ?4 AND state = 'active' AND revision = ?5
            "#,
        )
        .bind(payload_json)
        .bind(now)
        .bind(expires_at)
        .bind(draft_id)
        .bind(expected_revision)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::StaleRevision);
        }
        Ok(())
    }

    pub(crate) async fn upsert_secret(
        &self,
        write: &mut WriteSession,
        secret: EncryptedSecretRow,
    ) -> Result<(), PersistenceError> {
        sqlx::query(
            r#"
            INSERT INTO secrets (
                id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                key_id, encryption_version, value_hash, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)
            ON CONFLICT(scope, owner_id, kind) DO UPDATE SET
                id = excluded.id,
                masked_value = excluded.masked_value,
                ciphertext = excluded.ciphertext,
                nonce = excluded.nonce,
                key_id = excluded.key_id,
                encryption_version = excluded.encryption_version,
                value_hash = excluded.value_hash,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(secret.id)
        .bind(secret.scope)
        .bind(secret.owner_id)
        .bind(secret.kind)
        .bind(secret.masked_value)
        .bind(secret.ciphertext)
        .bind(secret.nonce)
        .bind(secret.key_id)
        .bind(i64::from(secret.encryption_version))
        .bind(secret.value_hash)
        .bind(secret.now)
        .execute(write.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn delete_secret(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
        kind: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM secrets WHERE scope = ?1 AND owner_id = ?2 AND kind = ?3")
            .bind(DRAFT_SECRET_SCOPE)
            .bind(draft_id)
            .bind(kind)
            .execute(write.connection())
            .await?;
        Ok(())
    }

    pub(crate) async fn delete_key_secrets_not_in(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
        retained_client_ids: &HashSet<String>,
    ) -> Result<(), PersistenceError> {
        let kinds = sqlx::query_scalar::<_, String>(
            "SELECT kind FROM secrets WHERE scope = ?1 AND owner_id = ?2 AND kind LIKE 'key_api_key:%'",
        )
        .bind(DRAFT_SECRET_SCOPE)
        .bind(draft_id)
        .fetch_all(write.connection())
        .await?;
        for kind in kinds {
            let retained = kind
                .strip_prefix(KEY_SECRET_PREFIX)
                .is_some_and(|client_id| retained_client_ids.contains(client_id));
            if !retained {
                self.delete_secret(write, draft_id, &kind).await?;
            }
        }
        Ok(())
    }

    pub(crate) async fn secret(
        &self,
        read: &mut ReadSession,
        draft_id: &str,
        kind: &str,
    ) -> Result<Option<StoredEncryptedSecret>, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                   key_id, encryption_version, value_hash
            FROM secrets
            WHERE scope = ?1 AND owner_id = ?2 AND kind = ?3
            "#,
        )
        .bind(DRAFT_SECRET_SCOPE)
        .bind(draft_id)
        .bind(kind)
        .fetch_optional(read.connection())
        .await?;
        Ok(row.map(row_to_stored_secret))
    }

    pub(crate) async fn upsert_preview(
        &self,
        write: &mut WriteSession,
        preview: &ProviderDraftPreview,
        now: &str,
    ) -> Result<(), PersistenceError> {
        let result_json = serde_json::to_string(preview).map_err(invalid_json)?;
        sqlx::query(
            r#"
            INSERT INTO provider_draft_previews (
                draft_id, kind, runtime_fingerprint, status, result_json, collected_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(draft_id, kind) DO UPDATE SET
                runtime_fingerprint = excluded.runtime_fingerprint,
                status = excluded.status,
                result_json = excluded.result_json,
                collected_at = excluded.collected_at,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(&preview.draft_id)
        .bind(&preview.kind)
        .bind(&preview.runtime_fingerprint)
        .bind(&preview.status)
        .bind(result_json)
        .bind(&preview.collected_at)
        .bind(now)
        .execute(write.connection())
        .await?;
        Ok(())
    }

    pub(crate) async fn mark_committed(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
        expected_revision: i64,
        commit_key: &str,
        station_id: &str,
        now: &str,
    ) -> Result<(), PersistenceError> {
        let updated = sqlx::query(
            r#"
            UPDATE provider_drafts
            SET state = 'committed', commit_key = ?1, committed_station_id = ?2, updated_at = ?3
            WHERE id = ?4 AND state = 'active' AND revision = ?5
            "#,
        )
        .bind(commit_key)
        .bind(station_id)
        .bind(now)
        .bind(draft_id)
        .bind(expected_revision)
        .execute(write.connection())
        .await?
        .rows_affected();
        if updated == 0 {
            return Err(PersistenceError::StaleRevision);
        }
        Ok(())
    }

    pub(crate) async fn discard(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM secrets WHERE scope = ?1 AND owner_id = ?2")
            .bind(DRAFT_SECRET_SCOPE)
            .bind(draft_id)
            .execute(write.connection())
            .await?;
        let deleted = sqlx::query("DELETE FROM provider_drafts WHERE id = ?1 AND state = 'active'")
            .bind(draft_id)
            .execute(write.connection())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::NotFound);
        }
        Ok(())
    }

    pub(crate) async fn delete_all_secrets(
        &self,
        write: &mut WriteSession,
        draft_id: &str,
    ) -> Result<(), PersistenceError> {
        sqlx::query("DELETE FROM secrets WHERE scope = ?1 AND owner_id = ?2")
            .bind(DRAFT_SECRET_SCOPE)
            .bind(draft_id)
            .execute(write.connection())
            .await?;
        Ok(())
    }

    async fn get_from_connection(
        &self,
        connection: &mut sqlx::SqliteConnection,
        draft_id: &str,
    ) -> Result<ProviderDraft, PersistenceError> {
        let row = sqlx::query(
            r#"
            SELECT id, base_station_id, revision, state, payload_schema_version,
                   payload_json, committed_station_id, created_at, updated_at, expires_at
            FROM provider_drafts
            WHERE id = ?1
            "#,
        )
        .bind(draft_id)
        .fetch_optional(&mut *connection)
        .await?
        .ok_or(PersistenceError::NotFound)?;
        let secret_kinds = sqlx::query_scalar::<_, String>(
            "SELECT kind FROM secrets WHERE scope = ?1 AND owner_id = ?2 ORDER BY kind ASC",
        )
        .bind(DRAFT_SECRET_SCOPE)
        .bind(draft_id)
        .fetch_all(&mut *connection)
        .await?;
        let payload_json: String = row.get("payload_json");
        let payload = serde_json::from_str(&payload_json).map_err(invalid_json)?;
        Ok(ProviderDraft {
            id: row.get("id"),
            base_station_id: row.get("base_station_id"),
            revision: row.get("revision"),
            state: row.get("state"),
            payload_schema_version: row.get("payload_schema_version"),
            payload,
            station_api_key_present: secret_kinds.iter().any(|kind| kind == "station_api_key"),
            login_password_present: secret_kinds.iter().any(|kind| kind == "login_password"),
            key_api_key_client_ids: secret_kinds
                .iter()
                .filter_map(|kind| {
                    kind.strip_prefix(KEY_SECRET_PREFIX)
                        .map(ToString::to_string)
                })
                .collect(),
            committed_station_id: row.get("committed_station_id"),
            created_at: row.get("created_at"),
            updated_at: row.get("updated_at"),
            expires_at: row.get("expires_at"),
        })
    }
}

pub(crate) fn draft_key_secret_kind(client_id: &str) -> String {
    format!("{KEY_SECRET_PREFIX}{client_id}")
}

fn serialize_payload(payload: &ProviderDraftPayload) -> Result<String, PersistenceError> {
    serde_json::to_string(payload).map_err(invalid_json)
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
        key_id: row.get("key_id"),
        encryption_version: row.get::<i64, _>("encryption_version") as u16,
        value_hash: row.get("value_hash"),
    }
}

fn invalid_json(error: serde_json::Error) -> PersistenceError {
    PersistenceError::InvariantViolation(format!("provider draft JSON is invalid: {error}"))
}
