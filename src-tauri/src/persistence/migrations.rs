use std::{collections::BTreeSet, path::Path, time::Duration};

use semver::Version;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Executor, Row, Sqlite,
};

use crate::persistence::{
    backup::{create_verified_backup, validate_read_only_sqlite},
    error::PersistenceError,
    schema_compatibility::{
        decide_open_mode, load_schema_compatibility, BinaryCompatibility, SchemaCompatibility,
    },
};

pub(crate) fn migrator() -> &'static sqlx::migrate::Migrator {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/persistence/migrations");
    &MIGRATOR
}

pub(crate) async fn applied_schema_version<'e, E>(executor: E) -> Result<i64, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query(
        r#"
        SELECT version
        FROM _sqlx_migrations
        WHERE success = 1
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(executor)
    .await?;
    row.map(|row| row.get("version"))
        .ok_or(PersistenceError::MissingMigrationMetadata)
}

pub(crate) async fn run_pending_v2_migrations_after_verified_backup(
    database_path: &Path,
    backup_path: &Path,
    binary: BinaryCompatibility,
) -> Result<SchemaCompatibility, PersistenceError> {
    if !database_path.is_file() {
        return Err(PersistenceError::MissingDatabase);
    }
    let pool = migration_pool(database_path).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    decide_open_mode(&binary, &compatibility, schema_version)?;

    create_verified_backup(&pool, backup_path).await?;
    migrator().run(&pool).await?;

    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    decide_open_mode(&latest_binary(), &compatibility, schema_version)?;
    pool.close().await;
    validate_read_only_sqlite(database_path).await?;
    Ok(compatibility)
}

fn latest_binary() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=2,
        writable_schema: BTreeSet::from([2]),
    }
}

async fn migration_pool(database_path: &Path) -> Result<sqlx::SqlitePool, PersistenceError> {
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?)
}
