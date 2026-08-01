use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use base64::{engine::general_purpose, engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{SecondsFormat, Utc};
use rand::{rngs::OsRng, RngCore};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Connection, Row, SqliteConnection, SqlitePool,
};

use crate::{
    models::secrets::{canonical_secret_aad, mask_secret},
    persistence::{
        self, migrations, schema_registry,
        settings_compat::repair_legacy_settings_in_connection,
        upgrade_fault::NoUpgradeFaults,
        upgrade_journal::{
            BaselineConversionJournal, BaselineConversionPhase, UpgradeAttemptId, UtcTimestamp,
        },
        upgrade_recovery_executor::{
            observe_persistence_journal, remove_file_and_sync_parent_with_faults, sha256_file,
            write_baseline_conversion_journal_atomically, PersistenceJournalKind,
            UPGRADE_JOURNAL_FILE,
        },
    },
    services::data_store::{
        atomic_file::{ApprovedLeaf, AtomicFilePublishPort, LocalAtomicFileAdapter, PublishMode},
        backup::write_security_baseline_backup_metadata,
    },
};

use super::{crypto, DeviceKeyId, DeviceKeyResolver, SecretKeyMaterial};

pub(crate) const PRE_BASELINE_SCHEMA_VERSION: i64 = schema_registry::PRE_SECRET_BASELINE_SCHEMA;
pub(crate) const ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION: i64 =
    schema_registry::ENCRYPTED_SECRET_BASELINE_SCHEMA;
pub(crate) const ENCRYPTED_SECRET_SCHEMA_PROFILE: &str = "encrypted-secrets-v1";
pub(crate) const ENCRYPTED_SECRET_FORMAT_VERSION: i64 =
    schema_registry::CURRENT_SECRET_FORMAT_VERSION;
pub(crate) const SECRET_FORMAT_VERSION_SETTING: &str = "__secret_format_version";
pub(crate) const ACTIVE_KEY_ID_SETTING: &str = "__active_key_id";
pub(crate) const LAST_SUCCESSFUL_STARTUP_VERSION_SETTING: &str =
    "__last_successful_startup_version";
const INSECURE_LOCAL_KEY_PLACEHOLDER: &str = "sk-local-pool-change-me";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BaselineConversionReport {
    pub(crate) converted: bool,
    pub(crate) backup_path: Option<PathBuf>,
}

pub(crate) fn resolver_from_parts(key_id: impl Into<String>, key: [u8; 32]) -> DeviceKeyResolver {
    DeviceKeyResolver::active(
        DeviceKeyId::new(key_id),
        SecretKeyMaterial::from_bytes(key),
        super::CURRENT_SECRET_ENCRYPTION_VERSION,
    )
}

pub(crate) fn ensure_active_database_baseline(
    default_data_dir: &Path,
    active_path: &Path,
    resolver: &DeviceKeyResolver,
) -> Result<BaselineConversionReport, String> {
    let journal_path = default_data_dir.join(UPGRADE_JOURNAL_FILE);
    if journal_path.exists() {
        return resume_or_reject_journaled_conversion(
            default_data_dir,
            active_path,
            resolver,
            &journal_path,
        );
    }
    match baseline_precondition_state(active_path)? {
        BaselinePreconditionState::EncryptedBaseline => {
            validate_baseline(active_path, resolver)?;
            Ok(BaselineConversionReport {
                converted: false,
                backup_path: None,
            })
        }
        BaselinePreconditionState::StructuralPreBaseline => {
            let journal = create_baseline_conversion_journal(&journal_path, active_path)?;
            run_journaled_conversion(default_data_dir, active_path, resolver, &journal_path, journal)
        }
        BaselinePreconditionState::Invalid { schema_version } => Err(format!(
            "encrypted-secret baseline requires compatibility schema {PRE_BASELINE_SCHEMA_VERSION} or {ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION}, found {schema_version}"
        )),
    }
}

pub(crate) fn initialize_fresh_database_at_baseline(
    path: &Path,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    if path.exists() {
        return Err("baseline database already exists".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create database parent: {error}"))?;
    }
    apply_migrations_and_finalize(path, resolver)
}

pub(crate) fn initialize_pre_baseline_runtime_for_import(
    path: &Path,
) -> Result<crate::persistence::runtime::PersistenceRuntime, String> {
    if path.exists() {
        return Err("pre-baseline staging database already exists".to_string());
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create database parent: {error}"))?;
    }
    let pool = block_on(connect_pool(path, true))?;
    block_on(async {
        migrations::migrator_through(PRE_BASELINE_SCHEMA_VERSION)
            .map_err(|error| format!("failed to prepare staging migrator: {error}"))?
            .run(&pool)
            .await
            .map_err(|error| format!("failed to run staging migrations: {error}"))?;
        pool.close().await;
        Ok::<_, String>(())
    })?;
    let compatibility = crate::persistence::schema_compatibility::BinaryCompatibility {
        app_version: semver::Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=PRE_BASELINE_SCHEMA_VERSION,
        writable_schema: std::collections::BTreeSet::from([PRE_BASELINE_SCHEMA_VERSION]),
    };
    block_on(crate::persistence::runtime::PersistenceRuntime::open(
        path,
        compatibility,
    ))
    .map_err(|error| format!("failed to open pre-baseline staging runtime: {error}"))
}

pub(crate) fn finalize_pre_baseline_database(
    path: &Path,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    apply_migrations_and_finalize(path, resolver)
}

pub(crate) fn record_successful_startup_metadata(
    path: &Path,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    block_on(async {
        let mut connection = connect_connection(path, false).await?;
        upsert_setting(
            &mut connection,
            SECRET_FORMAT_VERSION_SETTING,
            &ENCRYPTED_SECRET_FORMAT_VERSION.to_string(),
        )
        .await?;
        upsert_setting(
            &mut connection,
            ACTIVE_KEY_ID_SETTING,
            resolver.active_key_id().as_str(),
        )
        .await?;
        upsert_setting(
            &mut connection,
            LAST_SUCCESSFUL_STARTUP_VERSION_SETTING,
            env!("CARGO_PKG_VERSION"),
        )
        .await?;
        connection
            .close()
            .await
            .map_err(|error| format!("failed to close startup metadata connection: {error}"))?;
        Ok::<_, String>(())
    })
}

fn resume_or_reject_journaled_conversion(
    default_data_dir: &Path,
    active_path: &Path,
    resolver: &DeviceKeyResolver,
    journal_path: &Path,
) -> Result<BaselineConversionReport, String> {
    let observed = observe_persistence_journal(journal_path);
    match observed.kind {
        PersistenceJournalKind::BaselineConversion => {
            let journal = observed
                .baseline
                .ok_or_else(|| "baseline conversion journal is unavailable".to_string())?;
            match baseline_precondition_state(active_path)? {
                BaselinePreconditionState::EncryptedBaseline => {
                    validate_baseline(active_path, resolver)?;
                    cleanup_baseline_journal(journal_path)?;
                    Ok(BaselineConversionReport {
                        converted: false,
                        backup_path: Some(baseline_backup_path(default_data_dir, &journal)?),
                    })
                }
                BaselinePreconditionState::StructuralPreBaseline => {
                    run_journaled_conversion(default_data_dir, active_path, resolver, journal_path, journal)
                }
                BaselinePreconditionState::Invalid { schema_version } => Err(format!(
                    "baseline conversion journal is active but compatibility schema {schema_version} does not satisfy the local baseline precondition"
                )),
            }
        }
        PersistenceJournalKind::GenerationUpgrade => {
            match baseline_precondition_state(active_path)? {
                BaselinePreconditionState::EncryptedBaseline => {
                    validate_baseline(active_path, resolver)?;
                    Ok(BaselineConversionReport {
                        converted: false,
                        backup_path: None,
                    })
                }
                BaselinePreconditionState::StructuralPreBaseline => Err(
                    "generation upgrade journal is active; encrypted-secret baseline conversion will not run".to_string(),
                ),
                BaselinePreconditionState::Invalid { schema_version } => Err(format!(
                    "generation upgrade journal is active and compatibility schema {schema_version} does not satisfy the local baseline precondition"
                )),
            }
        }
        PersistenceJournalKind::Missing => unreachable!("caller checks journal existence"),
        PersistenceJournalKind::Invalid => Err(
            "persistence recovery journal is invalid; encrypted-secret baseline conversion requires manual recovery".to_string(),
        ),
    }
}

fn create_baseline_conversion_journal(
    journal_path: &Path,
    active_path: &Path,
) -> Result<BaselineConversionJournal, String> {
    let active_file_name = active_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "active database path has no valid file name".to_string())?;
    let attempt_id = UpgradeAttemptId::parse(&uuid::Uuid::now_v7().hyphenated().to_string())
        .map_err(|error| error.to_string())?;
    stabilize_sqlite_identity(active_path)?;
    let source_candidate_identity = sha256_file(active_path).map_err(redacted_journal_error)?;
    let journal = BaselineConversionJournal::prepared(
        attempt_id,
        source_candidate_identity,
        active_file_name,
        now_timestamp()?,
    )
    .map_err(|error| error.to_string())?;
    persist_baseline_journal(journal_path, &journal)?;
    Ok(journal)
}

