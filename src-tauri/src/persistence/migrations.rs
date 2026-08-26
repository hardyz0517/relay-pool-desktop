use std::{
    borrow::Cow,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Component, Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Executor, Row, Sqlite,
};

use crate::persistence::{
    backup::{create_verified_backup_from_path, validate_read_only_sqlite},
    error::PersistenceError,
    maintenance::request_log_url_sanitizer::{
        sanitize_request_log_upstream_urls, sanitize_request_log_upstream_urls_before_schema18,
        RequestLogUrlSanitizerOptions,
    },
    schema_compatibility::{decide_open_mode, load_schema_compatibility, BinaryCompatibility},
    schema_registry,
};

const SCHEMA_UPGRADE_BACKUP_MANIFEST_VERSION: u32 = 1;
const SCHEMA_UPGRADE_BACKUP_MANIFEST_SUFFIX: &str = ".backup-manifest.json";

// Database files created by the 2026-08-25 build contain this checksum for
// migration 55. The source file was subsequently amended before it was
// committed. Keep this compatibility entry until those databases have passed
// the repair; unknown checksum drift remains a hard failure.
const LEGACY_MIGRATION_55_CHECKSUM: [u8; 48] = [
    0x3D, 0xDD, 0x99, 0xB4, 0xA9, 0xCF, 0xDC, 0xBE, 0xA0, 0x65, 0xFC, 0xA8, 0xBA, 0x08, 0x83, 0xF6,
    0x4E, 0x07, 0x4D, 0xAC, 0xD8, 0x88, 0xC2, 0xBD, 0x55, 0xF6, 0x3A, 0x8D, 0x2B, 0x56, 0xB4, 0x62,
    0x47, 0x43, 0x72, 0xAC, 0xB3, 0xB4, 0xDA, 0xB2, 0x42, 0x44, 0xD6, 0x53, 0x91, 0x78, 0xAC, 0xF8,
];

// Database files created by the 2026-08-26 build contain this checksum for
// migration 57. The source migration was subsequently amended before the
// next release. Reconcile this exact checksum only after verifying the
// durable postconditions of the destructive pricing cleanup.
const LEGACY_MIGRATION_57_CHECKSUM: [u8; 48] = [
    0x39, 0xE2, 0x2D, 0x81, 0x60, 0x00, 0x89, 0x3C, 0xFC, 0xD8, 0x15, 0x25, 0x1C, 0x5A, 0x1F, 0x07,
    0x76, 0xB9, 0x3C, 0x10, 0xAE, 0xAE, 0x3A, 0xB6, 0x3D, 0x6D, 0x42, 0xE6, 0xB2, 0x0C, 0xF2, 0x4D,
    0x85, 0x52, 0x1E, 0xA2, 0x43, 0xF0, 0x0B, 0x76, 0xCC, 0xA4, 0x76, 0x95, 0x38, 0x69, 0x50, 0x83,
];

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchemaUpgradeBackupManifest {
    manifest_version: u32,
    source_schema: i64,
    target_schema: i64,
    source_identity: SchemaUpgradeSourceIdentity,
    backup_file_name: String,
    backup_identity: SchemaUpgradeFileIdentity,
}

/// A metadata-only identity keeps retry checks constant-time even for multi-GB
/// databases. The installation lease prevents concurrent writes by Relay Pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchemaUpgradeFileIdentity {
    volume_serial: Option<u64>,
    file_id: Option<u64>,
    length: u64,
    modified_unix_seconds: Option<u64>,
    modified_nanoseconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SchemaUpgradeSourceIdentity {
    database: SchemaUpgradeFileIdentity,
    wal: Option<SchemaUpgradeFileIdentity>,
    journal: Option<SchemaUpgradeFileIdentity>,
}

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
    sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default()).await?;
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

#[cfg(test)]
pub(crate) async fn initialize_v2_database_at_schema_for_test(
    path: &Path,
    target_schema: i64,
) -> Result<(), PersistenceError> {
    if path.exists() {
        return Err(PersistenceError::InvariantViolation(
            "generation 2 database already exists".to_string(),
        ));
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let pool = migration_pool_create(path).await?;
    let bounded = migrator_through(target_schema)?;
    bounded.run(&pool).await?;
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
        if schema_version >= 18 {
            sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default())
                .await?;
        }
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
    if (5..18).contains(&schema_version) {
        sanitize_request_log_upstream_urls_before_schema18(
            &pool,
            RequestLogUrlSanitizerOptions::default(),
        )
        .await?;
    }
    pool.close().await;

    let backup_path =
        ensure_schema_upgrade_backup(path, schema_version, current_schema_version()).await?;

    let pool = migration_pool_existing(path).await?;
    if let Err(error) = reconcile_historical_migration_checksums(&pool).await {
        pool.close().await;
        return Err(error);
    }
    if let Err(error) = migrator().run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default()).await?;
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

pub(crate) async fn upgrade_existing_v2_database_to_schema(
    path: &Path,
    target_schema: i64,
) -> Result<Option<PathBuf>, PersistenceError> {
    schema_registry::validate_migration_registry()?;
    if !path.is_file() {
        return Err(PersistenceError::MissingDatabase);
    }
    if target_schema >= current_schema_version() {
        return upgrade_existing_v2_database(path).await;
    }

    let pool = migration_pool_existing(path).await?;
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    let binary = current_binary_compatibility();
    let open_mode = decide_open_mode(&binary, &compatibility, schema_version)?;
    if compatibility.schema_version >= target_schema && schema_version >= target_schema {
        pool.close().await;
        return Ok(None);
    }
    if compatibility.schema_version > target_schema
        || schema_version != compatibility.schema_version
        || open_mode == crate::persistence::schema_compatibility::OpenMode::Writable
    {
        pool.close().await;
        return Err(PersistenceError::InvariantViolation(
            "generation 2 schema is not eligible for a bounded upgrade".to_string(),
        ));
    }
    pool.close().await;

    let backup_path =
        ensure_schema_upgrade_backup(path, compatibility.schema_version, target_schema).await?;

    let pool = migration_pool_existing(path).await?;
    if let Err(error) = reconcile_historical_migration_checksums(&pool).await {
        pool.close().await;
        return Err(error);
    }
    let bounded = migrator_through(target_schema)?;
    if let Err(error) = bounded.run(&pool).await {
        pool.close().await;
        return Err(error.into());
    }
    let compatibility = load_schema_compatibility(&pool).await?;
    let schema_version = applied_schema_version(&pool).await?;
    pool.close().await;
    if compatibility.schema_version != target_schema || schema_version != target_schema {
        return Err(PersistenceError::InvariantViolation(
            "bounded schema upgrade did not reach the requested target".to_string(),
        ));
    }
    validate_read_only_sqlite(path).await?;
    Ok(Some(backup_path))
}

