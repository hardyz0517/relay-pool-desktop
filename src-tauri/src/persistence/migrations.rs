use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

use semver::Version;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Executor, Sqlite,
};

use crate::persistence::{
    backup::{create_verified_backup_from_path, validate_read_only_sqlite},
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

pub(crate) async fn upgrade_existing_v2_database(
    path: &Path,
) -> Result<Option<PathBuf>, PersistenceError> {
    if !path.is_file() {
        return Err(PersistenceError::MissingDatabase);
    }

    let pool = migration_pool_existing(path).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    let binary = current_binary_compatibility();
    let open_mode = decide_open_mode(&binary, &compatibility, schema_version)?;
    if open_mode == crate::persistence::schema_compatibility::OpenMode::Writable {
        pool.close().await;
        return Ok(None);
    }
    if schema_version >= current_schema_version()
        || binary.app_version < compatibility.min_writer_app_version
    {
        pool.close().await;
        return Err(PersistenceError::InvariantViolation(
            "generation 2 schema is not eligible for an in-place upgrade".to_string(),
        ));
    }
    pool.close().await;

    let backup_path = schema_upgrade_backup_path(path, schema_version)?;
    create_verified_backup_from_path(path, &backup_path).await?;

    let pool = migration_pool_existing(path).await?;
    if let Err(error) = migrator().run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    let mode = decide_open_mode(&binary, &compatibility, schema_version)?;
    pool.close().await;
    if mode != crate::persistence::schema_compatibility::OpenMode::Writable {
        return Err(PersistenceError::InvariantViolation(
            "generation 2 schema upgrade did not produce a writable database".to_string(),
        ));
    }
    validate_read_only_sqlite(path).await?;
    Ok(Some(backup_path))
}

pub(crate) fn current_binary_compatibility() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 3),
        database_generation: 2,
        readable_schema: 1..=10,
        writable_schema: BTreeSet::from([10]),
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

async fn migration_pool_existing(
    database_path: &Path,
) -> Result<sqlx::SqlitePool, PersistenceError> {
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

fn schema_upgrade_backup_path(
    database_path: &Path,
    source_schema: i64,
) -> Result<PathBuf, PersistenceError> {
    let parent = database_path.parent().ok_or(PersistenceError::IoFailed {
        kind: std::io::ErrorKind::InvalidInput,
    })?;
    Ok(parent.join("backups").join(format!(
        "relay-pool-v2-schema-{source_schema}-to-{}-{}.sqlite3",
        current_schema_version(),
        uuid::Uuid::now_v7()
    )))
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use sqlx::{migrate::Migrator, Row};

    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[tokio::test]
    async fn existing_schema_is_backed_up_and_migrated_to_current() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 9).await;
        write_migration_canary(&path).await;

        let backup = upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema")
            .expect("backup path");

        assert!(backup.is_file());
        assert_eq!(database_schema_version(&backup).await, 9);
        assert_eq!(
            database_schema_version(&path).await,
            current_schema_version()
        );
        assert_eq!(read_migration_canary(&backup).await, "preserved");
        assert_eq!(read_migration_canary(&path).await, "preserved");
        let runtime = PersistenceRuntime::open_current(&path)
            .await
            .expect("open migrated runtime");
        assert_eq!(
            runtime.health().await.expect("health").open_mode,
            "writable"
        );
        runtime.close().await.expect("close runtime");
    }

    #[tokio::test]
    async fn current_schema_upgrade_is_idempotent_and_creates_no_backup() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_v2_database(&path)
            .await
            .expect("initialize database");

        assert_eq!(
            upgrade_existing_v2_database(&path)
                .await
                .expect("current schema"),
            None
        );
        assert!(!root.path().join("backups").exists());
    }

    async fn initialize_database_through(path: &Path, target_version: i64) {
        let pool = migration_pool_create(path).await.expect("migration pool");
        let partial = Migrator {
            migrations: Cow::Owned(
                migrator()
                    .iter()
                    .filter(|migration| migration.version <= target_version)
                    .cloned()
                    .collect(),
            ),
            ignore_missing: false,
            locking: true,
            no_tx: false,
        };
        partial.run(&pool).await.expect("partial migrations");
        pool.close().await;
    }

    async fn database_schema_version(path: &Path) -> i64 {
        let pool = migration_pool_existing(path).await.expect("open database");
        let row = sqlx::query(
            "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("schema version");
        let version = row.get("schema_version");
        pool.close().await;
        version
    }

    async fn write_migration_canary(path: &Path) {
        let pool = migration_pool_existing(path).await.expect("open database");
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('schema-upgrade-canary', 'preserved', '1')",
        )
        .execute(&pool)
        .await
        .expect("write canary");
        pool.close().await;
    }

    async fn read_migration_canary(path: &Path) -> String {
        let pool = migration_pool_existing(path).await.expect("open database");
        let row = sqlx::query("SELECT value FROM settings WHERE key = 'schema-upgrade-canary'")
            .fetch_one(&pool)
            .await
            .expect("read canary");
        let value = row.get("value");
        pool.close().await;
        value
    }
}
