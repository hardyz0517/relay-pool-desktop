use sqlx::SqliteConnection;

use crate::persistence::error::PersistenceError;

/// Reads the durable revision used to version asset read models.
///
/// Keeping this query in persistence prevents application query modules from
/// reaching into SQLite directly while preserving the caller's transaction.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AssetRevisionStore;

impl AssetRevisionStore {
    pub(crate) async fn load(
        &self,
        connection: &mut SqliteConnection,
    ) -> Result<i64, PersistenceError> {
        let revision = sqlx::query_scalar::<_, Option<i64>>(
            "SELECT MAX(revision) FROM domain_revisions WHERE scope LIKE 'station:%' OR scope LIKE 'station_key:%' OR scope LIKE 'setting:%' OR scope LIKE 'model_alias:%'",
        )
        .fetch_one(&mut *connection)
        .await?
        .unwrap_or_default();
        if revision <= 0 {
            return Err(PersistenceError::RevisionUnavailable(
                "asset_read_model".to_string(),
            ));
        }
        Ok(revision)
    }
}