/// SQLx deliberately rejects modified applied migrations. One released build
/// shipped migration 55 before the file was amended in source control, so we
/// repair that exact, known checksum only after checking its durable effects.
/// Callers create a verified pre-upgrade backup before reaching this point,
/// because the schema 57 repair can discard obsolete price fields.
async fn reconcile_historical_migration_checksums(
    pool: &sqlx::SqlitePool,
) -> Result<(), PersistenceError> {
    let rows = sqlx::query(
        "SELECT version, checksum FROM _sqlx_migrations WHERE success = 1 ORDER BY version",
    )
    .fetch_all(pool)
    .await?;

    for row in rows {
        let version: i64 = row.try_get("version")?;
        let actual: Vec<u8> = row.try_get("checksum")?;
        let migration = migrator()
            .iter()
            .find(|migration| migration.version == version)
            .ok_or_else(|| PersistenceError::MigrationChecksumMismatch {
                version,
                expected: "migration missing from registry".to_string(),
                actual: hex_checksum(&actual),
            })?;
        let expected = migration.checksum.as_ref();
        if actual.as_slice() == expected {
            continue;
        }

        if version == 55 && actual.as_slice() == LEGACY_MIGRATION_55_CHECKSUM {
            verify_legacy_migration_55_postcondition(pool).await?;
            sqlx::query(
                "UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2 AND success = 1",
            )
            .bind(expected)
            .bind(version)
            .execute(pool)
            .await?;
            continue;
        }

        if version == 57 && actual.as_slice() == LEGACY_MIGRATION_57_CHECKSUM {
            complete_legacy_migration_57_cleanup(pool).await?;
            verify_legacy_migration_57_postcondition(pool).await?;
            sqlx::query(
                "UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2 AND success = 1",
            )
            .bind(expected)
            .bind(version)
            .execute(pool)
            .await?;
            continue;
        }

        return Err(PersistenceError::MigrationChecksumMismatch {
            version,
            expected: hex_checksum(expected),
            actual: hex_checksum(&actual),
        });
    }
    Ok(())
}

async fn verify_legacy_migration_55_postcondition(
    pool: &sqlx::SqlitePool,
) -> Result<(), PersistenceError> {
    let schema_version: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(pool)
    .await?;
    if schema_version != 55 {
        return Err(PersistenceError::InvariantViolation(
            "legacy migration 55 checksum matched but schema metadata is not 55".to_string(),
        ));
    }
    let historical_defaults: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM settings WHERE key = 'collector_timeout_seconds' AND trim(value) = '15'",
    )
    .fetch_one(pool)
    .await?;
    if historical_defaults != 0 {
        return Err(PersistenceError::InvariantViolation(
            "legacy migration 55 checksum matched but collector timeout postcondition is not satisfied"
                .to_string(),
        ));
    }
    Ok(())
}

/// The released schema-57 file removed the old pricing table and linkage, but
/// left three unused request-log price columns. The canonical migration later
/// removed those columns too. This repair is restricted to the exact released
/// checksum and runs only after a verified backup has been made.
async fn complete_legacy_migration_57_cleanup(
    pool: &sqlx::SqlitePool,
) -> Result<(), PersistenceError> {
    let schema_version: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(pool)
    .await?;
    if schema_version != 57 {
        return Err(PersistenceError::InvariantViolation(
            "legacy migration 57 checksum matched but schema metadata is not 57".to_string(),
        ));
    }

    let legacy_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'pricing_rules'",
    )
    .fetch_one(pool)
    .await?;
    let legacy_occurrence_column_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM pragma_table_info('change_event_occurrences') WHERE name = 'pricing_rule_id'",
    )
    .fetch_one(pool)
    .await?;
    let legacy_reference_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE sql IS NOT NULL AND lower(sql) LIKE '%pricing_rules%'",
    )
    .fetch_one(pool)
    .await?;
    let foreign_key_violation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(pool)
            .await?;
    if legacy_table_count != 0
        || legacy_occurrence_column_count != 0
        || legacy_reference_count != 0
        || foreign_key_violation_count != 0
    {
        return Err(PersistenceError::InvariantViolation(
            "legacy migration 57 checksum matched but pricing cleanup is unsafe to complete"
                .to_string(),
        ));
    }

    let mut transaction = pool.begin().await?;
    for column in ["base_input_cost", "base_output_cost", "base_total_cost"] {
        let exists: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('request_logs') WHERE name = ?1",
        )
        .bind(column)
        .fetch_one(&mut *transaction)
        .await?;
        if exists != 0 {
            let statement = match column {
                "base_input_cost" => "ALTER TABLE request_logs DROP COLUMN base_input_cost",
                "base_output_cost" => "ALTER TABLE request_logs DROP COLUMN base_output_cost",
                "base_total_cost" => "ALTER TABLE request_logs DROP COLUMN base_total_cost",
                _ => unreachable!("legacy schema 57 repair has a fixed column list"),
            };
            sqlx::query(statement).execute(&mut *transaction).await?;
        }
    }
    transaction.commit().await?;
    Ok(())
}

async fn verify_legacy_migration_57_postcondition(
    pool: &sqlx::SqlitePool,
) -> Result<(), PersistenceError> {
    let schema_version: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(pool)
    .await?;
    if schema_version != 57 {
        return Err(PersistenceError::InvariantViolation(
            "legacy migration 57 checksum matched but schema metadata is not 57".to_string(),
        ));
    }

    let request_log_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'request_logs'",
    )
    .fetch_one(pool)
    .await?;
    let legacy_table_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'pricing_rules'",
    )
    .fetch_one(pool)
    .await?;
    let legacy_column_count: i64 = sqlx::query_scalar(
        "SELECT
            (SELECT COUNT(*) FROM pragma_table_info('request_logs')
             WHERE name IN (
                 'base_fixed_cost', 'base_input_cost', 'base_output_cost',
                 'base_total_cost', 'pricing_rule_id'
             ))
            + (SELECT COUNT(*) FROM pragma_table_info('change_event_occurrences')
               WHERE name = 'pricing_rule_id')",
    )
    .fetch_one(pool)
    .await?;
    let legacy_reference_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master
         WHERE sql IS NOT NULL AND lower(sql) LIKE '%pricing_rules%'",
    )
    .fetch_one(pool)
    .await?;
    let foreign_key_violation_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM pragma_foreign_key_check")
            .fetch_one(pool)
            .await?;

    if request_log_table_count != 1
        || legacy_table_count != 0
        || legacy_column_count != 0
        || legacy_reference_count != 0
        || foreign_key_violation_count != 0
    {
        return Err(PersistenceError::InvariantViolation(
            "legacy migration 57 checksum matched but pricing cleanup postcondition is not satisfied"
                .to_string(),
        ));
    }
    Ok(())
}

fn hex_checksum(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02X}")).collect()
}

pub(crate) fn current_binary_compatibility() -> BinaryCompatibility {
    schema_registry::current_binary_compatibility()
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

async fn ensure_schema_upgrade_backup(
    database_path: &Path,
    source_schema: i64,
    target_schema: i64,
) -> Result<PathBuf, PersistenceError> {
    let source_identity = capture_source_identity(database_path)?;
    let backups_root = schema_upgrade_backups_root(database_path)?;
    if let Some(existing) = find_reusable_schema_upgrade_backup(
        &backups_root,
        source_schema,
        target_schema,
        &source_identity,
    ) {
        return Ok(existing);
    }

    let backup_path =
        schema_upgrade_backup_path_to_schema(database_path, source_schema, target_schema)?;
    create_verified_backup_from_path(database_path, &backup_path).await?;

    if capture_source_identity(database_path)? != source_identity {
        return Err(PersistenceError::InvariantViolation(
            "database source changed while creating the schema upgrade backup".to_string(),
        ));
    }

    let backup_identity = database_file_identity(&backup_path)?;
    let backup_file_name = backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "schema upgrade backup has no valid file name".to_string(),
            )
        })?
        .to_owned();
    write_schema_upgrade_backup_manifest(
        &backup_path,
        &SchemaUpgradeBackupManifest {
            manifest_version: SCHEMA_UPGRADE_BACKUP_MANIFEST_VERSION,
            source_schema,
            target_schema,
            source_identity,
            backup_file_name,
            backup_identity,
        },
    )?;
    Ok(backup_path)
}

