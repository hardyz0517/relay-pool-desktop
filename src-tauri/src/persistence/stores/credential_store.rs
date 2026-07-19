use sqlx::{Executor, Row, Sqlite, SqliteConnection};

use crate::{
    models::{
        remote_keys::{RemoteKeyMatchStatus, RemoteStationKey},
        station_keys::StationKey,
    },
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
    services::secrets::mask::mask_secret,
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

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CredentialStore;

impl CredentialStore {
    pub(crate) async fn list_station_keys(
        &self,
        read: &mut ReadSession,
        station_id: &str,
    ) -> Result<Vec<StationKey>, PersistenceError> {
        list_station_keys(read.connection(), station_id).await
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
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
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
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
        }
        station_key_by_id(write.connection(), &patch.id).await
    }

    pub(crate) async fn station_key_by_id(
        &self,
        read: &mut ReadSession,
        station_key_id: &str,
    ) -> Result<StationKey, PersistenceError> {
        station_key_by_id(read.connection(), station_key_id).await
    }

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
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
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
                return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
            }
        }
        list_station_keys(write.connection(), station_id).await
    }

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
        remote_station_key_by_id(write.connection(), &row.id).await
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
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
        }
        let station_id = station_id_for_key(write.connection(), station_key_id).await?;
        list_remote_station_keys(write.connection(), &station_id).await
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
    Ok(secret_row_id(connection, &secret.scope, &secret.owner_id, &secret.kind).await?)
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
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
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
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

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
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
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
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
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
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
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

async fn station_id_for_key(
    connection: &mut SqliteConnection,
    station_key_id: &str,
) -> Result<String, PersistenceError> {
    let row = sqlx::query("SELECT station_id FROM station_keys WHERE id = ?1")
        .bind(station_key_id)
        .fetch_optional(connection)
        .await?;
    row.map(|row| row.get("station_id"))
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
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
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
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

fn validate_station_key_fields(name: &str, max_concurrency: i64) -> Result<(), PersistenceError> {
    if name.trim().is_empty() || max_concurrency <= 0 {
        return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
    }
    Ok(())
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|item| item.trim().to_string())
        .filter(|item| !item.is_empty())
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
