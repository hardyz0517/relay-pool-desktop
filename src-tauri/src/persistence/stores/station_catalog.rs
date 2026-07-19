use sqlx::{Executor, Row, Sqlite, SqliteConnection};

use crate::{
    models::stations::{CreateStationInput, Station, UpdateStationInput},
    persistence::{
        error::PersistenceError, read_session::ReadSession, write_session::WriteSession,
    },
    services::{
        outbound::{normalize_proxy_mode, normalize_proxy_url},
        secrets::mask::mask_secret,
        station_endpoints::{normalize_station_endpoints, same_origin},
    },
};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StationCatalogStore;

#[derive(Debug, Clone)]
pub(crate) struct NewStationRow {
    pub(crate) id: String,
    pub(crate) now: String,
    pub(crate) input: CreateStationInput,
}

#[derive(Debug, Clone)]
pub(crate) struct StationChange {
    pub(crate) now: String,
    pub(crate) input: UpdateStationInput,
}

impl StationCatalogStore {
    pub(crate) async fn list(
        &self,
        read: &mut ReadSession,
    ) -> Result<Vec<Station>, PersistenceError> {
        list_stations(read.connection()).await
    }

    pub(crate) async fn insert(
        &self,
        write: &mut WriteSession,
        station: NewStationRow,
    ) -> Result<Station, PersistenceError> {
        validate_station_fields(
            &station.input.name,
            &station.input.station_type,
            &station.input.website_url,
            station.input.credit_per_cny,
            station.input.collection_interval_minutes,
        )?;
        let endpoints =
            normalize_station_endpoints(&station.input.website_url, &station.input.api_base_url)
                .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
        let collector_proxy_mode = normalize_proxy_mode(&station.input.collector_proxy_mode, true);
        let collector_proxy_url = normalize_proxy_url(station.input.collector_proxy_url);
        let priority = next_station_priority(write.connection()).await?;
        let api_key = station.input.api_key.trim().to_string();

        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, endpoint_revision,
                api_key, api_key_secret_id, collector_proxy_mode, collector_proxy_url,
                enabled, priority, credit_per_cny, balance_raw, balance_cny,
                low_balance_threshold_cny, collection_interval_minutes, status,
                latency_ms, last_checked_at, last_pricing_fetched_at, note, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, NULL, ?7, ?8, ?9, ?10, ?11, NULL, NULL, ?12,
                ?13, ?14, NULL, NULL, NULL, ?15, ?16, ?17)
            "#,
        )
        .bind(&station.id)
        .bind(station.input.name.trim())
        .bind(&station.input.station_type)
        .bind(&endpoints.website_url)
        .bind(&endpoints.api_base_url)
        .bind(&api_key)
        .bind(&collector_proxy_mode)
        .bind(&collector_proxy_url)
        .bind(bool_to_i64(station.input.enabled))
        .bind(priority)
        .bind(station.input.credit_per_cny)
        .bind(station.input.low_balance_threshold_cny)
        .bind(i64::from(station.input.collection_interval_minutes))
        .bind(if station.input.enabled {
            "unchecked"
        } else {
            "disabled"
        })
        .bind(normalize_optional_string(station.input.note))
        .bind(&station.now)
        .bind(&station.now)
        .execute(write.connection())
        .await?;

        station_by_id(write.connection(), &station.id).await
    }

    pub(crate) async fn update_if_revision(
        &self,
        write: &mut WriteSession,
        change: StationChange,
    ) -> Result<Station, PersistenceError> {
        validate_station_fields(
            &change.input.name,
            &change.input.station_type,
            &change.input.website_url,
            change.input.credit_per_cny,
            change.input.collection_interval_minutes,
        )?;
        update_station(write.connection(), change).await
    }

    pub(crate) async fn delete_owned_state(
        &self,
        write: &mut WriteSession,
        station_id: &str,
    ) -> Result<(), PersistenceError> {
        let deleted = sqlx::query("DELETE FROM stations WHERE id = ?1")
            .bind(station_id)
            .execute(write.connection())
            .await?
            .rows_affected();
        if deleted == 0 {
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
        }
        normalize_station_priorities(write.connection()).await
    }

    pub(crate) async fn reorder(
        &self,
        write: &mut WriteSession,
        station_ids: &[String],
        now: &str,
    ) -> Result<Vec<Station>, PersistenceError> {
        if station_ids.is_empty() {
            return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
        }
        for (index, id) in station_ids.iter().enumerate() {
            let updated =
                sqlx::query("UPDATE stations SET priority = ?1, updated_at = ?2 WHERE id = ?3")
                    .bind(index as i64)
                    .bind(now)
                    .bind(id)
                    .execute(write.connection())
                    .await?
                    .rows_affected();
            if updated == 0 {
                return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
            }
        }
        list_stations(write.connection()).await
    }
}