fn schema_upgrade_backups_root(database_path: &Path) -> Result<PathBuf, PersistenceError> {
    database_path
        .parent()
        .map(|parent| parent.join("backups"))
        .ok_or(PersistenceError::IoFailed {
            kind: std::io::ErrorKind::InvalidInput,
        })
}

fn capture_source_identity(path: &Path) -> Result<SchemaUpgradeSourceIdentity, PersistenceError> {
    Ok(SchemaUpgradeSourceIdentity {
        database: database_file_identity(path)?,
        wal: optional_database_file_identity(&sqlite_sidecar_path(path, "-wal"))?,
        journal: optional_database_file_identity(&sqlite_sidecar_path(path, "-journal"))?,
    })
}

fn optional_database_file_identity(
    path: &Path,
) -> Result<Option<SchemaUpgradeFileIdentity>, PersistenceError> {
    if path.exists() {
        let identity = database_file_identity(path)?;
        // SQLite may materialize an empty WAL while a read-only connection is opened.
        // It contains no source state and must not make an otherwise stable source
        // appear modified between the pre- and post-backup snapshots.
        if identity.length == 0 {
            Ok(None)
        } else {
            Ok(Some(identity))
        }
    } else {
        Ok(None)
    }
}

fn sqlite_sidecar_path(database_path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{}", database_path.display(), suffix))
}

fn database_file_identity(path: &Path) -> Result<SchemaUpgradeFileIdentity, PersistenceError> {
    let file = File::open(path)?;
    let metadata = file.metadata()?;
    let (volume_serial, file_id) = platform_file_identity(&file)?;
    let (modified_unix_seconds, modified_nanoseconds) = metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
        .map(|duration| (Some(duration.as_secs()), Some(duration.subsec_nanos())))
        .unwrap_or((None, None));
    Ok(SchemaUpgradeFileIdentity {
        volume_serial,
        file_id,
        length: metadata.len(),
        modified_unix_seconds,
        modified_nanoseconds,
    })
}

#[cfg(windows)]
fn platform_file_identity(file: &File) -> Result<(Option<u64>, Option<u64>), PersistenceError> {
    use std::{mem, os::windows::io::AsRawHandle};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileInformationByHandle, BY_HANDLE_FILE_INFORMATION,
    };

    let mut info = unsafe { mem::zeroed::<BY_HANDLE_FILE_INFORMATION>() };
    let ok = unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) };
    if ok == 0 {
        return Err(PersistenceError::IoFailed {
            kind: io::Error::last_os_error().kind(),
        });
    }
    let file_id = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    Ok((Some(u64::from(info.dwVolumeSerialNumber)), Some(file_id)))
}

#[cfg(not(windows))]
fn platform_file_identity(_file: &File) -> Result<(Option<u64>, Option<u64>), PersistenceError> {
    Ok((None, None))
}

fn find_reusable_schema_upgrade_backup(
    backups_root: &Path,
    source_schema: i64,
    target_schema: i64,
    source_identity: &SchemaUpgradeSourceIdentity,
) -> Option<PathBuf> {
    let entries = fs::read_dir(backups_root).ok()?;
    entries.filter_map(Result::ok).find_map(|entry| {
        let manifest_path = entry.path();
        let file_name = manifest_path.file_name()?.to_str()?;
        if !manifest_path.is_file() || !file_name.ends_with(SCHEMA_UPGRADE_BACKUP_MANIFEST_SUFFIX) {
            return None;
        }
        let manifest = read_schema_upgrade_backup_manifest(&manifest_path)?;
        if manifest.manifest_version != SCHEMA_UPGRADE_BACKUP_MANIFEST_VERSION
            || manifest.source_schema != source_schema
            || manifest.target_schema != target_schema
            || manifest.source_identity != *source_identity
        {
            return None;
        }
        let backup_path = backup_path_from_manifest(backups_root, &manifest.backup_file_name)?;
        let actual_backup_identity = database_file_identity(&backup_path).ok()?;
        (actual_backup_identity == manifest.backup_identity).then_some(backup_path)
    })
}

fn backup_path_from_manifest(backups_root: &Path, backup_file_name: &str) -> Option<PathBuf> {
    let path = Path::new(backup_file_name);
    matches!(path.components().next(), Some(Component::Normal(_))).then_some(())?;
    (path.components().count() == 1).then_some(backups_root.join(path))
}

fn schema_upgrade_backup_manifest_path(backup_path: &Path) -> Result<PathBuf, PersistenceError> {
    let file_name = backup_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            PersistenceError::InvariantViolation(
                "schema upgrade backup has no valid manifest name".to_string(),
            )
        })?;
    Ok(backup_path.with_file_name(format!(
        "{file_name}{SCHEMA_UPGRADE_BACKUP_MANIFEST_SUFFIX}"
    )))
}

fn read_schema_upgrade_backup_manifest(path: &Path) -> Option<SchemaUpgradeBackupManifest> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

fn write_schema_upgrade_backup_manifest(
    backup_path: &Path,
    manifest: &SchemaUpgradeBackupManifest,
) -> Result<(), PersistenceError> {
    let manifest_path = schema_upgrade_backup_manifest_path(backup_path)?;
    let temporary_path = manifest_path.with_file_name(format!(
        "{}.tmp",
        manifest_path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| PersistenceError::InvariantViolation(
                "schema upgrade backup manifest has no valid file name".to_string(),
            ))?
    ));
    let bytes = serde_json::to_vec_pretty(manifest).map_err(|_| {
        PersistenceError::InvariantViolation(
            "failed to serialize schema upgrade backup manifest".to_string(),
        )
    })?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temporary_path)?;
    file.write_all(&bytes)?;
    file.sync_all()?;
    fs::rename(temporary_path, manifest_path)?;
    Ok(())
}

fn schema_upgrade_backup_path_to_schema(
    database_path: &Path,
    source_schema: i64,
    target_schema: i64,
) -> Result<PathBuf, PersistenceError> {
    let parent = database_path.parent().ok_or(PersistenceError::IoFailed {
        kind: std::io::ErrorKind::InvalidInput,
    })?;
    Ok(parent.join("backups").join(format!(
        "relay-pool-v2-schema-{source_schema}-to-{target_schema}-{}.sqlite3",
        uuid::Uuid::now_v7()
    )))
}

pub(crate) fn migrator_through(
    target_schema: i64,
) -> Result<sqlx::migrate::Migrator, PersistenceError> {
    let migrations: Vec<_> = migrator()
        .iter()
        .filter(|migration| migration.version <= target_schema)
        .cloned()
        .collect();
    if !migrations
        .iter()
        .any(|migration| migration.version == target_schema)
    {
        return Err(PersistenceError::InvariantViolation(format!(
            "target schema {target_schema} is not present in migration registry"
        )));
    }
    Ok(sqlx::migrate::Migrator {
        migrations: Cow::Owned(migrations),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    })
}

#[cfg(test)]
mod tests {
    use std::{borrow::Cow, fs};

    use sha2::{Digest, Sha384};
    use sqlx::{migrate::Migrator, Connection, Row};

    use super::*;
    use crate::persistence::runtime::PersistenceRuntime;

