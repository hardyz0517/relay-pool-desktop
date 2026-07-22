use std::{collections::BTreeSet, path::Path, time::Duration};

use semver::Version;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Executor, Sqlite,
};

use crate::persistence::{
    error::PersistenceError,
    schema_compatibility::{decide_open_mode, load_schema_compatibility, BinaryCompatibility},
};

pub(crate) fn migrator() -> &'static sqlx::migrate::Migrator {
    static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/persistence/migrations");
    &MIGRATOR
}

pub(crate) async fn applied_schema_version<'e, E>(executor: E) -> Result<i64, PersistenceError>
where
    E: Executor<'e, Database = Sqlite>,
{
    let row = sqlx::query!(
        r#"
        SELECT version AS "version!: i64"
        FROM _sqlx_migrations
        WHERE success = 1
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .fetch_optional(executor)
    .await?;
    row.map(|row| row.version)
        .ok_or(PersistenceError::MissingMigrationMetadata)
}

pub(crate) async fn initialize_v2_database(path: &Path) -> Result<(), PersistenceError> {
    if path.exists() {
        return Err(PersistenceError::InvariantViolation(
            "generation 2 database already exists".to_string(),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pool = migration_pool_create(path).await?;
    migrator().run(&pool).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    decide_open_mode(
        &current_binary_compatibility(),
        &compatibility,
        schema_version,
    )?;
    pool.close().await;
    Ok(())
}

pub(crate) fn current_binary_compatibility() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=8,
        writable_schema: BTreeSet::from([8]),
    }
}

pub(crate) fn current_schema_version() -> i64 {
    migrator()
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or_default()
}

async fn migration_pool_create(database_path: &Path) -> Result<sqlx::SqlitePool, PersistenceError> {
    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true)
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
