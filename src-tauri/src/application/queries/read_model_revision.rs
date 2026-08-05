use crate::persistence::{error::PersistenceError, ReadSession};

/// Read-model responses carry the highest durable source revision observed by
/// the query. It is computed from the same read transaction as the rows.
pub(crate) async fn load_asset_revision(read: &mut ReadSession) -> Result<i64, PersistenceError> {
    let revision = sqlx::query_scalar::<_, i64>(
        "SELECT MAX(revision) FROM domain_revisions WHERE scope LIKE 'station:%' OR scope LIKE 'station_key:%' OR scope LIKE 'setting:%' OR scope LIKE 'model_alias:%'",
    )
    .fetch_one(read.connection())
    .await?;
    if revision <= 0 {
        return Err(PersistenceError::RevisionUnavailable(
            "asset_read_model".to_string(),
        ));
    }
    Ok(revision)
}
