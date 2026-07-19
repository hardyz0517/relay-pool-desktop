use sqlx::{Executor, SqlitePool};

use crate::persistence::{
    error::PersistenceError,
    schema_compatibility::{OpenMode, SchemaCompatibility},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeHealth {
    pub(crate) open_mode: String,
    pub(crate) database_generation: i64,
    pub(crate) schema_version: i64,
}

pub(crate) async fn record_runtime_open(
    pool: &SqlitePool,
    open_mode: OpenMode,
) -> Result<(), PersistenceError> {
    if open_mode != OpenMode::Writable {
        return Ok(());
    }
    pool.execute(
        r#"
        UPDATE persistence_runtime_health
        SET write_probe_count = write_probe_count + 1,
            last_open_mode = 'writable',
            last_checked_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE singleton_key = 1
        "#,
    )
    .await?;
    Ok(())
}

pub(crate) async fn runtime_health(
    _pool: &SqlitePool,
    open_mode: OpenMode,
    compatibility: &SchemaCompatibility,
) -> Result<RuntimeHealth, PersistenceError> {
    Ok(RuntimeHealth {
        open_mode: open_mode.as_code().to_owned(),
        database_generation: compatibility.database_generation,
        schema_version: compatibility.schema_version,
    })
}
