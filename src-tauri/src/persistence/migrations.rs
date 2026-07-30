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
    sqlx::query!(
        r#"
        UPDATE persistence_schema_compatibility
        SET schema_version = 16,
            min_reader_app_version = '0.3.3',
            min_writer_app_version = '0.3.3',
            updated_by_migration = 16,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE singleton_key = 1
        "#,
    )
    .execute(&pool)
    .await?;
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
        readable_schema: 1..=16,
        writable_schema: BTreeSet::from([16]),
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

    use sqlx::{migrate::Migrator, query, Row};

    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[tokio::test]
    async fn current_schema_seeds_builtin_monitor_templates_idempotently() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_v2_database(&path)
            .await
            .expect("initialize database");

        let pool = migration_pool_existing(&path).await.expect("open database");
        migrator().run(&pool).await.expect("rerun migrations");
        let templates: Vec<(String, i64, i64)> = sqlx::query_as(
            r#"
            SELECT id, enabled, built_in
            FROM channel_monitor_request_templates
            WHERE id IN (
                'builtin-openai-chat-low-token',
                'builtin-openai-responses-low-token'
            )
            ORDER BY id
            "#,
        )
        .fetch_all(&pool)
        .await
        .expect("list builtin templates");
        pool.close().await;

        assert_eq!(templates.len(), 2);
        assert!(templates
            .iter()
            .all(|(_, enabled, built_in)| *enabled == 1 && *built_in == 1));
    }

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

    #[tokio::test]
    async fn remote_key_upgrade_repairs_duplicate_local_owners_before_enforcing_uniqueness() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 10).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
             ) VALUES ('station-1', 'Station', 'sub2api', 'https://example.test',
                       'https://example.test/v1', '1', '1')",
        )
        .execute(&pool)
        .await
        .expect("insert station");
        query(
            "INSERT INTO station_keys (id, station_id, note)
             VALUES ('local-1', 'station-1', '由远端发现开关自动创建：remote-owned')",
        )
        .execute(&pool)
        .await
        .expect("insert station key");
        for (id, confidence, collected_at) in
            [("remote-other", 1.0, "3"), ("remote-owned", 0.9, "2")]
        {
            query(
                "INSERT INTO remote_station_keys (
                    id, station_id, remote_key_id_hash, raw_source, match_status,
                    matched_station_key_id, match_confidence, collected_at, updated_at
                 ) VALUES (?1, 'station-1', ?1, 'fixture', 'matched',
                           'local-1', ?2, ?3, ?3)",
            )
            .bind(id)
            .bind(confidence)
            .bind(collected_at)
            .execute(&pool)
            .await
            .expect("insert duplicate remote match");
        }
        pool.close().await;

        upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema");

        let pool = migration_pool_existing(&path).await.expect("upgraded pool");
        let owner: String = sqlx::query_scalar(
            "SELECT id FROM remote_station_keys WHERE matched_station_key_id = 'local-1'",
        )
        .fetch_one(&pool)
        .await
        .expect("one local owner");
        assert_eq!(owner, "remote-owned");
        let repaired: (String, Option<String>, f64) = sqlx::query_as(
            "SELECT match_status, matched_station_key_id, match_confidence
             FROM remote_station_keys WHERE id = 'remote-other'",
        )
        .fetch_one(&pool)
        .await
        .expect("repaired duplicate");
        assert_eq!(repaired, ("unbound".to_string(), None, 0.0));
        let duplicate = query(
            "UPDATE remote_station_keys
             SET matched_station_key_id = 'local-1', match_status = 'matched'
             WHERE id = 'remote-other'",
        )
        .execute(&pool)
        .await;
        assert!(
            duplicate.is_err(),
            "schema must reject a second local owner"
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn profile_v2_upgrade_only_updates_outdated_builtin_cli_definitions() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 13).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
             ) VALUES (
                'profile-station', 'Profile Station', 'openai_compatible',
                'https://example.test', 'https://example.test/v1', '1', '1'
             )",
        )
        .execute(&pool)
        .await
        .expect("insert station");

        for (id, profile_id, profile_version) in [
            ("standard-v1", "standard_api", 1_i64),
            ("codex-v1", "codex_cli_compat", 1),
            ("claude-v1", "claude_code_compat", 1),
            ("gemini-v1", "gemini_cli_compat", 1),
            ("grok-v1", "grok_cli_compat", 1),
            ("codex-future", "codex_cli_compat", 3),
        ] {
            query(
                "INSERT INTO channel_monitors (
                    id, name, target_type, station_id, template_id,
                    interval_seconds, timeout_seconds, created_at, updated_at,
                    client_profile_id, client_profile_version
                 ) VALUES (
                    ?1, ?1, 'station', 'profile-station',
                    'builtin-openai-chat-low-token', 60, 30, '1', '1', ?2, ?3
                 )",
            )
            .bind(id)
            .bind(profile_id)
            .bind(profile_version)
            .execute(&pool)
            .await
            .expect("insert monitor definition");
        }
        pool.close().await;

        upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema");

        let pool = migration_pool_existing(&path).await.expect("upgraded pool");
        let versions: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id, client_profile_version, schedule_revision
             FROM channel_monitors
             ORDER BY id",
        )
        .fetch_all(&pool)
        .await
        .expect("read migrated profiles");
        pool.close().await;

        assert_eq!(
            versions,
            vec![
                ("claude-v1".to_string(), 2, 2),
                ("codex-future".to_string(), 3, 1),
                ("codex-v1".to_string(), 2, 2),
                ("gemini-v1".to_string(), 2, 2),
                ("grok-v1".to_string(), 1, 1),
                ("standard-v1".to_string(), 1, 1),
            ]
        );
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
        let row = query(
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
        query(
            "INSERT INTO settings (key, value, updated_at) VALUES ('schema-upgrade-canary', 'preserved', '1')",
        )
        .execute(&pool)
        .await
        .expect("write canary");
        pool.close().await;
    }

    async fn read_migration_canary(path: &Path) -> String {
        let pool = migration_pool_existing(path).await.expect("open database");
        let row = query("SELECT value FROM settings WHERE key = 'schema-upgrade-canary'")
            .fetch_one(&pool)
            .await
            .expect("read canary");
        let value = row.get("value");
        pool.close().await;
        value
    }
}