async fn update_station(
    connection: &mut SqliteConnection,
    change: StationChange,
) -> Result<Station, PersistenceError> {
    let existing = sqlx::query(
        r#"
        SELECT api_key, api_key_secret_id, website_url, api_base_url, endpoint_revision
        FROM stations
        WHERE id = ?1
        "#,
    )
    .bind(&change.input.id)
    .fetch_optional(&mut *connection)
    .await?;
    let Some(existing) = existing else {
        return Err(PersistenceError::Sqlx(sqlx::Error::RowNotFound));
    };
    let existing_api_key: String = existing.get("api_key");
    let existing_secret_id: Option<String> = existing.get("api_key_secret_id");
    let existing_website_url: String = existing.get("website_url");
    let existing_api_base_url: String = existing.get("api_base_url");
    let existing_endpoint_revision: i64 = existing.get("endpoint_revision");

    let new_api_key = change
        .input
        .api_key
        .as_ref()
        .map(|api_key| api_key.trim())
        .filter(|api_key| !api_key.is_empty());
    let next_api_key = new_api_key
        .map(ToString::to_string)
        .unwrap_or(existing_api_key);
    let endpoints =
        normalize_station_endpoints(&change.input.website_url, &change.input.api_base_url)
            .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
    let website_url_changed = endpoints.website_url != existing_website_url;
    let api_base_url_changed = endpoints.api_base_url != existing_api_base_url;
    let endpoints_changed = website_url_changed || api_base_url_changed;
    let website_origin_changed = endpoints_changed
        && !same_origin(&existing_website_url, &endpoints.website_url)
            .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
    let api_origin_changed = endpoints_changed
        && !same_origin(&existing_api_base_url, &endpoints.api_base_url)
            .map_err(|_| PersistenceError::Sqlx(sqlx::Error::RowNotFound))?;
    let endpoint_revision = if endpoints_changed {
        existing_endpoint_revision.max(1) + 1
    } else {
        existing_endpoint_revision.max(1)
    };
    let next_enabled = change.input.enabled && !api_origin_changed;
    let collector_proxy_mode = normalize_proxy_mode(&change.input.collector_proxy_mode, true);
    let collector_proxy_url = normalize_proxy_url(change.input.collector_proxy_url);

    sqlx::query(
        r#"
        UPDATE stations
        SET name = ?1,
            station_type = ?2,
            website_url = ?3,
            api_base_url = ?4,
            endpoint_revision = ?5,
            api_key = ?6,
            api_key_secret_id = ?7,
            collector_proxy_mode = ?8,
            collector_proxy_url = ?9,
            enabled = ?10,
            credit_per_cny = ?11,
            low_balance_threshold_cny = ?12,
            collection_interval_minutes = ?13,
            status = CASE WHEN ?10 = 0 THEN 'disabled'
                          WHEN ?17 = 1 THEN 'unchecked'
                          WHEN status = 'disabled' THEN 'unchecked'
                          ELSE status END,
            note = ?14,
            last_checked_at = CASE WHEN ?17 = 1 THEN NULL ELSE last_checked_at END,
            last_pricing_fetched_at = CASE WHEN ?17 = 1 THEN NULL ELSE last_pricing_fetched_at END,
            updated_at = ?15
        WHERE id = ?16
        "#,
    )
    .bind(change.input.name.trim())
    .bind(&change.input.station_type)
    .bind(&endpoints.website_url)
    .bind(&endpoints.api_base_url)
    .bind(endpoint_revision)
    .bind(&next_api_key)
    .bind(&existing_secret_id)
    .bind(&collector_proxy_mode)
    .bind(&collector_proxy_url)
    .bind(bool_to_i64(next_enabled))
    .bind(change.input.credit_per_cny)
    .bind(change.input.low_balance_threshold_cny)
    .bind(i64::from(change.input.collection_interval_minutes))
    .bind(normalize_optional_string(change.input.note))
    .bind(&change.now)
    .bind(&change.input.id)
    .bind(bool_to_i64(endpoints_changed))
    .execute(&mut *connection)
    .await?;

    if website_origin_changed {
        clear_station_origin_bound_login_material(&mut *connection, &change.input.id, &change.now)
            .await?;
    }
    if api_base_url_changed {
        clear_station_endpoint_health_state(&mut *connection, &change.input.id).await?;
    }

    station_by_id(&mut *connection, &change.input.id).await
}