fn run_journaled_conversion(
    default_data_dir: &Path,
    active_path: &Path,
    resolver: &DeviceKeyResolver,
    journal_path: &Path,
    mut journal: BaselineConversionJournal,
) -> Result<BaselineConversionReport, String> {
    loop {
        match journal.payload().phase {
            BaselineConversionPhase::Prepared => {
                journal = execute_baseline_prepared(
                    default_data_dir,
                    active_path,
                    journal_path,
                    &journal,
                )?;
            }
            BaselineConversionPhase::BackupVerified => {
                journal = execute_baseline_backup_verified(
                    default_data_dir,
                    active_path,
                    resolver,
                    journal_path,
                    &journal,
                )?;
            }
            BaselineConversionPhase::CandidateBuilt => {
                journal = execute_baseline_candidate_built(
                    active_path,
                    resolver,
                    journal_path,
                    &journal,
                )?;
            }
            BaselineConversionPhase::CandidateValidated => {
                journal =
                    execute_baseline_candidate_validated(active_path, journal_path, &journal)?;
            }
            BaselineConversionPhase::ActivePublished => {
                journal = execute_baseline_active_published(
                    active_path,
                    resolver,
                    journal_path,
                    &journal,
                )?;
            }
            BaselineConversionPhase::ActiveValidated => {
                cleanup_baseline_journal(journal_path)?;
                return Ok(BaselineConversionReport {
                    converted: true,
                    backup_path: Some(baseline_backup_path(default_data_dir, &journal)?),
                });
            }
        }
    }
}