    #[test]
    fn model_mapping_foundation_checksum_is_frozen() {
        let mut hasher = Sha384::new();
        hasher.update(include_bytes!(
            "migrations/0043_model_mapping_foundation.sql"
        ));
        let checksum = hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02X}"))
            .collect::<String>();
        assert_eq!(
            checksum,
            "3D6D2CFC7A8708FB1FBF7F5053EBBB7A151C01AD6A23C9CDE7AF95A8C64589DF6D061CDD60BFDEF7F4FBE75E2A5080BF"
        );
    }

    #[tokio::test]
    async fn schema_43_upgrade_applies_legacy_priority_repair() {
        let mut connection = sqlx::SqliteConnection::connect("sqlite::memory:")
            .await
            .expect("open sqlite");
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&mut connection)
            .await
            .expect("enable foreign keys");
        migrator_through(42)
            .expect("schema 42 migrator")
            .run(&mut connection)
            .await
            .expect("migrate schema 42");
        sqlx::query(
            "INSERT INTO model_aliases
                (id, client_model, upstream_model, enabled, created_at, updated_at)
             VALUES ('mapping-upgrade-alias', 'codex-upgrade', 'native-upgrade', 1, '1', '1')",
        )
        .execute(&mut connection)
        .await
        .expect("insert legacy alias");

        migrator_through(43)
            .expect("schema 43 migrator")
            .run(&mut connection)
            .await
            .expect("migrate schema 43");
        sqlx::query(
            "UPDATE model_mapping_rules SET priority = 0
             WHERE id = 'legacy-model-alias-rule:636F6465782D75706772616465'",
        )
        .execute(&mut connection)
        .await
        .expect("simulate legacy priority variant");
        sqlx::query(
            "UPDATE model_mapping_document_history
             SET document_json = json_set(document_json, '$.rules[0].priority', 0)
             WHERE revision = 1 AND source = 'migration'",
        )
        .execute(&mut connection)
        .await
        .expect("simulate legacy history variant");
        let before_repair: i64 = sqlx::query_scalar(
            "SELECT priority FROM model_mapping_rules
             WHERE id = 'legacy-model-alias-rule:636F6465782D75706772616465'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("legacy priority before repair");
        assert_eq!(before_repair, 0);

        migrator()
            .run(&mut connection)
            .await
            .expect("upgrade schema 43 to latest");
        let after_repair: i64 = sqlx::query_scalar(
            "SELECT priority FROM model_mapping_rules
             WHERE id = 'legacy-model-alias-rule:636F6465782D75706772616465'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("legacy priority after repair");
        assert_eq!(after_repair, 1);
        let history_priority: i64 = sqlx::query_scalar(
            "SELECT json_extract(document_json, '$.rules[0].priority')
             FROM model_mapping_document_history
             WHERE revision = 1 AND source = 'migration'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("history priority");
        assert_eq!(history_priority, 1);
        let current_schema: i64 = sqlx::query_scalar(
            "SELECT schema_version FROM persistence_schema_compatibility
             WHERE singleton_key = 1",
        )
        .fetch_one(&mut connection)
        .await
        .expect("current schema");
        assert_eq!(current_schema, current_schema_version());
    }

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
    async fn schema_55_to_56_migration_reaches_its_declared_postcondition() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 55).await;

        let backup = upgrade_existing_v2_database_to_schema(&path, 56)
            .await
            .expect("upgrade schema 55 to 56")
            .expect("schema 55 upgrade creates backup");

        assert_eq!(database_schema_version(&backup).await, 55);
        assert_eq!(database_schema_version(&path).await, 56);
    }

    #[tokio::test]
    async fn schema_56_to_57_discards_legacy_pricing_rules_after_verified_backup() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 56).await;

        let pool = migration_pool_existing(&path)
            .await
            .expect("schema 56 pool");
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
             ) VALUES (
                'station-legacy', 'Legacy station', 'sub2api',
                'https://example.test', 'https://example.test/v1', '1', '1'
             )",
        )
        .execute(&pool)
        .await
        .expect("legacy station");
        sqlx::query(
            "INSERT INTO pricing_rules (
                id, station_id, model, fixed_price, currency, unit, price_type,
                normalization_status, source, created_at, updated_at
             ) VALUES (
                'pricing-rule-legacy', 'station-legacy', 'legacy-model', 42.0, 'USD',
                'per_request', 'fixed', 'legacy_fixed', 'fixture', '1', '1'
             )",
        )
        .execute(&pool)
        .await
        .expect("legacy pricing rule");
        sqlx::query(
            "INSERT INTO request_logs (
                id, request_id, started_at, method, path, endpoint, status, created_at,
                base_input_cost, base_output_cost, base_fixed_cost, base_total_cost,
                pricing_rule_id
             ) VALUES (
                'request-legacy', 'request-legacy', '1', 'POST', '/v1/chat/completions',
                'chat_completions', 'success', '1', 0.4, 0.8, 42.0, 43.2,
                'pricing-rule-legacy'
             )",
        )
        .execute(&pool)
        .await
        .expect("legacy request log");
        sqlx::query(
            "INSERT INTO change_event_occurrences (
                id, source_observation_key, event_type, category, observation_kind, severity,
                object_type, station_id, pricing_rule_id, request_log_id, source,
                observed_at_ms, created_at_ms, seen_at_ms
             ) VALUES (
                'occurrence-legacy', 'legacy:pricing-rule-legacy', 'pricing_rule_changed',
                'audit_change', 'change', 'warning', 'pricing_rule', 'station-legacy',
                'pricing-rule-legacy', 'request-legacy', 'fixture', 100, 101, 102
             )",
        )
        .execute(&pool)
        .await
        .expect("legacy occurrence");
        pool.close().await;

        let backup = upgrade_existing_v2_database_to_schema(&path, 57)
            .await
            .expect("schema 56 upgrade")
            .expect("schema 56 upgrade backup");
        assert_eq!(database_schema_version(&backup).await, 56);
        assert_eq!(database_schema_version(&path).await, 57);

        let pool = migration_pool_existing(&path)
            .await
            .expect("schema 57 pool");
        let legacy_table_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'pricing_rules'",
        )
        .fetch_one(&pool)
        .await
        .expect("pricing rules table check");
        assert_eq!(legacy_table_count, 0);
        for (table, column) in [
            ("request_logs", "base_input_cost"),
            ("request_logs", "base_output_cost"),
            ("request_logs", "base_fixed_cost"),
            ("request_logs", "base_total_cost"),
            ("request_logs", "pricing_rule_id"),
            ("change_event_occurrences", "pricing_rule_id"),
        ] {
            let column_count: i64 =
                sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2")
                    .bind(table)
                    .bind(column)
                    .fetch_one(&pool)
                    .await
                    .expect("legacy column check");
            assert_eq!(column_count, 0, "{table}.{column} must be removed");
        }
        let retained: (String, String, Option<String>, Option<i64>) = sqlx::query_as(
            "SELECT source_observation_key, source, request_log_id, seen_at_ms
             FROM change_event_occurrences WHERE id = 'occurrence-legacy'",
        )
        .fetch_one(&pool)
        .await
        .expect("retained occurrence");
        assert_eq!(retained.0, "legacy:pricing-rule-legacy");
        assert_eq!(retained.1, "fixture");
        assert_eq!(retained.2.as_deref(), Some("request-legacy"));
        assert_eq!(retained.3, Some(102));
        let retained_request_log_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM request_logs WHERE id = 'request-legacy'")
                .fetch_one(&pool)
                .await
                .expect("retained request log");
        assert_eq!(retained_request_log_count, 1);
        let foreign_key_violations = sqlx::query("PRAGMA foreign_key_check")
            .fetch_all(&pool)
            .await
            .expect("foreign key check");
        assert!(foreign_key_violations.is_empty());
        pool.close().await;

        let backup_pool = migration_pool_existing(&backup)
            .await
            .expect("open verified pre-upgrade backup");
        let legacy_costs: (Option<f64>, Option<f64>, Option<f64>, Option<f64>) = sqlx::query_as(
            "SELECT base_input_cost, base_output_cost, base_fixed_cost, base_total_cost
             FROM request_logs WHERE id = 'request-legacy'",
        )
        .fetch_one(&backup_pool)
        .await
        .expect("legacy costs in backup");
        assert_eq!(legacy_costs, (Some(0.4), Some(0.8), Some(42.0), Some(43.2)));
        backup_pool.close().await;
    }

    #[tokio::test]
    async fn known_legacy_schema_55_checksum_is_reconciled_before_upgrade() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 55).await;
        let pool = migration_pool_existing(&path).await.expect("open database");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 55")
            .bind(LEGACY_MIGRATION_55_CHECKSUM.as_slice())
            .execute(&pool)
            .await
            .expect("install legacy checksum");
        pool.close().await;

        upgrade_existing_v2_database_to_schema(&path, 56)
            .await
            .expect("legacy checksum is safely reconciled");
        assert_eq!(database_schema_version(&path).await, 56);
    }

    #[tokio::test]
    async fn known_legacy_schema_57_checksum_is_reconciled_before_upgrade() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 57).await;
        let pool = migration_pool_existing(&path).await.expect("open database");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 57")
            .bind(LEGACY_MIGRATION_57_CHECKSUM.as_slice())
            .execute(&pool)
            .await
            .expect("install legacy checksum");
        pool.close().await;

        upgrade_existing_v2_database_to_schema(&path, 58)
            .await
            .expect("legacy checksum is safely reconciled");
        assert_eq!(database_schema_version(&path).await, 58);
    }

    #[tokio::test]
    async fn legacy_schema_57_completion_runs_after_backup_and_removes_leftover_prices() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 57).await;
        let pool = migration_pool_existing(&path).await.expect("open database");
        for statement in [
            "ALTER TABLE request_logs ADD COLUMN base_input_cost REAL",
            "ALTER TABLE request_logs ADD COLUMN base_output_cost REAL",
            "ALTER TABLE request_logs ADD COLUMN base_total_cost REAL",
        ] {
            sqlx::query(statement)
                .execute(&pool)
                .await
                .expect("restore released schema 57 column");
        }
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 57")
            .bind(LEGACY_MIGRATION_57_CHECKSUM.as_slice())
            .execute(&pool)
            .await
            .expect("install legacy checksum");
        pool.close().await;

        let backup = upgrade_existing_v2_database_to_schema(&path, 58)
            .await
            .expect("legacy schema 57 is safely completed")
            .expect("verified pre-repair backup");
        assert_eq!(database_schema_version(&path).await, 58);

        let pool = migration_pool_existing(&path)
            .await
            .expect("upgraded database");
        let leftover_column_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('request_logs')
             WHERE name IN ('base_input_cost', 'base_output_cost', 'base_total_cost')",
        )
        .fetch_one(&pool)
        .await
        .expect("read repaired schema");
        assert_eq!(leftover_column_count, 0);
        pool.close().await;

        let backup_pool = migration_pool_existing(&backup)
            .await
            .expect("open pre-repair backup");
        let backed_up_column_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('request_logs')
             WHERE name IN ('base_input_cost', 'base_output_cost', 'base_total_cost')",
        )
        .fetch_one(&backup_pool)
        .await
        .expect("read backup schema");
        assert_eq!(backed_up_column_count, 3);
        backup_pool.close().await;
    }

    #[tokio::test]
    async fn unknown_historical_checksum_drift_is_rejected_without_repair() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 55).await;
        let pool = migration_pool_existing(&path).await.expect("open database");
        sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = 55")
            .bind(vec![0xA5_u8; 48])
            .execute(&pool)
            .await
            .expect("install unknown checksum");
        let error = reconcile_historical_migration_checksums(&pool)
            .await
            .expect_err("unknown checksum drift must fail closed");
        assert!(matches!(
            error,
            PersistenceError::MigrationChecksumMismatch { version: 55, .. }
        ));
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_upgrade_backup_reuses_only_a_matching_verified_manifest() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 55).await;
        let backups_root = root.path().join("backups");
        fs::create_dir_all(&backups_root).expect("backup root");
        fs::write(
            backups_root.join("interrupted-schema-55-to-56.sqlite3.tmp"),
            b"incomplete backup",
        )
        .expect("incomplete backup artifact");
        fs::write(
            backups_root.join("interrupted-schema-55-to-56.sqlite3.tmp-journal"),
            b"incomplete journal artifact",
        )
        .expect("incomplete journal artifact");

        let first = ensure_schema_upgrade_backup(&path, 55, 56)
            .await
            .expect("first backup");
        let first_manifest = schema_upgrade_backup_manifest_path(&first).expect("manifest path");
        assert!(first.is_file());
        assert!(first_manifest.is_file());

        let second = ensure_schema_upgrade_backup(&path, 55, 56)
            .await
            .expect("reused backup");
        assert_eq!(second, first, "unchanged source reuses the verified backup");

        write_migration_canary(&path).await;
        let third = ensure_schema_upgrade_backup(&path, 55, 56)
            .await
            .expect("backup after source change");
        assert_ne!(third, first, "source change requires a fresh backup");
        assert!(third.is_file());
        assert!(backups_root
            .join("interrupted-schema-55-to-56.sqlite3.tmp")
            .is_file());
        assert!(backups_root
            .join("interrupted-schema-55-to-56.sqlite3.tmp-journal")
            .is_file());
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
    async fn schema_50_materializes_v1_routing_rows_without_changing_revision() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 49).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let v1 = r#"{
            "version":1,
            "reliability_weight":4000,
            "responsiveness_weight":2500,
            "cost_weight":2000,
            "preference_weight":1500,
            "max_candidates":32,
            "exploration_share_basis_points":500,
            "allow_depleted_fallback":false,
            "affinity_enabled":false,
            "affinity_ttl_seconds":300,
            "max_rate_multiplier":null,
            "routing_group_filter":"all_groups",
            "outbound_proxy_mode":"inherit",
            "outbound_proxy_url":null
        }"#;
        sqlx::query(
            "UPDATE routing_policy SET config_json = ?1, policy_version = 'routing-policy-v1', config_revision = 7 WHERE singleton_key = 1",
        )
        .bind(v1)
        .execute(&pool)
        .await
        .expect("legacy active row");
        sqlx::query("UPDATE domain_revisions SET revision = 7 WHERE scope = 'routing_policy'")
            .execute(&pool)
            .await
            .expect("legacy policy revision");
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (7, ?1, 'routing-policy-v1', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(v1)
        .execute(&pool)
        .await
        .expect("legacy history row");

        migrator_through(50)
            .expect("schema 50 migrator")
            .run(&pool)
            .await
            .expect("schema 50 migration");
        let active: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("active policy");
        let history: String = sqlx::query_scalar(
            "SELECT config_json FROM routing_policy_history WHERE config_revision = 7",
        )
        .fetch_one(&pool)
        .await
        .expect("policy history");
        for json in [active, history] {
            let value: serde_json::Value = serde_json::from_str(&json).expect("valid V2 JSON");
            assert_eq!(value["version"], 2);
            assert_eq!(value["maxCandidates"], 32);
            assert_eq!(value["retryFailover"]["maxTotalAttempts"], 4);
            assert_eq!(value["retryFailover"]["maxSameTargetCapacityRetries"], 2);
            assert_eq!(value["retryFailover"]["capacityRetryWaitBudgetMs"], 2000);
            assert_eq!(
                value["retryFailover"]["allowCrossCapacityDomainFallback"],
                true
            );
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT config_revision FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("active revision");
        assert_eq!(revision, 7, "materialization must not create a policy edit");
        let policy_version: String =
            sqlx::query_scalar("SELECT policy_version FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("policy version");
        assert_eq!(policy_version, "routing-policy-v2");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_50_leaves_wrong_typed_v1_rows_for_typed_recovery() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 49).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let malformed = r#"{
            "version":1,
            "reliability_weight":4000,
            "responsiveness_weight":2500,
            "cost_weight":2000,
            "preference_weight":1500,
            "max_candidates":32,
            "exploration_share_basis_points":500,
            "allow_depleted_fallback":"false",
            "affinity_enabled":false,
            "affinity_ttl_seconds":300
        }"#;
        sqlx::query(
            "UPDATE routing_policy SET config_json = ?1, policy_version = 'routing-policy-v1' WHERE singleton_key = 1",
        )
        .bind(malformed)
        .execute(&pool)
        .await
        .expect("malformed active row");

        migrator_through(50)
            .expect("schema 50 migrator")
            .run(&pool)
            .await
            .expect("schema 50 migration");
        let config_json: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("active policy");
        assert_eq!(config_json, malformed);
        let policy_version: String =
            sqlx::query_scalar("SELECT policy_version FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("policy version");
        assert_eq!(policy_version, "routing-policy-v1");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_52_materializes_missing_protection_profile_on_existing_v2_rows() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 51).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let v2_without_profile = serde_json::json!({
            "version": 2,
            "reliabilityWeight": 4000,
            "responsivenessWeight": 2500,
            "costWeight": 2000,
            "preferenceWeight": 1500,
            "maxCandidates": 32,
            "explorationShareBasisPoints": 500,
            "allowDepletedFallback": false,
            "affinityEnabled": false,
            "affinityTtlSeconds": 300,
            "maxRateMultiplier": null,
            "routingGroupFilter": "all_groups",
            "outboundProxyMode": "inherit",
            "outboundProxyUrl": null,
            "retryFailover": {
                "version": 1,
                "maxTotalAttempts": 4,
                "maxSameTargetCapacityRetries": 2,
                "capacityRetryWaitBudgetMs": 2000,
                "allowCrossCapacityDomainFallback": true
            }
        });
        let v2_json = serde_json::to_string(&v2_without_profile).expect("V2 JSON");
        sqlx::query("UPDATE routing_policy SET config_json = ?1, policy_version = 'routing-policy-v2' WHERE singleton_key = 1")
            .bind(&v2_json)
            .execute(&pool)
            .await
            .expect("legacy V2 active row");
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (8, ?1, 'routing-policy-v2', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(&v2_json)
        .execute(&pool)
        .await
        .expect("legacy V2 history row");
        let mut explicit_profile = v2_without_profile.clone();
        explicit_profile["protectionProfile"] = serde_json::json!({
            "version": 1,
            "enabled": true,
            "windowMaxSamples": 8,
            "windowMs": 10000,
            "minSamples": 2,
            "failureThresholdPercent": 40,
            "halfOpenSuccessesToClose": 3
        });
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (9, ?1, 'routing-policy-v2', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(serde_json::to_string(&explicit_profile).expect("explicit profile JSON"))
        .execute(&pool)
        .await
        .expect("explicit V2 history row");

        migrator_through(52)
            .expect("schema 52 migrator")
            .run(&pool)
            .await
            .expect("schema 52 migration");
        for json in [
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy WHERE singleton_key = 1",
            )
            .fetch_one(&pool)
            .await
            .expect("active policy"),
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy_history WHERE config_revision = 8",
            )
            .fetch_one(&pool)
            .await
            .expect("history policy"),
        ] {
            let value: serde_json::Value = serde_json::from_str(&json).expect("valid JSON");
            assert_eq!(value["version"], 2);
            assert_eq!(value["protectionProfile"]["version"], 1);
            assert_eq!(value["protectionProfile"]["enabled"], false);
            assert_eq!(value["protectionProfile"]["windowMaxSamples"], 64);
            assert_eq!(value["protectionProfile"]["windowMs"], 300000);
        }
        let preserved: String = sqlx::query_scalar(
            "SELECT config_json FROM routing_policy_history WHERE config_revision = 9",
        )
        .fetch_one(&pool)
        .await
        .expect("preserved explicit profile");
        let preserved: serde_json::Value = serde_json::from_str(&preserved).expect("valid JSON");
        assert_eq!(preserved["protectionProfile"]["enabled"], true);
        assert_eq!(preserved["protectionProfile"]["windowMaxSamples"], 8);
        let revision: i64 = sqlx::query_scalar(
            "SELECT config_revision FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("active revision");
        assert_eq!(revision, 1, "materialization must not create a policy edit");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_54_converts_active_and_history_policy_durations_to_seconds() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 53).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        let current: String =
            sqlx::query_scalar("SELECT config_json FROM routing_policy WHERE singleton_key = 1")
                .fetch_one(&pool)
                .await
                .expect("current policy");
        let mut legacy: serde_json::Value =
            serde_json::from_str(&current).expect("legacy policy JSON");
        legacy["retryFailover"]["capacityRetryWaitBudgetMs"] = serde_json::json!(750);
        legacy["protectionProfile"]["windowMs"] = serde_json::json!(300_500);
        legacy["timeoutPolicy"]["connectMs"] = serde_json::json!(1_250);
        legacy["timeoutPolicy"]["firstByteMs"] = serde_json::json!(30_500);
        legacy["timeoutPolicy"]["precommitMs"] = serde_json::json!(60_250);
        legacy["timeoutPolicy"]["bufferedExecutionMs"] = serde_json::json!(300_750);
        legacy["timeoutPolicy"]["streamIdleMs"] = serde_json::json!(90_125);
        let legacy_json = serde_json::to_string(&legacy).expect("legacy policy serialization");
        sqlx::query(
            "UPDATE routing_policy SET config_json = ?1, config_revision = 17 WHERE singleton_key = 1",
        )
        .bind(&legacy_json)
        .execute(&pool)
        .await
        .expect("legacy active policy");
        sqlx::query(
            "INSERT OR REPLACE INTO routing_policy_history (config_revision, config_json, policy_version, system_version, status, created_at_ms) VALUES (18, ?1, 'routing-policy-v2', 'intelligent-routing-engine', 'active', 0)",
        )
        .bind(&legacy_json)
        .execute(&pool)
        .await
        .expect("legacy history policy");

        migrator_through(54)
            .expect("schema 54 migrator")
            .run(&pool)
            .await
            .expect("schema 54 migration");
        for json in [
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy WHERE singleton_key = 1",
            )
            .fetch_one(&pool)
            .await
            .expect("migrated active policy"),
            sqlx::query_scalar::<_, String>(
                "SELECT config_json FROM routing_policy_history WHERE config_revision = 18",
            )
            .fetch_one(&pool)
            .await
            .expect("migrated history policy"),
        ] {
            let value: serde_json::Value = serde_json::from_str(&json).expect("migrated JSON");
            assert_eq!(value["retryFailover"]["version"], 2);
            assert_eq!(
                value["retryFailover"]["capacityRetryWaitBudgetSeconds"],
                0.75
            );
            assert!(value["retryFailover"]["capacityRetryWaitBudgetMs"].is_null());
            assert_eq!(value["protectionProfile"]["version"], 2);
            assert_eq!(value["protectionProfile"]["windowSeconds"], 300.5);
            assert!(value["protectionProfile"]["windowMs"].is_null());
            assert_eq!(value["timeoutPolicy"]["version"], 2);
            assert_eq!(value["timeoutPolicy"]["connectSeconds"], 1.25);
            assert_eq!(value["timeoutPolicy"]["streamIdleSeconds"], 90.125);
            assert!(value["timeoutPolicy"]["connectMs"].is_null());
        }
        let revision: i64 = sqlx::query_scalar(
            "SELECT config_revision FROM routing_policy WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("active revision");
        assert_eq!(revision, 17, "unit migration must not create a policy edit");
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_55_upgrades_the_historical_collector_timeout_default() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 54).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");

        let before: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'collector_timeout_seconds'",
        )
        .fetch_one(&pool)
        .await
        .expect("historical timeout default");
        assert_eq!(before, "15");

        migrator_through(55)
            .expect("schema 55 migrator")
            .run(&pool)
            .await
            .expect("schema 55 migration");

        let after: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'collector_timeout_seconds'",
        )
        .fetch_one(&pool)
        .await
        .expect("upgraded timeout default");
        let compatibility: i64 = sqlx::query_scalar(
            "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
        )
        .fetch_one(&pool)
        .await
        .expect("schema compatibility");
        assert_eq!(after, "30");
        assert_eq!(compatibility, 55);
        pool.close().await;
    }

    #[tokio::test]
    async fn schema_55_preserves_a_custom_collector_timeout() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 54).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        sqlx::query("UPDATE settings SET value = '45' WHERE key = 'collector_timeout_seconds'")
            .execute(&pool)
            .await
            .expect("custom timeout");

        migrator_through(55)
            .expect("schema 55 migrator")
            .run(&pool)
            .await
            .expect("schema 55 migration");

        let after: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'collector_timeout_seconds'",
        )
        .fetch_one(&pool)
        .await
        .expect("preserved custom timeout");
        assert_eq!(after, "45");
        pool.close().await;
    }

    #[tokio::test]
    async fn group_missing_incidents_upgrade_to_one_informational_change() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 41).await;

        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        sqlx::query(
            "INSERT INTO change_incidents (
                id, condition_key, event_type, lifecycle_state, base_severity, severity,
                object_type, lifecycle_policy_fingerprint, episode_number, first_seen_at_ms,
                last_seen_at_ms, occurrence_count, episode_occurrence_count,
                last_observation_summary_json, created_at_ms, updated_at_ms
             ) VALUES (
                'legacy-group-missing', 'station_group:station-1:group-1', 'group_missing',
                'resolved', 'warning', 'warning', 'station_group_binding', 'legacy',
                1, 100, 300, 1, 1, '{\"groupName\":\"Legacy group\"}', 100, 300
             )",
        )
        .execute(&pool)
        .await
        .expect("legacy group incident");
        sqlx::query(
            "INSERT INTO incident_attention (
                incident_id, episode_number, seen_at_ms, updated_at_ms
             ) VALUES ('legacy-group-missing', 1, 250, 250)",
        )
        .execute(&pool)
        .await
        .expect("legacy attention");
        for (id, kind, observed_at_ms) in [
            ("legacy-group-missing-abnormal", "abnormal", 200_i64),
            ("legacy-group-missing-healthy", "healthy", 300_i64),
        ] {
            sqlx::query(
                "INSERT INTO change_event_occurrences (
                    id, source_observation_key, event_type, category, observation_kind,
                    severity, condition_key, incident_id, episode_number, object_type, source,
                    new_value_json, observed_at_ms, created_at_ms
                 ) VALUES (?1, ?2, 'group_missing', 'condition_observation', ?3,
                           'warning', 'station_group:station-1:group-1',
                           'legacy-group-missing', 1, 'station_group_binding', 'legacy',
                           '{\"groupName\":\"Legacy group\"}', ?4, ?4)",
            )
            .bind(id)
            .bind(format!("fixture:{id}"))
            .bind(kind)
            .bind(observed_at_ms)
            .execute(&pool)
            .await
            .expect("legacy group occurrence");
        }

        migrator_through(42)
            .expect("schema 42 migrator")
            .run(&pool)
            .await
            .expect("upgrade missing group information");

        let incident_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_incidents WHERE event_type = 'group_missing'",
        )
        .fetch_one(&pool)
        .await
        .expect("group incident count");
        assert_eq!(incident_count, 0);
        let information_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_event_occurrences
             WHERE event_type = 'group_missing'
               AND category = 'audit_change'
               AND observation_kind = 'change'
               AND severity = 'info'
               AND incident_id IS NULL",
        )
        .fetch_one(&pool)
        .await
        .expect("group information count");
        assert_eq!(information_count, 1);
        let seen_at_ms: Option<i64> = sqlx::query_scalar(
            "SELECT seen_at_ms FROM change_event_occurrences
             WHERE event_type = 'group_missing' AND category = 'audit_change'",
        )
        .fetch_one(&pool)
        .await
        .expect("migrated seen state");
        assert_eq!(seen_at_ms, Some(250));
        let recovery_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_event_occurrences
             WHERE event_type = 'group_missing' AND observation_kind = 'healthy'",
        )
        .fetch_one(&pool)
        .await
        .expect("group recovery count");
        assert_eq!(recovery_count, 0);
        let attention_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM incident_attention WHERE incident_id = 'legacy-group-missing'",
        )
        .fetch_one(&pool)
        .await
        .expect("legacy attention cleanup");
        assert_eq!(attention_count, 0);

        pool.close().await;
    }

    #[tokio::test]
    async fn station_published_status_migration_creates_constrained_fact_tables() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_v2_database(&path)
            .await
            .expect("initialize database");

        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        assert_eq!(
            applied_schema_version(&pool).await.expect("schema version"),
            current_schema_version()
        );
        for table in [
            "station_published_status_sources",
            "station_published_monitors",
            "station_published_monitor_samples",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .expect("table existence");
            assert_eq!(exists, 1, "missing {table}");
        }
        for index in [
            "idx_station_published_status_sources_station_revision",
            "idx_station_published_monitors_workspace",
            "idx_station_published_monitor_samples_timeline",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            )
            .bind(index)
            .fetch_one(&pool)
            .await
            .expect("index existence");
            assert_eq!(exists, 1, "missing {index}");
        }
        let interval: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'published_status_interval_minutes'",
        )
        .fetch_one(&pool)
        .await
        .expect("published status interval setting");
        assert_eq!(interval, "5");

        let missing_station = sqlx::query(
            r#"
            INSERT INTO station_published_status_sources (
                station_id, endpoint_revision, source_kind, source_state, last_attempt_at,
                monitor_count, created_at, updated_at
            ) VALUES ('missing', 1, 'fixture', 'available', '0', 0, '0', '0')
            "#,
        )
        .execute(&pool)
        .await;
        assert!(
            missing_station.is_err(),
            "source station foreign key must hold"
        );

        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
            ) VALUES (
                'published-status-station', 'Published Status Fixture', 'sub2api',
                'https://example.invalid', 'https://example.invalid/v1', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture station");
        sqlx::query(
            r#"
            INSERT INTO station_published_status_sources (
                station_id, endpoint_revision, source_kind, source_state, last_attempt_at,
                monitor_count, created_at, updated_at
            ) VALUES ('published-status-station', 1, 'fixture', 'available', '0', 1, '0', '0')
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture source");
        sqlx::query(
            r#"
            INSERT INTO station_published_monitors (
                id, station_id, endpoint_revision, source_kind, upstream_monitor_id,
                identity_kind, name, provider, primary_model, extra_models_json,
                presence_status, current_outcome, source_status, last_seen_run_id,
                last_seen_at, created_at, updated_at
            ) VALUES (
                'published-status-monitor', 'published-status-station', 1, 'fixture',
                'upstream-monitor', 'upstream_id', 'Fixture Monitor', 'fixture', 'fixture-model',
                '[]', 'current', 'available', 'available', 'fixture-run', '0', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture monitor");
        sqlx::query(
            r#"
            INSERT INTO station_published_monitor_samples (
                id, monitor_id, model, checked_at, outcome, source_status,
                first_seen_run_id, last_seen_run_id, created_at, updated_at
            ) VALUES (
                'published-status-sample', 'published-status-monitor', 'fixture-model', '0',
                'available', 'available', 'fixture-run', 'fixture-run', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await
        .expect("fixture sample");
        let duplicate = sqlx::query(
            r#"
            INSERT INTO station_published_monitor_samples (
                id, monitor_id, model, checked_at, outcome, source_status,
                first_seen_run_id, last_seen_run_id, created_at, updated_at
            ) VALUES (
                'duplicate-published-status-sample', 'published-status-monitor', 'fixture-model',
                '0', 'available', 'available', 'fixture-run', 'fixture-run', '0', '0'
            )
            "#,
        )
        .execute(&pool)
        .await;
        assert!(duplicate.is_err(), "sample identity must be unique");

        sqlx::query("DELETE FROM stations WHERE id = 'published-status-station'")
            .execute(&pool)
            .await
            .expect("delete fixture station");
        let remaining: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM station_published_monitor_samples WHERE monitor_id = 'published-status-monitor'",
        )
        .fetch_one(&pool)
        .await
        .expect("remaining samples");
        assert_eq!(remaining, 0, "station delete must cascade published facts");
        pool.close().await;
    }

    #[tokio::test]
    async fn remote_key_upgrade_repairs_duplicate_local_owners_before_enforcing_uniqueness() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 10).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
             ) VALUES ('station-1', 'Station', 'sub2api', 'https://example.test',
                       'https://example.test/v1', '1', '1')",
        )
        .execute(&pool)
        .await
        .expect("insert station");
        sqlx::query(
            "INSERT INTO station_keys (id, station_id, note)
             VALUES ('local-1', 'station-1', '由远端发现开关自动创建：remote-owned')",
        )
        .execute(&pool)
        .await
        .expect("insert station key");
        for (id, confidence, collected_at) in
            [("remote-other", 1.0, "3"), ("remote-owned", 0.9, "2")]
        {
            sqlx::query(
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
        let duplicate = sqlx::query(
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
        sqlx::query(
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
            sqlx::query(
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

    #[tokio::test]
    async fn schema_21_quarantines_removed_collector_providers_without_deleting_assets() {
        let root = tempfile::tempdir().expect("tempdir");
        let path = root.path().join("relay-pool-v2.sqlite3");
        initialize_database_through(&path, 20).await;
        let pool = migration_pool_existing(&path)
            .await
            .expect("migration pool");

        for (id, station_type) in [
            ("legacy-openai", "openai-compatible"),
            ("legacy-custom", "custom"),
            ("newapi-station", "newapi"),
            ("sub2api-station", "sub2api"),
        ] {
            sqlx::query(
                "INSERT INTO stations (
                    id, name, station_type, website_url, api_base_url,
                    enabled, status, created_at, updated_at
                 ) VALUES (?1, ?1, ?2, 'https://example.test',
                           'https://example.test/v1', 1, 'healthy', '1', '1')",
            )
            .bind(id)
            .bind(station_type)
            .execute(&pool)
            .await
            .expect("insert station");
            sqlx::query(
                "INSERT INTO station_keys (id, station_id, created_at, updated_at)
                 VALUES (?1 || '-key', ?1, '1', '1')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert station key");
            sqlx::query(
                "INSERT INTO collector_runs (
                    id, run_key, request_hash, station_id, endpoint_revision,
                    adapter, task_type, status, started_at, created_at
                 ) VALUES (?1 || '-run', ?1 || '-run-key', 'hash', ?1, 1,
                           ?2, 'models', 'success', '1', '1')",
            )
            .bind(id)
            .bind(station_type)
            .execute(&pool)
            .await
            .expect("insert collector run");
            sqlx::query(
                "INSERT INTO collector_task_state (
                    station_id, task_type, last_run_id, last_status, updated_at
                 ) VALUES (?1, 'models', ?1 || '-run', 'success', '1')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert task state");
            sqlx::query(
                "INSERT INTO collector_model_facts (
                    station_id, model, available, source, confidence, last_seen_run_id, updated_at
                 ) VALUES (?1, 'fixture-model', 1, 'fixture', 1.0, ?1 || '-run', '1')",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert model fact");
            sqlx::query(
                "INSERT INTO change_events (
                    id, severity, event_type, status, title, message, object_type,
                    object_id, station_id, dedupe_key, source, detected_at, created_at, updated_at
                 ) VALUES (
                    ?1 || '-model-event', 'info', 'model_added', 'unread',
                    'Model added', 'fixture', 'model', 'fixture-model', ?1,
                    ?1 || '-model-event', 'collector', '1', '1', '1'
                 )",
            )
            .bind(id)
            .execute(&pool)
            .await
            .expect("insert change event");
        }
        migrator_through(21)
            .expect("schema 21 migrator")
            .run(&pool)
            .await
            .expect("apply schema 21 quarantine");
        let quarantined_model_events: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM change_events WHERE event_type IN ('model_added', 'model_removed')",
        )
        .fetch_one(&pool)
        .await
        .expect("schema 21 model events");
        assert_eq!(quarantined_model_events, 0);
        pool.close().await;

        upgrade_existing_v2_database(&path)
            .await
            .expect("upgrade schema");

        let pool = migration_pool_existing(&path).await.expect("upgraded pool");
        let disabled_legacy_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM stations
             WHERE station_type IN ('openai-compatible', 'openai_compatible', 'custom')
               AND enabled = 0
               AND status = 'disabled'",
        )
        .fetch_one(&pool)
        .await
        .expect("disabled legacy count");
        assert_eq!(disabled_legacy_count, 2);
        let station_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM stations")
            .fetch_one(&pool)
            .await
            .expect("station count");
        let station_key_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM station_keys")
            .fetch_one(&pool)
            .await
            .expect("station key count");
        assert_eq!(station_count, 4);
        assert_eq!(station_key_count, 4);
        let remaining_model_facts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_model_facts
             WHERE station_id IN ('newapi-station', 'legacy-openai', 'legacy-custom')",
        )
        .fetch_one(&pool)
        .await
        .expect("remaining model facts");
        assert_eq!(remaining_model_facts, 0);
        let sub2api_model_facts: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_model_facts WHERE station_id = 'sub2api-station'",
        )
        .fetch_one(&pool)
        .await
        .expect("sub2api model facts");
        assert_eq!(sub2api_model_facts, 1);
        let removed_model_task_state: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM collector_task_state WHERE task_type = 'models'",
        )
        .fetch_one(&pool)
        .await
        .expect("models task state");
        assert_eq!(removed_model_task_state, 0);
        let legacy_change_events_table: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'change_events'",
        )
        .fetch_one(&pool)
        .await
        .expect("legacy change events table postcondition");
        assert_eq!(legacy_change_events_table, 0);
        let model_setting_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM settings WHERE key = 'model_list_interval_minutes'",
        )
        .fetch_one(&pool)
        .await
        .expect("model setting");
        assert_eq!(model_setting_count, 0);
        pool.close().await;

        assert_eq!(
            database_schema_version(&path).await,
            current_schema_version()
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
