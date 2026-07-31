use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteSynchronous},
    Connection, Row, SqliteConnection,
};

use crate::{
    persistence::{
        self,
        upgrade_recovery_executor::{observe_persistence_journal, PersistenceJournalKind},
    },
    services::data_store::types::RecoveryReason,
    services::secrets::baseline_conversion::{
        ACTIVE_KEY_ID_SETTING, ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION,
        ENCRYPTED_SECRET_FORMAT_VERSION, SECRET_FORMAT_VERSION_SETTING,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SecretFormatProbe {
    Legacy,
    EncryptedBaseline,
    InvalidMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StartupKeyRequirementProbe {
    LegacyFormat,
    Verified {
        persisted_key_id: String,
        system_key_id: String,
    },
    MissingPersistedKeyId {
        system_key_id: Option<String>,
    },
    MissingSystemKeyId {
        persisted_key_id: String,
    },
    MismatchedKeyId {
        persisted_key_id: String,
        system_key_id: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupJournalProbe {
    Missing,
    GenerationUpgrade,
    BaselineConversion,
    Invalid,
    NotChecked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupUpgradeProbe {
    pub(crate) active_database_path: PathBuf,
    pub(crate) compatibility_schema_version: i64,
    pub(crate) sql_migration_version: i64,
    pub(crate) latest_sql_migration_version: i64,
    pub(crate) secret_format: SecretFormatProbe,
    pub(crate) key_requirement: StartupKeyRequirementProbe,
    pub(crate) journal: StartupJournalProbe,
    pub(crate) sqlite_quick_check_passed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupProbeErrorKind {
    OpenFailed,
    LockedOrBusy,
    PermissionDenied,
    MissingCompatibilityMetadata,
    MissingMigrationMetadata,
    MissingSettingsTable,
    CorruptedDatabase,
    QueryFailed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupProbeError {
    kind: StartupProbeErrorKind,
    message: String,
}

impl StartupProbeError {
    fn new(kind: StartupProbeErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    #[cfg(test)]
    const fn kind(&self) -> StartupProbeErrorKind {
        self.kind
    }

    pub(crate) const fn recovery_reason(&self) -> RecoveryReason {
        match self.kind {
            StartupProbeErrorKind::LockedOrBusy | StartupProbeErrorKind::PermissionDenied => {
                RecoveryReason::Unreadable
            }
            StartupProbeErrorKind::MissingCompatibilityMetadata
            | StartupProbeErrorKind::MissingMigrationMetadata
            | StartupProbeErrorKind::MissingSettingsTable => {
                RecoveryReason::InconsistentSchemaMetadata
            }
            StartupProbeErrorKind::OpenFailed
            | StartupProbeErrorKind::CorruptedDatabase
            | StartupProbeErrorKind::QueryFailed => RecoveryReason::CorruptedDatabase,
        }
    }
}

impl std::fmt::Display for StartupProbeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

pub(crate) fn probe_upgrade_state_with_journal(
    path: &Path,
    journal_path: Option<&Path>,
    system_active_key_id: Option<&str>,
) -> Result<StartupUpgradeProbe, StartupProbeError> {
    block_on(async {
        let mut connection = connect_read_only(path).await?;
        let sqlite_quick_check_passed = read_sqlite_quick_check(&mut connection).await?;
        let compatibility_schema_version = read_compatibility_schema(&mut connection).await?;
        let sql_migration_version = read_sql_migration_version(&mut connection).await?;
        let secret_format =
            read_secret_format(&mut connection, compatibility_schema_version).await?;
        let key_requirement =
            read_key_requirement(&mut connection, secret_format, system_active_key_id).await?;
        connection
            .close()
            .await
            .map_err(|error| classify_sqlx_error("close startup upgrade probe", error))?;
        Ok::<_, StartupProbeError>(StartupUpgradeProbe {
            active_database_path: path.to_path_buf(),
            compatibility_schema_version,
            sql_migration_version,
            latest_sql_migration_version: persistence::current_schema_version(),
            secret_format,
            key_requirement,
            journal: journal_path
                .map(read_journal_probe)
                .unwrap_or(StartupJournalProbe::NotChecked),
            sqlite_quick_check_passed,
        })
    })
}

async fn connect_read_only(path: &Path) -> Result<SqliteConnection, StartupProbeError> {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .read_only(true)
            .synchronous(SqliteSynchronous::Full)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5)),
    )
    .await
    .map_err(|error| classify_open_error("read startup upgrade state", error))
}

async fn read_compatibility_schema(
    connection: &mut SqliteConnection,
) -> Result<i64, StartupProbeError> {
    let row = sqlx::query(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(connection)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => StartupProbeError::new(
            StartupProbeErrorKind::MissingCompatibilityMetadata,
            "missing compatibility schema metadata",
        ),
        other => classify_required_metadata_query_error(
            "read compatibility schema version",
            StartupProbeErrorKind::MissingCompatibilityMetadata,
            other,
        ),
    })?;
    Ok(row.get("schema_version"))
}

async fn read_sql_migration_version(
    connection: &mut SqliteConnection,
) -> Result<i64, StartupProbeError> {
    let row = sqlx::query(
        r#"
        SELECT version
        FROM _sqlx_migrations
        WHERE success = 1
        ORDER BY version DESC
        LIMIT 1
        "#,
    )
    .fetch_one(connection)
    .await
    .map_err(|error| match error {
        sqlx::Error::RowNotFound => StartupProbeError::new(
            StartupProbeErrorKind::MissingMigrationMetadata,
            "missing SQL migration ledger metadata",
        ),
        other => classify_required_metadata_query_error(
            "read SQL migration ledger version",
            StartupProbeErrorKind::MissingMigrationMetadata,
            other,
        ),
    })?;
    Ok(row.get("version"))
}

async fn read_sqlite_quick_check(
    connection: &mut SqliteConnection,
) -> Result<bool, StartupProbeError> {
    let value = sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(connection)
        .await
        .map_err(|error| classify_sqlx_error("run SQLite quick_check", error))?;
    Ok(value.eq_ignore_ascii_case("ok"))
}

async fn read_secret_format(
    connection: &mut SqliteConnection,
    compatibility_schema_version: i64,
) -> Result<SecretFormatProbe, StartupProbeError> {
    let explicit = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
        .bind(SECRET_FORMAT_VERSION_SETTING)
        .fetch_optional(&mut *connection)
        .await
        .map_err(|error| classify_settings_query_error("read secret format metadata", error))?;
    match explicit.as_deref() {
        Some(value) if value == ENCRYPTED_SECRET_FORMAT_VERSION.to_string() => {
            Ok(SecretFormatProbe::EncryptedBaseline)
        }
        Some("0") => Ok(SecretFormatProbe::Legacy),
        Some(_) => Ok(SecretFormatProbe::InvalidMetadata),
        None if compatibility_schema_version >= ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION => {
            Ok(SecretFormatProbe::EncryptedBaseline)
        }
        None => Ok(SecretFormatProbe::Legacy),
    }
}

async fn read_key_requirement(
    connection: &mut SqliteConnection,
    secret_format: SecretFormatProbe,
    system_active_key_id: Option<&str>,
) -> Result<StartupKeyRequirementProbe, StartupProbeError> {
    if secret_format != SecretFormatProbe::EncryptedBaseline {
        return Ok(StartupKeyRequirementProbe::LegacyFormat);
    }
    let persisted = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
        .bind(ACTIVE_KEY_ID_SETTING)
        .fetch_optional(connection)
        .await
        .map_err(|error| classify_settings_query_error("read active key identity", error))?
        .and_then(|value| {
            let trimmed = value.trim();
            (!trimmed.is_empty()).then(|| trimmed.to_string())
        });
    let system = system_active_key_id.and_then(|value| {
        let trimmed = value.trim();
        (!trimmed.is_empty()).then(|| trimmed.to_string())
    });
    match (persisted, system) {
        (Some(persisted_key_id), Some(system_key_id)) if persisted_key_id == system_key_id => {
            Ok(StartupKeyRequirementProbe::Verified {
                persisted_key_id,
                system_key_id,
            })
        }
        (Some(persisted_key_id), Some(system_key_id)) => {
            Ok(StartupKeyRequirementProbe::MismatchedKeyId {
                persisted_key_id,
                system_key_id,
            })
        }
        (Some(persisted_key_id), None) => {
            Ok(StartupKeyRequirementProbe::MissingSystemKeyId { persisted_key_id })
        }
        (None, system_key_id) => {
            Ok(StartupKeyRequirementProbe::MissingPersistedKeyId { system_key_id })
        }
    }
}

fn read_journal_probe(path: &Path) -> StartupJournalProbe {
    match observe_persistence_journal(path).kind {
        PersistenceJournalKind::Missing => StartupJournalProbe::Missing,
        PersistenceJournalKind::GenerationUpgrade => StartupJournalProbe::GenerationUpgrade,
        PersistenceJournalKind::BaselineConversion => StartupJournalProbe::BaselineConversion,
        PersistenceJournalKind::Invalid => StartupJournalProbe::Invalid,
    }
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(future)
}

fn classify_settings_query_error(context: &str, error: sqlx::Error) -> StartupProbeError {
    if error
        .to_string()
        .to_ascii_lowercase()
        .contains("no such table: settings")
    {
        return StartupProbeError::new(
            StartupProbeErrorKind::MissingSettingsTable,
            format!("failed to {context}: {error}"),
        );
    }
    classify_sqlx_error(context, error)
}

fn classify_required_metadata_query_error(
    context: &str,
    missing_kind: StartupProbeErrorKind,
    error: sqlx::Error,
) -> StartupProbeError {
    if error
        .to_string()
        .to_ascii_lowercase()
        .contains("no such table")
    {
        return StartupProbeError::new(
            missing_kind,
            format!("failed to {context}: missing required metadata table"),
        );
    }
    classify_sqlx_error(context, error)
}

fn classify_open_error(context: &str, error: sqlx::Error) -> StartupProbeError {
    let classified = classify_sqlx_error(context, error);
    if classified.kind == StartupProbeErrorKind::QueryFailed {
        return StartupProbeError::new(StartupProbeErrorKind::OpenFailed, classified.message);
    }
    classified
}

fn classify_sqlx_error(context: &str, error: sqlx::Error) -> StartupProbeError {
    let message = error.to_string();
    let lower = message.to_ascii_lowercase();
    let kind = if lower.contains("database is locked")
        || lower.contains("database table is locked")
        || lower.contains("busy")
    {
        StartupProbeErrorKind::LockedOrBusy
    } else if lower.contains("permission denied")
        || lower.contains("readonly")
        || lower.contains("read-only")
    {
        StartupProbeErrorKind::PermissionDenied
    } else if lower.contains("database disk image is malformed")
        || lower.contains("file is not a database")
        || lower.contains("not a database")
    {
        StartupProbeErrorKind::CorruptedDatabase
    } else {
        StartupProbeErrorKind::QueryFailed
    };
    StartupProbeError::new(kind, format!("failed to {context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        services::data_store::config::DatabaseGeneration,
        services::secrets::baseline_conversion::initialize_fresh_database_at_baseline,
        services::secrets::baseline_conversion::resolver_from_parts,
    };
    use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};
    use std::fs;

    #[test]
    fn probe_reads_matching_active_key_identity_without_writes() {
        let root = tempfile::Builder::new()
            .prefix("startup-probe-key-id-")
            .tempdir()
            .expect("tempdir");
        let final_path = root.path().join(DatabaseGeneration::Two.database_file());
        let resolver = resolver_from_parts("device-key-v1", [61; 32]);
        initialize_fresh_database_at_baseline(&final_path, &resolver).expect("database");
        let before = fs::read(&final_path).expect("before");

        let probe = probe_upgrade_state_with_journal(
            &final_path,
            None,
            Some(resolver.active_key_id().as_str()),
        )
        .expect("probe");

        assert!(matches!(
            probe.key_requirement,
            StartupKeyRequirementProbe::Verified { .. }
        ));
        assert_eq!(fs::read(&final_path).expect("after"), before);
    }

    #[test]
    fn missing_compatibility_metadata_is_typed_as_inconsistent_schema() {
        let root = tempfile::Builder::new()
            .prefix("startup-probe-missing-compat-")
            .tempdir()
            .expect("tempdir");
        let final_path = root.path().join(DatabaseGeneration::Two.database_file());
        block_on(async {
            let mut connection = writable_connection(&final_path).await.expect("connect");
            sqlx::query(
                r#"
                CREATE TABLE _sqlx_migrations (
                    version BIGINT PRIMARY KEY,
                    description TEXT NOT NULL,
                    installed_on TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP,
                    success BOOLEAN NOT NULL,
                    checksum BLOB NOT NULL,
                    execution_time BIGINT NOT NULL
                )
                "#,
            )
            .execute(&mut connection)
            .await
            .expect("create migration ledger");
            sqlx::query(
                "INSERT INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (17, 'test', 1, X'00', 0)",
            )
            .execute(&mut connection)
            .await
            .expect("insert migration ledger");
            connection.close().await.expect("close");
        });

        let error = probe_upgrade_state_with_journal(&final_path, None, Some("device-key-v1"))
            .expect_err("missing compatibility metadata");

        assert_eq!(
            error.kind(),
            StartupProbeErrorKind::MissingCompatibilityMetadata
        );
        assert_eq!(
            error.recovery_reason(),
            RecoveryReason::InconsistentSchemaMetadata
        );
    }

    async fn writable_connection(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
        SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(SqliteJournalMode::Wal)
                .synchronous(SqliteSynchronous::Full)
                .foreign_keys(true)
                .busy_timeout(std::time::Duration::from_secs(5)),
        )
        .await
    }
}