async fn clear_station_endpoint_health_state(
    connection: &mut SqliteConnection,
    station_id: &str,
) -> Result<(), PersistenceError> {
    sqlx::query("DELETE FROM station_endpoint_health WHERE station_id = ?1")
        .bind(station_id)
        .execute(&mut *connection)
        .await?;
    sqlx::query(
        r#"
        DELETE FROM station_key_health
        WHERE station_key_id IN (SELECT id FROM station_keys WHERE station_id = ?1)
        "#,
    )
    .bind(station_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn clear_station_origin_bound_login_material(
    connection: &mut SqliteConnection,
    station_id: &str,
    now: &str,
) -> Result<(), PersistenceError> {
    let secret_ids = sqlx::query(
        r#"
        SELECT login_password_secret_id, access_token_secret_id,
               refresh_token_secret_id, cookie_secret_id
        FROM station_credentials
        WHERE station_id = ?1
        "#,
    )
    .bind(station_id)
    .fetch_optional(&mut *connection)
    .await?
    .map(|row| {
        [
            row.get::<Option<String>, _>("login_password_secret_id"),
            row.get::<Option<String>, _>("access_token_secret_id"),
            row.get::<Option<String>, _>("refresh_token_secret_id"),
            row.get::<Option<String>, _>("cookie_secret_id"),
        ]
    })
    .unwrap_or([None, None, None, None]);

    sqlx::query(
        r#"
        UPDATE station_credentials
        SET login_password = NULL,
            login_password_secret_id = NULL,
            remember_password = 0,
            login_status = 'unknown',
            login_error = NULL,
            last_login_at = NULL,
            session_status = 'none',
            session_expires_at = NULL,
            access_token_secret_id = NULL,
            refresh_token_secret_id = NULL,
            cookie_secret_id = NULL,
            newapi_user_id = NULL,
            token_expires_at = NULL,
            token_refreshed_at = NULL,
            session_source = 'none',
            updated_at = ?1
        WHERE station_id = ?2
        "#,
    )
    .bind(now)
    .bind(station_id)
    .execute(&mut *connection)
    .await?;

    for secret_id in secret_ids.into_iter().flatten() {
        sqlx::query(
            r#"
            DELETE FROM secrets
            WHERE id = ?1
              AND NOT EXISTS (
                    SELECT 1 FROM stations WHERE api_key_secret_id = ?1
                    UNION ALL SELECT 1 FROM station_credentials WHERE login_password_secret_id = ?1
                    UNION ALL SELECT 1 FROM station_credentials WHERE access_token_secret_id = ?1
                    UNION ALL SELECT 1 FROM station_credentials WHERE refresh_token_secret_id = ?1
                    UNION ALL SELECT 1 FROM station_credentials WHERE cookie_secret_id = ?1
              )
            "#,
        )
        .bind(secret_id)
        .execute(&mut *connection)
        .await?;
    }
    Ok(())
}

async fn list_stations<'e, E>(executor: E) -> Result<Vec<Station>, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let rows = sqlx::query(
        r#"
        SELECT id, name, station_type, website_url, api_base_url, endpoint_revision,
               api_key,
               (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
               enabled, priority, credit_per_cny, balance_raw, balance_cny,
               low_balance_threshold_cny, collection_interval_minutes, status, latency_ms,
               last_checked_at, last_pricing_fetched_at, note, created_at, updated_at,
               (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id) AS api_key_masked,
               api_key_secret_id, collector_proxy_mode, collector_proxy_url
        FROM stations
        ORDER BY priority ASC, created_at ASC
        "#,
    )
    .fetch_all(executor)
    .await?;
    rows.into_iter().map(row_to_station).collect()
}

async fn station_by_id<'e, E>(executor: E, id: &str) -> Result<Station, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query(
        r#"
        SELECT id, name, station_type, website_url, api_base_url, endpoint_revision,
               api_key,
               (SELECT COUNT(*) FROM station_keys WHERE station_keys.station_id = stations.id) AS key_count,
               enabled, priority, credit_per_cny, balance_raw, balance_cny,
               low_balance_threshold_cny, collection_interval_minutes, status, latency_ms,
               last_checked_at, last_pricing_fetched_at, note, created_at, updated_at,
               (SELECT masked_value FROM secrets WHERE secrets.id = stations.api_key_secret_id) AS api_key_masked,
               api_key_secret_id, collector_proxy_mode, collector_proxy_url
        FROM stations
        WHERE id = ?1
        "#,
    )
    .bind(id)
    .fetch_optional(executor)
    .await?;
    row.map(row_to_station)
        .transpose()?
        .ok_or(PersistenceError::Sqlx(sqlx::Error::RowNotFound))
}

