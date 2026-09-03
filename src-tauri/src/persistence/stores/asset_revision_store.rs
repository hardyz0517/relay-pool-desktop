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
        // This is an independently owned workspace revision. Migration-owned
        // table triggers advance it in the same transaction as every row that
        // can change the Station Asset read model. Do not synthesize a
        // watermark from unrelated aggregate revisions: MAX can miss writes,
        // and SUM is not a stable revision owner.
        let revision = sqlx::query_scalar::<_, i64>(
            "SELECT revision FROM domain_revisions WHERE scope = 'read_model:station_assets'",
        )
        .fetch_optional(&mut *connection)
        .await?
        .ok_or_else(|| {
            PersistenceError::RevisionUnavailable("read_model:station_assets".to_string())
        })?;
        if revision <= 0 {
            return Err(PersistenceError::RevisionUnavailable(
                "read_model:station_assets".to_string(),
            ));
        }
        Ok(revision)
    }

    pub(crate) async fn load_station_detail(
        &self,
        connection: &mut SqliteConnection,
        station_id: &str,
    ) -> Result<i64, PersistenceError> {
        let scope = format!("read_model:station_detail:{station_id}");
        let revision =
            sqlx::query_scalar::<_, i64>("SELECT revision FROM domain_revisions WHERE scope = ?1")
                .bind(&scope)
                .fetch_optional(&mut *connection)
                .await?
                .ok_or_else(|| PersistenceError::RevisionUnavailable(scope.clone()))?;
        if revision <= 0 {
            return Err(PersistenceError::RevisionUnavailable(scope));
        }
        Ok(revision)
    }
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Executor, SqliteConnection};

    use super::AssetRevisionStore;
    use crate::persistence::migrations::migrator;

    #[tokio::test]
    async fn station_asset_revision_is_independently_owned_and_transactional() {
        let mut connection = SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open memory database");
        migrator()
            .run(&mut connection)
            .await
            .expect("migrate schema");

        let store = AssetRevisionStore;
        assert_eq!(store.load(&mut connection).await.expect("baseline"), 1);

        connection
            .execute(
                "INSERT INTO stations (
                    id, name, station_type, website_url, api_base_url,
                    created_at, updated_at
                 ) VALUES (
                    'asset-revision-station', 'Station', 'openai',
                    'https://example.invalid', 'https://example.invalid/v1',
                    '1', '1'
                 )",
            )
            .await
            .expect("insert station");
        assert_eq!(
            store.load(&mut connection).await.expect("station revision"),
            2
        );

        connection
            .execute(
                "INSERT INTO station_collection_projection (
                    station_id, status, reason_codes_json, revision,
                    endpoint_revision, credential_revision, intent_sequence,
                    operation_id, updated_at_ms
                 ) VALUES (
                    'asset-revision-station', 'healthy', '[]', 1,
                    1, 1, 1, 'asset-revision-operation', 1
                 )",
            )
            .await
            .expect("insert collection projection");
        assert_eq!(
            store
                .load(&mut connection)
                .await
                .expect("projection revision"),
            3
        );
    }
}