fn execute_baseline_prepared(
    default_data_dir: &Path,
    active_path: &Path,
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<BaselineConversionJournal, String> {
    assert_active_identity(active_path, journal)?;
    let backup_path = baseline_backup_path(default_data_dir, journal)?;
    remove_database_artifacts(&backup_path)?;
    create_security_baseline_backup(&backup_path, active_path)?;
    write_security_baseline_backup_metadata(
        &backup_path,
        ENCRYPTED_SECRET_SCHEMA_PROFILE,
        "legacy-local-only",
    )?;
    block_on(persistence::validate_read_only_sqlite(&backup_path))
        .map_err(|error| format!("failed to verify encrypted-secret baseline backup: {error}"))?;
    let backup_sha = sha256_file(&backup_path).map_err(redacted_journal_error)?;
    let next = journal
        .advance(
            BaselineConversionPhase::BackupVerified,
            Some(backup_sha),
            None,
            now_timestamp()?,
        )
        .map_err(|error| error.to_string())?;
    persist_baseline_journal(journal_path, &next)?;
    Ok(next)
}

fn execute_baseline_backup_verified(
    default_data_dir: &Path,
    active_path: &Path,
    resolver: &DeviceKeyResolver,
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<BaselineConversionJournal, String> {
    assert_backup_identity(&baseline_backup_path(default_data_dir, journal)?, journal)?;
    let candidate_path = baseline_candidate_path(active_path, journal)?;
    remove_database_artifacts(&candidate_path)?;
    copy_sqlite_database(
        &baseline_backup_path(default_data_dir, journal)?,
        &candidate_path,
    )?;
    apply_migrations_and_finalize(&candidate_path, resolver)?;
    stabilize_sqlite_identity(&candidate_path)?;
    let candidate_sha = sha256_file(&candidate_path).map_err(redacted_journal_error)?;
    let next = journal
        .advance(
            BaselineConversionPhase::CandidateBuilt,
            None,
            Some(candidate_sha),
            now_timestamp()?,
        )
        .map_err(|error| error.to_string())?;
    persist_baseline_journal(journal_path, &next)?;
    Ok(next)
}

fn execute_baseline_candidate_built(
    active_path: &Path,
    resolver: &DeviceKeyResolver,
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<BaselineConversionJournal, String> {
    let candidate_path = baseline_candidate_path(active_path, journal)?;
    assert_candidate_identity(&candidate_path, journal)?;
    validate_baseline(&candidate_path, resolver)?;
    let next = journal
        .advance(
            BaselineConversionPhase::CandidateValidated,
            None,
            None,
            now_timestamp()?,
        )
        .map_err(|error| error.to_string())?;
    persist_baseline_journal(journal_path, &next)?;
    Ok(next)
}

fn execute_baseline_candidate_validated(
    active_path: &Path,
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<BaselineConversionJournal, String> {
    let candidate_path = baseline_candidate_path(active_path, journal)?;
    if candidate_path.is_file() {
        assert_candidate_identity(&candidate_path, journal)?;
        stabilize_sqlite_identity(active_path)?;
        stabilize_sqlite_identity(&candidate_path)?;
        publish_prepared_database(&candidate_path, active_path)?;
        stabilize_sqlite_identity(active_path)?;
    } else if !matches!(
        baseline_precondition_state(active_path)?,
        BaselinePreconditionState::EncryptedBaseline
    ) {
        return Err("validated encrypted-secret baseline candidate is missing".to_string());
    }
    let next = journal
        .advance(
            BaselineConversionPhase::ActivePublished,
            None,
            None,
            now_timestamp()?,
        )
        .map_err(|error| error.to_string())?;
    persist_baseline_journal(journal_path, &next)?;
    Ok(next)
}

fn execute_baseline_active_published(
    active_path: &Path,
    resolver: &DeviceKeyResolver,
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<BaselineConversionJournal, String> {
    validate_baseline(active_path, resolver)?;
    let next = journal
        .advance(
            BaselineConversionPhase::ActiveValidated,
            None,
            None,
            now_timestamp()?,
        )
        .map_err(|error| error.to_string())?;
    persist_baseline_journal(journal_path, &next)?;
    Ok(next)
}

fn apply_migrations_and_finalize(path: &Path, resolver: &DeviceKeyResolver) -> Result<(), String> {
    let pool = block_on(connect_pool(path, true))?;
    let result = block_on(async {
        if !column_exists(&pool, "secrets", "key_id").await? {
            migrations::migrator_through(ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION)
                .map_err(|error| format!("failed to prepare baseline migrator: {error}"))?
                .run(&pool)
                .await
                .map_err(|error| format!("failed to run baseline migrations: {error}"))?;
        }
        finalize_open_database(&pool, resolver).await?;
        pool.close().await;
        Ok::<_, String>(())
    });
    result
}

async fn finalize_open_database(
    pool: &SqlitePool,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    let mut transaction = pool
        .begin()
        .await
        .map_err(|error| format!("failed to start baseline conversion: {error}"))?;
    detect_plaintext_secret_conflicts(&mut transaction).await?;
    reencrypt_existing_secrets(&mut transaction, resolver).await?;
    repair_legacy_settings_in_connection(&mut transaction)
        .await
        .map_err(|error| format!("failed to repair legacy settings during baseline: {error}"))?;
    migrate_legacy_plaintext_columns(&mut transaction, resolver).await?;
    rebuild_secrets_with_final_constraints(&mut transaction).await?;
    sqlx::query(
        r#"
        UPDATE persistence_schema_compatibility
        SET schema_version = ?1,
            min_reader_app_version = '0.3.1',
            min_writer_app_version = '0.3.1',
            updated_by_migration = ?1,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE singleton_key = 1
        "#,
    )
    .bind(ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("failed to commit baseline schema profile: {error}"))?;
    let (migration_description, migration_checksum) = baseline_migration_metadata()?;
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO _sqlx_migrations (
            version, description, installed_on, success, checksum, execution_time
        ) VALUES (?1, ?2, CURRENT_TIMESTAMP, 1, ?3, 0)
        "#,
    )
    .bind(ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION)
    .bind(migration_description)
    .bind(migration_checksum)
    .execute(&mut *transaction)
    .await
    .map_err(|error| format!("failed to record baseline migration metadata: {error}"))?;
    record_secret_format_metadata(&mut transaction, resolver).await?;
    transaction
        .commit()
        .await
        .map_err(|error| format!("failed to commit baseline conversion: {error}"))?;
    sqlx::query("VACUUM")
        .execute(pool)
        .await
        .map_err(|error| format!("failed to vacuum baseline database: {error}"))?;
    Ok(())
}

async fn record_secret_format_metadata(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    upsert_setting_in_transaction(
        transaction,
        SECRET_FORMAT_VERSION_SETTING,
        &ENCRYPTED_SECRET_FORMAT_VERSION.to_string(),
    )
    .await?;
    upsert_setting_in_transaction(
        transaction,
        ACTIVE_KEY_ID_SETTING,
        resolver.active_key_id().as_str(),
    )
    .await
}

async fn upsert_setting_in_transaction(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    key: &str,
    value: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?1, ?2, strftime('%s', 'now'))
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(&mut **transaction)
    .await
    .map_err(|error| format!("failed to record startup metadata {key}: {error}"))?;
    Ok(())
}

async fn upsert_setting(
    connection: &mut SqliteConnection,
    key: &str,
    value: &str,
) -> Result<(), String> {
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?1, ?2, strftime('%s', 'now'))
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .execute(connection)
    .await
    .map_err(|error| format!("failed to record startup metadata {key}: {error}"))?;
    Ok(())
}

async fn reencrypt_existing_secrets(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    let rows = sqlx::query(
        r#"
        SELECT id, scope, owner_id, kind, masked_value, ciphertext, nonce,
               key_id, encryption_version
        FROM secrets
        ORDER BY id
        "#,
    )
    .fetch_all(&mut **connection)
    .await
    .map_err(|error| format!("failed to load secrets for baseline conversion: {error}"))?;
    for row in rows {
        let id: String = row.get("id");
        let scope: String = row.get("scope");
        let owner_id: String = row.get("owner_id");
        let kind: String = row.get("kind");
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        let key_id: Option<String> = row.get("key_id");
        let encryption_version: Option<i64> = row.get("encryption_version");
        if key_id.is_some() || encryption_version.is_some() {
            validate_secret_row(&id, &scope, &owner_id, &kind, &ciphertext, &nonce, resolver)?;
            continue;
        }
        let legacy_aad = format!("{scope}:{owner_id}:{kind}");
        let plaintext = resolver
            .with_active_key(|key| {
                crypto::decrypt_secret(
                    key,
                    &crypto::EncryptedPayload {
                        ciphertext: general_purpose::STANDARD.encode(&ciphertext),
                        nonce: general_purpose::STANDARD.encode(&nonce),
                        aad: legacy_aad,
                        value_hash: String::new(),
                    },
                )
            })
            .map_err(|_| "failed to access active key during baseline conversion".to_string())?
            .map_err(|_| format!("secret baseline conversion failed for row {}", safe_id(&id)))?;
        update_secret_ciphertext(
            connection, &id, &scope, &owner_id, &kind, &plaintext, resolver,
        )
        .await?;
    }
    Ok(())
}

async fn migrate_legacy_plaintext_columns(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    migrate_plaintext_reference_column(
        connection,
        "stations",
        "id",
        "api_key",
        "api_key_secret_id",
        "station",
        "api_key",
        resolver,
    )
    .await?;
    migrate_plaintext_reference_column(
        connection,
        "station_keys",
        "id",
        "api_key",
        "api_key_secret_id",
        "station_key",
        "api_key",
        resolver,
    )
    .await?;
    migrate_plaintext_reference_column(
        connection,
        "station_credentials",
        "station_id",
        "login_password",
        "login_password_secret_id",
        "station_credentials",
        "login_password",
        resolver,
    )
    .await?;
    migrate_local_access_key(connection, resolver).await
}

async fn detect_plaintext_secret_conflicts(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<(), String> {
    for (table, owner_column, plaintext_column, secret_column, scope, kind) in [
        (
            "stations",
            "id",
            "api_key",
            "api_key_secret_id",
            "station",
            "api_key",
        ),
        (
            "station_keys",
            "id",
            "api_key",
            "api_key_secret_id",
            "station_key",
            "api_key",
        ),
        (
            "station_credentials",
            "station_id",
            "login_password",
            "login_password_secret_id",
            "station_credentials",
            "login_password",
        ),
    ] {
        let query = format!(
            "SELECT {owner_column} AS owner_id, {secret_column} AS secret_id FROM {table} WHERE TRIM(COALESCE({plaintext_column}, '')) <> ''"
        );
        let rows = sqlx::query(&query)
            .fetch_all(&mut **connection)
            .await
            .map_err(|error| format!("failed to preflight legacy plaintext conflicts: {error}"))?;
        for row in rows {
            let owner_id: String = row.get("owner_id");
            let secret_id: Option<String> = row.get("secret_id");
            if secret_id.is_some() || secret_exists(connection, scope, &owner_id, kind).await? {
                return Err(format!(
                    "legacy plaintext conflicts with an existing secret for {scope}:{kind}:{}",
                    safe_id(&owner_id)
                ));
            }
        }
    }
    if let Some(value) = sqlx::query_scalar::<_, Option<String>>(
        "SELECT value FROM settings WHERE key = 'local_key'",
    )
    .fetch_optional(&mut **connection)
    .await
    .map_err(|error| format!("failed to preflight local access key conflict: {error}"))?
    .flatten()
    {
        if !value.trim().is_empty()
            && (local_access_key_binding_exists(connection).await?
                || secret_exists(connection, "settings", "local_key", "local_access_key").await?)
        {
            return Err(
                "legacy local access key conflicts with an existing secret binding".to_string(),
            );
        }
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn migrate_plaintext_reference_column(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    table: &str,
    owner_column: &str,
    plaintext_column: &str,
    secret_column: &str,
    scope: &str,
    kind: &str,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    let query = format!(
        "SELECT {owner_column} AS owner_id, {plaintext_column} AS plaintext, {secret_column} AS secret_id FROM {table} WHERE TRIM(COALESCE({plaintext_column}, '')) <> ''"
    );
    let rows = sqlx::query(&query)
        .fetch_all(&mut **connection)
        .await
        .map_err(|error| {
            format!("failed to load legacy plaintext column {table}.{plaintext_column}: {error}")
        })?;
    for row in rows {
        let owner_id: String = row.get("owner_id");
        let plaintext: String = row.get("plaintext");
        let secret_id: Option<String> = row.get("secret_id");
        if secret_id.is_some() || secret_exists(connection, scope, &owner_id, kind).await? {
            return Err(format!(
                "legacy plaintext conflicts with an existing secret for {scope}:{kind}:{}",
                safe_id(&owner_id)
            ));
        }
        let inserted_secret_id =
            insert_converted_secret(connection, scope, &owner_id, kind, &plaintext, resolver)
                .await?;
        let update = format!(
            "UPDATE {table} SET {secret_column} = ?1, {plaintext_column} = '' WHERE {owner_column} = ?2"
        );
        sqlx::query(&update)
            .bind(inserted_secret_id)
            .bind(owner_id)
            .execute(&mut **connection)
            .await
            .map_err(|error| {
                format!(
                    "failed to clear legacy plaintext column {table}.{plaintext_column}: {error}"
                )
            })?;
    }
    Ok(())
}

async fn migrate_local_access_key(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    let row = sqlx::query("SELECT value FROM settings WHERE key = 'local_key'")
        .fetch_optional(&mut **connection)
        .await
        .map_err(|error| format!("failed to load legacy local access key: {error}"))?;
    let Some(row) = row else {
        return Ok(());
    };
    let plaintext: String = row.get("value");
    let plaintext = if plaintext.trim().is_empty() || plaintext == INSECURE_LOCAL_KEY_PLACEHOLDER {
        generate_local_access_key()
    } else {
        plaintext
    };
    if local_access_key_binding_exists(connection).await?
        || secret_exists(connection, "settings", "local_key", "local_access_key").await?
    {
        return Err(
            "legacy local access key conflicts with an existing secret binding".to_string(),
        );
    }
    let secret_id = insert_converted_secret(
        connection,
        "settings",
        "local_key",
        "local_access_key",
        &plaintext,
        resolver,
    )
    .await?;
    sqlx::query(
        r#"
        INSERT INTO app_secret_bindings (
            binding_scope, binding_owner_id, binding_kind, secret_id, created_at, updated_at
        ) VALUES ('settings', 'local_key', 'local_access_key', ?1, strftime('%s', 'now'), strftime('%s', 'now'))
        "#,
    )
    .bind(secret_id)
    .execute(&mut **connection)
    .await
    .map_err(|error| format!("failed to bind local access key secret: {error}"))?;
    sqlx::query("UPDATE settings SET value = '', updated_at = strftime('%s', 'now') WHERE key = 'local_key'")
        .execute(&mut **connection)
        .await
        .map_err(|error| format!("failed to clear legacy local access key: {error}"))?;
    Ok(())
}

fn generate_local_access_key() -> String {
    let mut random = [0_u8; 32];
    OsRng.fill_bytes(&mut random);
    format!("sk-local-{}", URL_SAFE_NO_PAD.encode(random))
}

async fn insert_converted_secret(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    scope: &str,
    owner_id: &str,
    kind: &str,
    plaintext: &str,
    resolver: &DeviceKeyResolver,
) -> Result<String, String> {
    let id = uuid::Uuid::now_v7().to_string();
    let aad = canonical_secret_aad(scope, owner_id, kind, resolver.encryption_version());
    let payload = resolver
        .with_active_key(|key| crypto::encrypt_secret(key, plaintext, &aad))
        .map_err(|_| "failed to access active key during secret insert".to_string())?
        .map_err(|_| "failed to encrypt converted secret".to_string())?;
    sqlx::query(
        r#"
        INSERT INTO secrets (
            id, scope, owner_id, kind, masked_value, ciphertext, nonce,
            key_id, encryption_version, value_hash, created_at, updated_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, strftime('%s', 'now'), strftime('%s', 'now'))
        "#,
    )
    .bind(&id)
    .bind(scope)
    .bind(owner_id)
    .bind(kind)
    .bind(mask_secret(plaintext))
    .bind(decode_base64(&payload.ciphertext)?)
    .bind(decode_base64(&payload.nonce)?)
    .bind(resolver.active_key_id().as_str())
    .bind(i64::from(resolver.encryption_version()))
    .bind(payload.value_hash)
    .execute(&mut **connection)
    .await
    .map_err(|error| format!("failed to insert converted secret: {error}"))?;
    Ok(id)
}

async fn update_secret_ciphertext(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
    scope: &str,
    owner_id: &str,
    kind: &str,
    plaintext: &str,
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    let aad = canonical_secret_aad(scope, owner_id, kind, resolver.encryption_version());
    let payload = resolver
        .with_active_key(|key| crypto::encrypt_secret(key, plaintext, &aad))
        .map_err(|_| "failed to access active key during secret update".to_string())?
        .map_err(|_| "failed to re-encrypt secret".to_string())?;
    sqlx::query(
        r#"
        UPDATE secrets
        SET ciphertext = ?1,
            nonce = ?2,
            key_id = ?3,
            encryption_version = ?4,
            value_hash = ?5,
            updated_at = strftime('%s', 'now')
        WHERE id = ?6
        "#,
    )
    .bind(decode_base64(&payload.ciphertext)?)
    .bind(decode_base64(&payload.nonce)?)
    .bind(resolver.active_key_id().as_str())
    .bind(i64::from(resolver.encryption_version()))
    .bind(payload.value_hash)
    .bind(id)
    .execute(&mut **connection)
    .await
    .map_err(|error| format!("failed to update converted secret: {error}"))?;
    Ok(())
}

async fn rebuild_secrets_with_final_constraints(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<(), String> {
    let missing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM secrets WHERE key_id IS NULL OR encryption_version IS NULL OR value_hash IS NULL",
    )
    .fetch_one(&mut **connection)
    .await
    .map_err(|error| format!("failed to verify final secret metadata: {error}"))?;
    if missing != 0 {
        return Err("baseline conversion left secrets without key metadata".to_string());
    }
    let bindings = sqlx::query_as::<_, (String, String, String, String, String, String)>(
        r#"
        SELECT binding_scope, binding_owner_id, binding_kind, secret_id, created_at, updated_at
        FROM app_secret_bindings
        ORDER BY binding_scope, binding_owner_id, binding_kind
        "#,
    )
    .fetch_all(&mut **connection)
    .await
    .map_err(|error| format!("failed to preserve secret bindings: {error}"))?;
    sqlx::query("DROP TABLE app_secret_bindings")
        .execute(&mut **connection)
        .await
        .map_err(|error| format!("failed to detach secret bindings for rebuild: {error}"))?;
    sqlx::query(
        r#"
        CREATE TABLE secrets_final (
            id TEXT PRIMARY KEY,
            scope TEXT NOT NULL,
            owner_id TEXT NOT NULL,
            kind TEXT NOT NULL,
            masked_value TEXT NOT NULL,
            ciphertext BLOB NOT NULL,
            nonce BLOB NOT NULL,
            key_id TEXT NOT NULL,
            encryption_version INTEGER NOT NULL,
            value_hash TEXT NOT NULL,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            UNIQUE(scope, owner_id, kind)
        )
        "#,
    )
    .execute(&mut **connection)
    .await
    .map_err(|error| format!("failed to create final secrets table: {error}"))?;
    sqlx::query(
        r#"
        INSERT INTO secrets_final (
            id, scope, owner_id, kind, masked_value, ciphertext, nonce,
            key_id, encryption_version, value_hash, created_at, updated_at
        )
        SELECT id, scope, owner_id, kind, masked_value, ciphertext, nonce,
               key_id, encryption_version, value_hash, created_at, updated_at
        FROM secrets
        "#,
    )
    .execute(&mut **connection)
    .await
    .map_err(|error| format!("failed to copy final secrets rows: {error}"))?;
    sqlx::query("DROP TABLE secrets")
        .execute(&mut **connection)
        .await
        .map_err(|error| format!("failed to drop transitional secrets table: {error}"))?;
    sqlx::query("ALTER TABLE secrets_final RENAME TO secrets")
        .execute(&mut **connection)
        .await
        .map_err(|error| format!("failed to activate final secrets table: {error}"))?;
    sqlx::query(
        r#"
        CREATE TABLE app_secret_bindings (
            binding_scope TEXT NOT NULL,
            binding_owner_id TEXT NOT NULL,
            binding_kind TEXT NOT NULL,
            secret_id TEXT NOT NULL REFERENCES secrets(id) ON DELETE CASCADE,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL,
            PRIMARY KEY (binding_scope, binding_owner_id, binding_kind),
            UNIQUE(secret_id)
        )
        "#,
    )
    .execute(&mut **connection)
    .await
    .map_err(|error| format!("failed to recreate secret bindings: {error}"))?;
    sqlx::query("CREATE INDEX idx_app_secret_bindings_secret_id ON app_secret_bindings(secret_id)")
        .execute(&mut **connection)
        .await
        .map_err(|error| format!("failed to recreate secret binding index: {error}"))?;
    for (binding_scope, binding_owner_id, binding_kind, secret_id, created_at, updated_at) in
        bindings
    {
        sqlx::query(
            r#"
            INSERT INTO app_secret_bindings (
                binding_scope, binding_owner_id, binding_kind, secret_id, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
            "#,
        )
        .bind(binding_scope)
        .bind(binding_owner_id)
        .bind(binding_kind)
        .bind(secret_id)
        .bind(created_at)
        .bind(updated_at)
        .execute(&mut **connection)
        .await
        .map_err(|error| format!("failed to restore secret binding: {error}"))?;
    }
    Ok(())
}

fn validate_baseline(path: &Path, resolver: &DeviceKeyResolver) -> Result<(), String> {
    let rows = block_on(async {
        let mut connection = connect_connection(path, false).await?;
        let rows = sqlx::query(
            r#"
            SELECT id, scope, owner_id, kind, ciphertext, nonce, key_id, encryption_version
            FROM secrets
            ORDER BY id
            "#,
        )
        .fetch_all(&mut connection)
        .await
        .map_err(|error| format!("failed to read baseline secrets: {error}"))?;
        connection
            .close()
            .await
            .map_err(|error| format!("failed to close baseline validation connection: {error}"))?;
        Ok::<_, String>(rows)
    })?;
    for row in rows {
        let id: String = row.get("id");
        validate_secret_row(
            &id,
            row.get("scope"),
            row.get("owner_id"),
            row.get("kind"),
            row.get("ciphertext"),
            row.get("nonce"),
            resolver,
        )?;
    }
    block_on(persistence::validate_read_only_sqlite(path))
        .map_err(|error| format!("baseline sqlite validation failed: {error}"))?;
    Ok(())
}

fn validate_secret_row(
    id: &str,
    scope: &str,
    owner_id: &str,
    kind: &str,
    ciphertext: &[u8],
    nonce: &[u8],
    resolver: &DeviceKeyResolver,
) -> Result<(), String> {
    let aad = canonical_secret_aad(scope, owner_id, kind, resolver.encryption_version());
    resolver
        .with_key(
            resolver.active_key_id().as_str(),
            resolver.encryption_version(),
            |key| {
                crypto::decrypt_secret(
                    key,
                    &crypto::EncryptedPayload {
                        ciphertext: general_purpose::STANDARD.encode(ciphertext),
                        nonce: general_purpose::STANDARD.encode(nonce),
                        aad,
                        value_hash: String::new(),
                    },
                )
            },
        )
        .map_err(|_| "failed to access active key during baseline validation".to_string())?
        .map(|_| ())
        .map_err(|_| format!("baseline secret validation failed for row {}", safe_id(id)))
}

async fn secret_exists(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    scope: &str,
    owner_id: &str,
    kind: &str,
) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM secrets WHERE scope = ?1 AND owner_id = ?2 AND kind = ?3)",
    )
    .bind(scope)
    .bind(owner_id)
    .bind(kind)
    .fetch_one(&mut **connection)
    .await
    .map(|value| value == 1)
    .map_err(|error| format!("failed to inspect secret conflict: {error}"))
}

async fn local_access_key_binding_exists(
    connection: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
) -> Result<bool, String> {
    sqlx::query_scalar::<_, i64>(
        r#"
        SELECT EXISTS(
            SELECT 1 FROM app_secret_bindings
            WHERE binding_scope = 'settings'
              AND binding_owner_id = 'local_key'
              AND binding_kind = 'local_access_key'
        )
        "#,
    )
    .fetch_one(&mut **connection)
    .await
    .map(|value| value == 1)
    .map_err(|error| format!("failed to inspect local access key binding: {error}"))
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> Result<bool, String> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(pool)
        .await
        .map_err(|error| format!("failed to inspect table columns: {error}"))?;
    Ok(rows
        .into_iter()
        .any(|row| row.get::<String, _>("name") == column))
}

fn baseline_precondition_state(path: &Path) -> Result<BaselinePreconditionState, String> {
    block_on(async {
        let mut connection = connect_connection(path, false).await?;
        let row = sqlx::query(
            "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
        )
        .fetch_one(&mut connection)
        .await
        .map_err(|error| format!("failed to read schema compatibility: {error}"))?;
        let schema_version: i64 = row.get("schema_version");
        connection
            .close()
            .await
            .map_err(|error| format!("failed to close schema inspection connection: {error}"))?;
        Ok::<_, String>(match schema_version {
            ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION => {
                BaselinePreconditionState::EncryptedBaseline
            }
            PRE_BASELINE_SCHEMA_VERSION => BaselinePreconditionState::StructuralPreBaseline,
            other => BaselinePreconditionState::Invalid {
                schema_version: other,
            },
        })
    })
}

enum BaselinePreconditionState {
    StructuralPreBaseline,
    EncryptedBaseline,
    Invalid { schema_version: i64 },
}

fn baseline_migration_metadata() -> Result<(String, Vec<u8>), String> {
    let migration = migrations::migrator()
        .iter()
        .find(|migration| migration.version == ENCRYPTED_SECRET_BASELINE_SCHEMA_VERSION)
        .ok_or_else(|| "encrypted-secret baseline migration metadata is unavailable".to_string())?;
    Ok((
        migration.description.to_string(),
        migration.checksum.as_ref().to_vec(),
    ))
}

fn create_security_baseline_backup(backup_path: &Path, active_path: &Path) -> Result<(), String> {
    if let Some(parent) = backup_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create baseline backup directory: {error}"))?;
    }
    block_on(persistence::create_verified_backup_from_path(
        active_path,
        backup_path,
    ))
    .map_err(|error| format!("failed to create baseline backup: {error}"))?;
    Ok(())
}

fn copy_sqlite_database(source: &Path, target: &Path) -> Result<(), String> {
    if target.exists() {
        return Err("baseline staging database already exists".to_string());
    }
    block_on(persistence::create_verified_backup_from_path(
        source, target,
    ))
    .map(|_| ())
    .map_err(|error| format!("failed to create baseline staging database: {error}"))
}

fn publish_prepared_database(prepared: &Path, active_path: &Path) -> Result<(), String> {
    let parent = active_path
        .parent()
        .ok_or_else(|| "active database path has no parent".to_string())?
        .canonicalize()
        .map_err(|error| format!("failed to canonicalize active database parent: {error}"))?;
    let leaf = active_path
        .file_name()
        .ok_or_else(|| "active database path has no file name".to_string())?;
    let approved = ApprovedLeaf::approve(&parent, leaf)
        .map_err(|error| format!("failed to approve baseline publish path: {error}"))?;
    LocalAtomicFileAdapter
        .publish(prepared, &approved, PublishMode::ReplaceExisting)
        .map_err(|error| format!("failed to publish baseline database atomically: {error}"))?;
    Ok(())
}

fn remove_sqlite_sidecars(path: &Path) -> Result<(), String> {
    for sidecar in [wal_path(path), shm_path(path)] {
        remove_file_with_bounded_retry(&sidecar).map_err(|error| {
            format!(
                "failed to remove stale sqlite sidecar {}: {error}",
                sidecar.display()
            )
        })?;
    }
    Ok(())
}

fn remove_database_artifacts(path: &Path) -> Result<(), String> {
    for artifact in [path.to_path_buf(), wal_path(path), shm_path(path)] {
        remove_file_with_bounded_retry(&artifact).map_err(|error| {
            format!(
                "failed to remove encrypted-secret baseline artifact {}: {error}",
                artifact.display()
            )
        })?;
    }
    Ok(())
}

fn stabilize_sqlite_identity(path: &Path) -> Result<(), String> {
    if !path.exists() {
        return Err("baseline database is missing".to_string());
    }
    block_on(async {
        let mut connection = connect_connection(path, false).await?;
        let row = sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .fetch_one(&mut connection)
            .await
            .map_err(|error| format!("failed to checkpoint baseline database WAL: {error}"))?;
        let busy: i64 = row.get(0);
        connection.close().await.map_err(|error| {
            format!("failed to close baseline database after checkpoint: {error}")
        })?;
        if busy != 0 {
            return Err("baseline database WAL checkpoint reported busy readers".to_string());
        }
        Ok::<_, String>(())
    })?;
    remove_sqlite_sidecars(path)
}

fn remove_file_with_bounded_retry(path: &Path) -> Result<(), std::io::Error> {
    // Windows can keep SQLite WAL/SHM handles busy briefly after SQLx closes the pool.
    // Full-suite runs on Windows can delay handle release well past the SQLite busy timeout,
    // so keep the retry bounded but long enough to absorb transient WAL/SHM locks.
    const ATTEMPTS: usize = 800;
    const RETRY_DELAY: Duration = Duration::from_millis(25);
    for attempt in 0..ATTEMPTS {
        match fs::remove_file(path) {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) if attempt + 1 < ATTEMPTS && is_transient_file_lock(&error) =>
            {
                std::thread::sleep(RETRY_DELAY);
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn is_transient_file_lock(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::PermissionDenied
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::Other
    ) || matches!(
        error.raw_os_error(),
        // ERROR_SHARING_VIOLATION and ERROR_LOCK_VIOLATION. On Windows these can be
        // returned after SQLite closes while the WAL/SHM handle is still being released.
        Some(32 | 33)
    )
}

fn baseline_backup_path(
    default_data_dir: &Path,
    journal: &BaselineConversionJournal,
) -> Result<PathBuf, String> {
    Ok(default_data_dir.join(journal.payload().paths.backup()))
}

fn baseline_candidate_path(
    active_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<PathBuf, String> {
    let active_file_name = active_path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "active database path has no valid file name".to_string())?;
    if journal.payload().paths.active() != active_file_name {
        return Err("baseline conversion journal does not target the active database".to_string());
    }
    let parent = active_path
        .parent()
        .ok_or_else(|| "active database path has no parent".to_string())?;
    Ok(parent.join(journal.payload().paths.candidate()))
}

fn assert_active_identity(
    active_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<(), String> {
    stabilize_sqlite_identity(active_path)?;
    let actual = sha256_file(active_path).map_err(redacted_journal_error)?;
    if actual != journal.payload().source_candidate_identity {
        return Err(
            "encrypted-secret baseline source database changed during conversion".to_string(),
        );
    }
    Ok(())
}

fn assert_backup_identity(
    backup_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<(), String> {
    let actual = sha256_file(backup_path).map_err(redacted_journal_error)?;
    if Some(&actual) != journal.payload().verified_backup_sha256.as_ref() {
        return Err("encrypted-secret baseline backup identity mismatch".to_string());
    }
    Ok(())
}

fn assert_candidate_identity(
    candidate_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<(), String> {
    stabilize_sqlite_identity(candidate_path)?;
    let actual = sha256_file(candidate_path).map_err(redacted_journal_error)?;
    if Some(&actual) != journal.payload().candidate_sha256.as_ref() {
        return Err("encrypted-secret baseline candidate identity mismatch".to_string());
    }
    Ok(())
}

fn persist_baseline_journal(
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<(), String> {
    if let Some(parent) = journal_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("failed to create baseline journal parent: {error}"))?;
    }
    write_baseline_conversion_journal_atomically(journal_path, journal)
        .map_err(redacted_journal_error)
}

fn cleanup_baseline_journal(journal_path: &Path) -> Result<(), String> {
    remove_file_and_sync_parent_with_faults(journal_path, &NoUpgradeFaults)
        .map_err(redacted_journal_error)
}

fn wal_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", path.display()))
}

fn shm_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", path.display()))
}

fn now_timestamp() -> Result<UtcTimestamp, String> {
    UtcTimestamp::parse(&Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true))
        .map_err(|error| error.to_string())
}

fn redacted_journal_error(error: impl std::fmt::Debug) -> String {
    format!("encrypted-secret baseline recovery I/O failed: {error:?}")
}

async fn connect_pool(path: &Path, create_if_missing: bool) -> Result<SqlitePool, String> {
    Ok(SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(connect_options(path, create_if_missing))
        .await
        .map_err(|error| format!("failed to connect baseline database: {error}"))?)
}

async fn connect_connection(
    path: &Path,
    create_if_missing: bool,
) -> Result<SqliteConnection, String> {
    SqliteConnection::connect_with(&connect_options(path, create_if_missing))
        .await
        .map_err(|error| format!("failed to connect baseline database: {error}"))
}

fn connect_options(path: &Path, create_if_missing: bool) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create_if_missing)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
}

fn decode_base64(value: &str) -> Result<Vec<u8>, String> {
    general_purpose::STANDARD
        .decode(value)
        .map_err(|_| "failed to decode encrypted payload".to_string())
}

fn safe_id(value: &str) -> String {
    let suffix: String = value
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("***{suffix}")
}

fn block_on<T>(future: impl std::future::Future<Output = T>) -> T {
    tauri::async_runtime::block_on(future)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Row;

    #[test]
    fn baseline_conversion_rekeys_legacy_plaintext_and_local_access_key() {
        let root = tempfile::tempdir().expect("tempdir");
        let app_data = root.path().join("app-data");
        let active = root.path().join("relay-pool-desktop-v2.sqlite3");
        let resolver = resolver_from_parts("device-key-v1", [42; 32]);
        let runtime = initialize_pre_baseline_runtime_for_import(&active).expect("prebaseline db");
        block_on(runtime.write(|write| {
            Box::pin(async move {
                sqlx::query(
                    r#"
                    INSERT INTO stations (
                        id, name, station_type, website_url, api_base_url, api_key,
                        created_at, updated_at
                    ) VALUES (
                        'station-1', 'Station', 'newapi', 'https://example.test',
                        'https://example.test/v1', 'sk-station-plaintext-canary', '1', '1'
                    )
                    "#,
                )
                .execute(write.connection())
                .await?;
                sqlx::query(
                    r#"
                    INSERT INTO station_credentials (
                        station_id, login_password, remember_password, updated_at
                    ) VALUES ('station-1', 'password-plaintext-canary', 1, '1')
                    "#,
                )
                .execute(write.connection())
                .await?;
                sqlx::query(
                    "UPDATE settings SET value = 'sk-local-plaintext-canary' WHERE key = 'local_key'",
                )
                .execute(write.connection())
                .await?;
                sqlx::query(
                    "UPDATE settings SET value = 'minimize-to-tray' WHERE key = 'tray_behavior'",
                )
                .execute(write.connection())
                .await?;
                Ok(())
            })
        }))
        .expect("seed legacy plaintext");
        block_on(runtime.close()).expect("close runtime");
        drop(runtime);

        let report =
            ensure_active_database_baseline(&app_data, &active, &resolver).expect("convert");

        assert!(report.converted);
        assert!(report
            .backup_path
            .as_ref()
            .is_some_and(|path| path.is_file()));
        assert_eq!(
            query_i64(
                &active,
                "SELECT schema_version FROM persistence_schema_compatibility"
            ),
            17
        );
        assert_eq!(query_i64(&active, "SELECT COUNT(*) FROM secrets WHERE key_id = 'device-key-v1' AND encryption_version = 1"), 3);
        assert_eq!(
            query_string(
                &active,
                "SELECT api_key FROM stations WHERE id = 'station-1'"
            ),
            ""
        );
        assert_eq!(
            query_string(
                &active,
                "SELECT login_password FROM station_credentials WHERE station_id = 'station-1'"
            ),
            ""
        );
        assert_eq!(
            query_string(
                &active,
                "SELECT value FROM settings WHERE key = 'local_key'"
            ),
            ""
        );
        assert_eq!(
            query_string(
                &active,
                "SELECT value FROM settings WHERE key = 'tray_behavior'"
            ),
            "minimize_to_tray"
        );
        assert_eq!(query_i64(&active, "SELECT COUNT(*) FROM app_secret_bindings WHERE binding_scope = 'settings' AND binding_owner_id = 'local_key'"), 1);
        assert!(
            !database_bytes(&active).contains("sk-station-plaintext-canary")
                && !database_bytes(&active).contains("password-plaintext-canary")
                && !database_bytes(&active).contains("sk-local-plaintext-canary")
        );
        let key = resolver.with_active_key(|key| *key).expect("active key");
        crate::services::secrets::validation::validate_database_secrets(&active, &key)
            .expect("validate converted secrets");
    }

    #[test]
    fn baseline_conversion_replaces_fresh_local_access_key_placeholder() {
        let root = tempfile::tempdir().expect("tempdir");
        let app_data = root.path().join("app-data");
        let active = root.path().join("relay-pool-desktop-v2.sqlite3");
        let resolver = resolver_from_parts("device-key-v1", [55; 32]);
        let runtime = initialize_pre_baseline_runtime_for_import(&active).expect("prebaseline db");
        block_on(runtime.close()).expect("close runtime");
        drop(runtime);

        ensure_active_database_baseline(&app_data, &active, &resolver).expect("convert");

        let local_key = decrypt_local_access_key(&active, &resolver);
        assert!(local_key.starts_with("sk-local-"));
        assert_ne!(local_key, INSECURE_LOCAL_KEY_PLACEHOLDER);
        assert_eq!(
            query_string(
                &active,
                "SELECT value FROM settings WHERE key = 'local_key'"
            ),
            ""
        );
        assert_eq!(
            query_i64(
                &active,
                "SELECT COUNT(*) FROM app_secret_bindings WHERE binding_scope = 'settings' AND binding_owner_id = 'local_key' AND binding_kind = 'local_access_key'"
            ),
            1
        );
    }

    #[test]
    fn baseline_conversion_rejects_plaintext_secret_conflicts_without_mutating_source() {
        let root = tempfile::tempdir().expect("tempdir");
        let app_data = root.path().join("app-data");
        let active = root.path().join("relay-pool-desktop-v2.sqlite3");
        let resolver = resolver_from_parts("device-key-v1", [7; 32]);
        let runtime = initialize_pre_baseline_runtime_for_import(&active).expect("prebaseline db");
        block_on(runtime.write(|write| {
            Box::pin(async move {
                sqlx::query(
                    r#"
                    INSERT INTO secrets (
                        id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                        created_at, updated_at
                    ) VALUES (
                        'secret-existing', 'station', 'station-1', 'api_key',
                        'sk-...', X'00', X'000000000000000000000000', '1', '1'
                    )
                    "#,
                )
                .execute(write.connection())
                .await?;
                sqlx::query(
                    r#"
                    INSERT INTO stations (
                        id, name, station_type, website_url, api_base_url, api_key,
                        api_key_secret_id, created_at, updated_at
                    ) VALUES (
                        'station-1', 'Station', 'newapi', 'https://example.test',
                        'https://example.test/v1', 'sk-conflict-plaintext-canary',
                        'secret-existing', '1', '1'
                    )
                    "#,
                )
                .execute(write.connection())
                .await?;
                Ok(())
            })
        }))
        .expect("seed conflict");
        block_on(runtime.close()).expect("close runtime");
        drop(runtime);

        let error = ensure_active_database_baseline(&app_data, &active, &resolver)
            .expect_err("conflict rejected");

        assert!(error.contains("conflicts"));
        assert_eq!(
            query_i64(
                &active,
                "SELECT schema_version FROM persistence_schema_compatibility"
            ),
            16
        );
        assert_eq!(
            query_string(
                &active,
                "SELECT api_key FROM stations WHERE id = 'station-1'"
            ),
            "sk-conflict-plaintext-canary"
        );
        assert_eq!(
            query_string(
                &active,
                "SELECT api_key_secret_id FROM stations WHERE id = 'station-1'"
            ),
            "secret-existing"
        );
        assert_eq!(query_i64(&active, "SELECT COUNT(*) FROM secrets"), 1);
    }

    #[test]
    fn baseline_conversion_resumes_candidate_built_journal_and_cleans_up() {
        let root = tempfile::tempdir().expect("tempdir");
        let app_data = root.path().join("app-data");
        let active = root.path().join("relay-pool-desktop-v2.sqlite3");
        let journal_path = app_data.join(UPGRADE_JOURNAL_FILE);
        let resolver = resolver_from_parts("device-key-v1", [11; 32]);
        let runtime = initialize_pre_baseline_runtime_for_import(&active).expect("prebaseline db");
        block_on(runtime.write(|write| {
            Box::pin(async move {
                sqlx::query(
                    "UPDATE settings SET value = 'sk-recovery-local-canary' WHERE key = 'local_key'",
                )
                .execute(write.connection())
                .await?;
                Ok(())
            })
        }))
        .expect("seed legacy local key");
        block_on(runtime.close()).expect("close runtime");
        drop(runtime);

        let journal =
            create_baseline_conversion_journal(&journal_path, &active).expect("prepared journal");
        let journal =
            execute_baseline_prepared(&app_data, &active, &journal_path, &journal).expect("backup");
        let journal = execute_baseline_backup_verified(
            &app_data,
            &active,
            &resolver,
            &journal_path,
            &journal,
        )
        .expect("candidate built");
        assert_eq!(
            journal.payload().phase,
            BaselineConversionPhase::CandidateBuilt
        );

        let report =
            ensure_active_database_baseline(&app_data, &active, &resolver).expect("resume");

        assert!(report.converted);
        assert!(!journal_path.exists());
        assert_eq!(
            query_i64(
                &active,
                "SELECT schema_version FROM persistence_schema_compatibility"
            ),
            17
        );
        let backup_path = report.backup_path.expect("backup retained");
        assert!(backup_path.is_file());
        assert!(backup_path
            .parent()
            .expect("backup parent")
            .join("security-baseline-backup-metadata.json")
            .is_file());
        assert!(!database_bytes(&active).contains("sk-recovery-local-canary"));
    }

    fn query_i64(path: &Path, sql: &str) -> i64 {
        block_on(async {
            let mut connection = connect_connection(path, false).await.expect("connect");
            let value = sqlx::query_scalar::<_, i64>(sql)
                .fetch_one(&mut connection)
                .await
                .expect("query");
            connection.close().await.expect("close");
            value
        })
    }

    fn query_string(path: &Path, sql: &str) -> String {
        block_on(async {
            let mut connection = connect_connection(path, false).await.expect("connect");
            let row = sqlx::query(sql)
                .fetch_one(&mut connection)
                .await
                .expect("query");
            connection.close().await.expect("close");
            row.get::<String, _>(0)
        })
    }

    fn decrypt_local_access_key(path: &Path, resolver: &DeviceKeyResolver) -> String {
        let (ciphertext, nonce, key_id, encryption_version) = block_on(async {
            let mut connection = connect_connection(path, false).await.expect("connect");
            let row = sqlx::query(
                r#"
                SELECT secrets.ciphertext,
                       secrets.nonce,
                       secrets.key_id,
                       secrets.encryption_version
                FROM app_secret_bindings
                JOIN secrets ON secrets.id = app_secret_bindings.secret_id
                WHERE app_secret_bindings.binding_scope = 'settings'
                  AND app_secret_bindings.binding_owner_id = 'local_key'
                  AND app_secret_bindings.binding_kind = 'local_access_key'
                "#,
            )
            .fetch_one(&mut connection)
            .await
            .expect("query local access key secret");
            connection.close().await.expect("close");
            (
                row.get::<Vec<u8>, _>("ciphertext"),
                row.get::<Vec<u8>, _>("nonce"),
                row.get::<String, _>("key_id"),
                row.get::<i64, _>("encryption_version"),
            )
        });
        assert_eq!(key_id, resolver.active_key_id().as_str());
        assert_eq!(encryption_version, i64::from(resolver.encryption_version()));
        let aad = canonical_secret_aad(
            "settings",
            "local_key",
            "local_access_key",
            resolver.encryption_version(),
        );
        resolver
            .with_active_key(|key| {
                crypto::decrypt_secret(
                    key,
                    &crypto::EncryptedPayload {
                        ciphertext: general_purpose::STANDARD.encode(ciphertext),
                        nonce: general_purpose::STANDARD.encode(nonce),
                        aad,
                        value_hash: String::new(),
                    },
                )
            })
            .expect("active key")
            .expect("decrypt local access key")
    }

    fn database_bytes(path: &Path) -> String {
        String::from_utf8_lossy(&fs::read(path).expect("read database")).to_string()
    }
}