fn row_to_station(row: sqlx::sqlite::SqliteRow) -> Result<Station, PersistenceError> {
    let api_key: String = row.get("api_key");
    let secret_masked: Option<String> = row.get("api_key_masked");
    let api_key_secret_id: Option<String> = row.get("api_key_secret_id");
    let api_key_masked = secret_masked.unwrap_or_else(|| mask_secret(&api_key));
    let api_key_present = api_key_secret_id.is_some() || !api_key.trim().is_empty();
    let collection_interval_minutes: i64 = row.get("collection_interval_minutes");

    Ok(Station {
        id: row.get("id"),
        name: row.get("name"),
        station_type: row.get("station_type"),
        website_url: row.get("website_url"),
        api_base_url: row.get("api_base_url"),
        endpoint_revision: row.get("endpoint_revision"),
        collector_proxy_mode: row.get("collector_proxy_mode"),
        collector_proxy_url: row.get("collector_proxy_url"),
        api_key_masked,
        api_key_present,
        key_count: row.get("key_count"),
        enabled: i64_to_bool(row.get("enabled")),
        priority: row.get("priority"),
        credit_per_cny: row.get("credit_per_cny"),
        balance_raw: row.get("balance_raw"),
        balance_cny: row.get("balance_cny"),
        low_balance_threshold_cny: row.get("low_balance_threshold_cny"),
        collection_interval_minutes: collection_interval_minutes as u16,
        status: row.get("status"),
        latency_ms: row.get("latency_ms"),
        last_checked_at: row.get("last_checked_at"),
        last_pricing_fetched_at: row.get("last_pricing_fetched_at"),
        note: row.get("note"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    })
}

async fn next_station_priority<'e, E>(executor: E) -> Result<i64, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query("SELECT COALESCE(MAX(priority), -1) + 1 AS next_priority FROM stations")
        .fetch_one(executor)
        .await?;
    Ok(row.get("next_priority"))
}

async fn normalize_station_priorities(
    connection: &mut SqliteConnection,
) -> Result<(), PersistenceError> {
    let rows = sqlx::query("SELECT id FROM stations ORDER BY priority ASC, created_at ASC")
        .fetch_all(&mut *connection)
        .await?;
    for (index, row) in rows.into_iter().enumerate() {
        let id: String = row.get("id");
        sqlx::query("UPDATE stations SET priority = ?1 WHERE id = ?2")
            .bind(index as i64)
            .bind(id)
            .execute(&mut *connection)
            .await?;
    }
    Ok(())
}

fn validate_station_fields(
    name: &str,
    station_type: &str,
    website_url: &str,
    credit_per_cny: f64,
    collection_interval_minutes: u16,
) -> Result<(), PersistenceError> {
    if name.trim().is_empty()
        || website_url.trim().is_empty()
        || credit_per_cny <= 0.0
        || collection_interval_minutes == 0
        || !matches!(
            station_type,
            "sub2api" | "newapi" | "openai-compatible" | "custom"
        )
    {
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
