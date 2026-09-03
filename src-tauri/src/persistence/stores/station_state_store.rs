//! Persistence-owned reads for station current-state projections.
//!
//! The application read model deliberately does not know how collection and
//! authorization projections are stored or joined.  This store keeps both
//! queries on the caller's read transaction and returns typed scalar rows;
//! JSON payload validation remains at the application boundary so malformed
//! data fails closed instead of becoming a fabricated healthy state.

use sqlx::Row;

use crate::persistence::{error::PersistenceError, read_session::ReadSession};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StationCollectionProjectionRow {
    pub(crate) station_id: String,
    pub(crate) status: String,
    pub(crate) reason_codes_json: String,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StationAuthorizationProjectionRow {
    pub(crate) station_id: String,
    pub(crate) status: String,
    pub(crate) credential_revision: i64,
    pub(crate) reason_code: Option<String>,
    pub(crate) revision: i64,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct StationStateStore;

impl StationStateStore {
    /// Load collection projections for a bounded station-id JSON array.
    ///
    /// The query remains parameterized and executes on the caller's existing
    /// read transaction, preserving a consistent snapshot for the complete
    /// station asset response.
    pub(crate) async fn list_collection_projections(
        &self,
        read: &mut ReadSession,
        station_ids_json: &str,
    ) -> Result<Vec<StationCollectionProjectionRow>, PersistenceError> {
        validate_station_ids_json(station_ids_json)?;
        let rows = sqlx::query(
            "WITH requested(station_id) AS (SELECT value FROM json_each(?1))
             SELECT projection.station_id, projection.status,
                    projection.reason_codes_json, projection.revision
             FROM station_collection_projection projection
             JOIN requested ON requested.station_id = projection.station_id",
        )
        .bind(station_ids_json)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter().map(read_collection_projection).collect()
    }

    /// Load authorization projections and their durable credential revision
    /// for a bounded station-id JSON array.
    pub(crate) async fn list_authorization_projections(
        &self,
        read: &mut ReadSession,
        station_ids_json: &str,
    ) -> Result<Vec<StationAuthorizationProjectionRow>, PersistenceError> {
        validate_station_ids_json(station_ids_json)?;
        let rows = sqlx::query(
            "WITH requested(station_id) AS (SELECT value FROM json_each(?1))
             SELECT projection.station_id, projection.status,
                    projection.credential_revision, projection.reason_code,
                    COALESCE(revision.revision, projection.credential_revision) AS revision
             FROM station_authorization_projection projection
             JOIN requested ON requested.station_id = projection.station_id
             LEFT JOIN domain_revisions revision
               ON revision.scope = 'station_account:' || projection.station_id",
        )
        .bind(station_ids_json)
        .fetch_all(read.connection())
        .await?;
        rows.into_iter()
            .map(read_authorization_projection)
            .collect()
    }
}

fn read_collection_projection(
    row: sqlx::sqlite::SqliteRow,
) -> Result<StationCollectionProjectionRow, PersistenceError> {
    let station_id: String = row.try_get("station_id")?;
    let status: String = row.try_get("status")?;
    let reason_codes_json: String = row.try_get("reason_codes_json")?;
    let revision: i64 = row.try_get("revision")?;
    if station_id.trim().is_empty() || status.trim().is_empty() || revision < 1 {
        return Err(PersistenceError::InvariantViolation(
            "stored station collection projection is invalid".to_string(),
        ));
    }
    Ok(StationCollectionProjectionRow {
        station_id,
        status,
        reason_codes_json,
        revision,
    })
}

fn read_authorization_projection(
    row: sqlx::sqlite::SqliteRow,
) -> Result<StationAuthorizationProjectionRow, PersistenceError> {
    let station_id: String = row.try_get("station_id")?;
    let status: String = row.try_get("status")?;
    let credential_revision: i64 = row.try_get("credential_revision")?;
    let reason_code: Option<String> = row.try_get("reason_code")?;
    let revision: i64 = row.try_get("revision")?;
    if station_id.trim().is_empty()
        || status.trim().is_empty()
        || credential_revision < 0
        || revision < 0
    {
        return Err(PersistenceError::InvariantViolation(
            "stored station authorization projection is invalid".to_string(),
        ));
    }
    Ok(StationAuthorizationProjectionRow {
        station_id,
        status,
        credential_revision,
        reason_code,
        revision,
    })
}

fn validate_station_ids_json(station_ids_json: &str) -> Result<(), PersistenceError> {
    if station_ids_json.trim().is_empty() {
        return Err(PersistenceError::ConstraintViolation);
    }
    let station_ids = serde_json::from_str::<Vec<String>>(station_ids_json)
        .map_err(|_| PersistenceError::ConstraintViolation)?;
    if station_ids
        .iter()
        .any(|station_id| station_id.trim().is_empty())
    {
        return Err(PersistenceError::ConstraintViolation);
    }
    Ok(())
}
