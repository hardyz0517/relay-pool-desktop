use std::{
    collections::BTreeMap,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqliteSynchronous},
    ConnectOptions, Connection, QueryBuilder, Row, Sqlite, SqliteConnection,
};

use super::{
    catalog::{table_catalog, TablePolicy},
    common_login_contract::{
        is_supported_password_scope, CommonLoginCatalog, LegacyCommonLoginProfile,
        COMMON_LOGIN_SETTING, LEGACY_COMMON_LOGIN_SETTING, LEGACY_PASSWORD_SCOPE, PASSWORD_KIND,
    },
    schema_reader::ordered_import_tables_v1,
    transform::{portable_binary_bytes, scan_for_sensitive_residue, PortableRow},
    validate::{
        open_read_only_sqlite, quote_identifier, validate_closed_sqlite_database,
        PortableMigrationValidationError, PortableValidationResult,
    },
};
use crate::services::data_store::atomic_file::{
    unique_sibling, ApprovedLeaf, AtomicFilePublishPort, LocalAtomicFileAdapter, PublishMode,
};

#[derive(Debug, Clone)]
pub(crate) struct TrustedTableBatch {
    pub(crate) table_name: String,
    pub(crate) rows: Vec<PortableRow>,
}

#[derive(Debug, Default)]
pub(crate) struct TrustedTargetWriter;

impl TrustedTargetWriter {
    pub(crate) async fn rebuild_current_database(
        &self,
        target_path: &Path,
        batches: &[TrustedTableBatch],
    ) -> PortableValidationResult<()> {
        if target_path.exists() {
            return Err(PortableMigrationValidationError::UnsupportedSchema);
        }

        crate::persistence::migrations::initialize_v2_database(target_path)
            .await
            .map_err(|_| PortableMigrationValidationError::Sql)?;

        let mut connection = open_trusted_writer(target_path).await?;
        let result = self.write_batches(&mut connection, batches).await;
        if result.is_ok() {
            connection.close().await?;
            finalize_closed_database(target_path).await?;
            validate_closed_sqlite_database(target_path).await?;
        }
        result
    }

    async fn write_batches(
        &self,
        connection: &mut SqliteConnection,
        batches: &[TrustedTableBatch],
    ) -> PortableValidationResult<()> {
        let batch_by_table = batches
            .iter()
            .map(|batch| (batch.table_name.as_str(), batch))
            .collect::<BTreeMap<_, _>>();

        for table_name in batch_by_table.keys() {
            let catalog = table_catalog(table_name)
                .ok_or(PortableMigrationValidationError::UnsupportedSchemaObject)?;
            if !matches!(
                catalog.policy,
                TablePolicy::Include
                    | TablePolicy::IncludeWithTransform
                    | TablePolicy::OptionalHistory
            ) {
                return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
            }
        }

        let mut transaction = connection.begin().await?;
        sqlx::query("PRAGMA defer_foreign_keys = ON")
            .execute(&mut *transaction)
            .await?;

        for table_name in ordered_import_tables_v1() {
            let Some(batch) = batch_by_table.get(table_name) else {
                continue;
            };
            for row in &batch.rows {
                validate_row(table_name, row)?;
                if table_name == "settings" {
                    upsert_setting(&mut transaction, row).await?;
                } else if table_name == "routing_policy" {
                    upsert_routing_policy(&mut transaction, row).await?;
                } else {
                    insert_row(&mut transaction, table_name, row).await?;
                }
            }
        }

        rebuild_domain_revision_baseline(&mut transaction).await?;

        transaction.commit().await?;
        ensure_common_login_secret_references(connection).await?;
        validate_connection(connection).await
    }
}

async fn rebuild_domain_revision_baseline(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
) -> PortableValidationResult<()> {
    sqlx::query("DELETE FROM domain_revisions")
        .execute(&mut **transaction)
        .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'station:' || id, MAX(endpoint_revision, 1), 0,
                CASE WHEN endpoint_revision > 0 THEN 'legacy_endpoint_revision' ELSE 'baseline_snapshot' END
         FROM stations",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'station_account:' || id, 1, 0, 'baseline_snapshot' FROM stations",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'station_key:' || id, ROW_NUMBER() OVER (ORDER BY id), 0, 'baseline_snapshot'
         FROM station_keys",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'station_group:' || id, 1, 0, 'baseline_snapshot' FROM station_group_bindings",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'setting:' || key, ROW_NUMBER() OVER (ORDER BY key), 0, 'baseline_snapshot'
         FROM settings",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('model_alias:direct', 1, 0, 'baseline_snapshot')",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'model_alias:' || id, ROW_NUMBER() OVER (ORDER BY id), 0, 'baseline_snapshot'
         FROM model_aliases",
    )
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         SELECT 'routing_policy', COALESCE(MAX(config_revision), 1), 0, 'baseline_snapshot'
         FROM routing_policy",
    )
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

pub(crate) async fn validate_rebuilt_target_database(
    path: &Path,
    target_key_id: &str,
    transport_key_id: &str,
) -> PortableValidationResult<BTreeMap<String, usize>> {
    let mut connection = open_read_only_sqlite(path).await?;
    let mut row_counts = BTreeMap::new();
    for table_name in ordered_import_tables_v1() {
        row_counts.insert(
            table_name.to_string(),
            rebuilt_table_count(&mut connection, table_name).await?,
        );
    }
    ensure_rebuilt_secrets_use_target_key(&mut connection, target_key_id, transport_key_id).await?;
    ensure_common_login_secret_references(&mut connection).await?;
    connection.close().await?;
    Ok(row_counts)
}

async fn ensure_common_login_secret_references(
    connection: &mut SqliteConnection,
) -> PortableValidationResult<()> {
    let current = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
        .bind(COMMON_LOGIN_SETTING)
        .fetch_optional(&mut *connection)
        .await?;
    if let Some(value) = current {
        let catalog: CommonLoginCatalog = serde_json::from_str(&value)
            .map_err(|_| PortableMigrationValidationError::UnsupportedSchema)?;
        for password in catalog.passwords {
            if password.id.is_empty()
                || password.password_secret_id.is_empty()
                || !is_supported_password_scope(&password.secret_scope)
            {
                return Err(PortableMigrationValidationError::UnsupportedSchema);
            }
            ensure_exact_secret_reference(
                connection,
                &password.password_secret_id,
                &password.secret_scope,
                &password.id,
            )
            .await?;
        }
        return Ok(());
    }

    let legacy = sqlx::query_scalar::<_, String>("SELECT value FROM settings WHERE key = ?1")
        .bind(LEGACY_COMMON_LOGIN_SETTING)
        .fetch_optional(&mut *connection)
        .await?;
    let Some(value) = legacy else {
        return Ok(());
    };
    let profiles: Vec<LegacyCommonLoginProfile> = serde_json::from_str(&value)
        .map_err(|_| PortableMigrationValidationError::UnsupportedSchema)?;
    for profile in profiles {
        let Some(secret_id) = profile.password_secret_id else {
            continue;
        };
        if profile.id.is_empty() || secret_id.is_empty() {
            return Err(PortableMigrationValidationError::UnsupportedSchema);
        }
        ensure_exact_secret_reference(connection, &secret_id, LEGACY_PASSWORD_SCOPE, &profile.id)
            .await?;
    }
    Ok(())
}

async fn ensure_exact_secret_reference(
    connection: &mut SqliteConnection,
    secret_id: &str,
    scope: &str,
    owner_id: &str,
) -> PortableValidationResult<()> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM secrets
         WHERE id = ?1 AND scope = ?2 AND owner_id = ?3 AND kind = ?4",
    )
    .bind(secret_id)
    .bind(scope)
    .bind(owner_id)
    .bind(PASSWORD_KIND)
    .fetch_one(&mut *connection)
    .await?;
    if count == 1 {
        Ok(())
    } else {
        Err(PortableMigrationValidationError::UnsupportedSchema)
    }
}

async fn open_trusted_writer(path: &Path) -> PortableValidationResult<SqliteConnection> {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .disable_statement_logging();
    let mut connection = SqliteConnection::connect_with(&options).await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await?;
    Ok(connection)
}

async fn rebuilt_table_count(
    connection: &mut SqliteConnection,
    table_name: &str,
) -> PortableValidationResult<usize> {
    let sql = format!("SELECT COUNT(*) FROM {}", quote_identifier(table_name)?);
    let count: i64 = sqlx::query_scalar(&sql).fetch_one(&mut *connection).await?;
    usize::try_from(count).map_err(|_| PortableMigrationValidationError::UnsupportedSchema)
}

async fn ensure_rebuilt_secrets_use_target_key(
    connection: &mut SqliteConnection,
    target_key_id: &str,
    transport_key_id: &str,
) -> PortableValidationResult<()> {
    let rows = sqlx::query("SELECT key_id FROM secrets")
        .fetch_all(&mut *connection)
        .await?;
    for row in rows {
        let key_id: String = row.get("key_id");
        if key_id != target_key_id || key_id == transport_key_id {
            return Err(PortableMigrationValidationError::UnsupportedSchema);
        }
    }
    Ok(())
}

fn validate_row(table_name: &str, row: &PortableRow) -> PortableValidationResult<()> {
    let catalog = table_catalog(table_name)
        .ok_or(PortableMigrationValidationError::UnsupportedSchemaObject)?;
    for column in row.keys() {
        if !catalog.columns.contains(&column.as_str()) {
            return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
        }
    }
    let serialized = serde_json::to_vec(row).map_err(|_| PortableMigrationValidationError::Sql)?;
    scan_for_sensitive_residue(&serialized)?;
    Ok(())
}

async fn upsert_setting(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    row: &PortableRow,
) -> PortableValidationResult<()> {
    let key =
        sqlite_text(row.get("key")).ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    let value =
        sqlite_text(row.get("value")).ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    let updated_at = sqlite_text(row.get("updated_at"))
        .ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    sqlx::query(
        r#"
        INSERT INTO settings (key, value, updated_at)
        VALUES (?1, ?2, ?3)
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = excluded.updated_at
        "#,
    )
    .bind(key)
    .bind(value)
    .bind(updated_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn insert_row(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    table_name: &str,
    row: &PortableRow,
) -> PortableValidationResult<()> {
    let catalog = table_catalog(table_name)
        .ok_or(PortableMigrationValidationError::UnsupportedSchemaObject)?;
    let columns = catalog
        .columns
        .iter()
        .copied()
        .filter(|column| row.contains_key(*column))
        .collect::<Vec<_>>();
    if columns.is_empty() {
        return Err(PortableMigrationValidationError::UnsupportedSchema);
    }

    let mut builder = QueryBuilder::<Sqlite>::new("INSERT INTO ");
    builder.push(quote_identifier(table_name)?);
    builder.push(" (");
    {
        let mut separated = builder.separated(", ");
        for column in &columns {
            separated.push(quote_identifier(column)?);
        }
    }
    builder.push(") VALUES (");
    {
        let mut separated = builder.separated(", ");
        for column in &columns {
            if table_name == "secrets" && matches!(*column, "ciphertext" | "nonce") {
                separated.push_bind(sqlite_blob(row.get(*column))?);
            } else {
                separated.push_bind(sqlite_text(row.get(*column)));
            }
        }
    }
    builder.push(")");
    builder.build().execute(&mut **transaction).await?;
    Ok(())
}

async fn upsert_routing_policy(
    transaction: &mut sqlx::Transaction<'_, Sqlite>,
    row: &PortableRow,
) -> PortableValidationResult<()> {
    let columns = [
        "singleton_key",
        "config_json",
        "config_revision",
        "policy_version",
        "system_version",
        "status",
        "created_at_ms",
        "updated_at_ms",
    ];
    let mut builder = QueryBuilder::<Sqlite>::new(
        "INSERT INTO routing_policy (singleton_key, config_json, config_revision, policy_version, system_version, status, created_at_ms, updated_at_ms) VALUES (",
    );
    {
        let mut separated = builder.separated(", ");
        for column in columns {
            separated.push_bind(sqlite_text(row.get(column)));
        }
    }
    builder.push(") ON CONFLICT(singleton_key) DO UPDATE SET config_json = excluded.config_json, config_revision = excluded.config_revision, policy_version = excluded.policy_version, system_version = excluded.system_version, status = excluded.status, created_at_ms = excluded.created_at_ms, updated_at_ms = excluded.updated_at_ms");
    builder.build().execute(&mut **transaction).await?;
    Ok(())
}

async fn finalize_closed_database(target_path: &Path) -> PortableValidationResult<()> {
    checkpoint_and_remove_empty_sidecars(target_path).await?;
    let compact_path = unique_sibling(target_path, "portable-import-compact");
    vacuum_into(target_path, &compact_path).await?;
    checkpoint_and_remove_empty_sidecars(target_path).await?;
    publish_compacted_database(&compact_path, target_path)?;
    remove_empty_sidecars(target_path)?;
    Ok(())
}

async fn checkpoint_and_remove_empty_sidecars(path: &Path) -> PortableValidationResult<()> {
    let mut connection = open_trusted_writer(path).await?;
    sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    remove_empty_sidecars(path)
}

async fn vacuum_into(source_path: &Path, compact_path: &Path) -> PortableValidationResult<()> {
    let mut connection = open_trusted_writer(source_path).await?;
    let compact = compact_path
        .to_str()
        .ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    sqlx::query("VACUUM INTO ?1")
        .bind(compact)
        .execute(&mut connection)
        .await?;
    connection.close().await?;
    Ok(())
}

fn publish_compacted_database(
    compact_path: &Path,
    target_path: &Path,
) -> PortableValidationResult<()> {
    let parent = target_path
        .parent()
        .ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    let leaf = target_path
        .file_name()
        .ok_or(PortableMigrationValidationError::UnsupportedSchema)?
        .to_os_string();
    let approved = ApprovedLeaf::approve(parent, leaf)
        .map_err(|_| PortableMigrationValidationError::AtomicPublish)?;
    LocalAtomicFileAdapter
        .publish(compact_path, &approved, PublishMode::ReplaceExisting)
        .map_err(|_| PortableMigrationValidationError::AtomicPublish)?;
    Ok(())
}

fn remove_empty_sidecars(path: &Path) -> PortableValidationResult<()> {
    for suffix in ["wal", "shm"] {
        let sidecar = sqlite_sidecar_path(path, suffix)?;
        if !sidecar.exists() {
            continue;
        }
        let metadata = fs::metadata(&sidecar).map_err(|_| PortableMigrationValidationError::Sql)?;
        if metadata.len() != 0 {
            return Err(PortableMigrationValidationError::SidecarNotEmpty);
        }
        fs::remove_file(&sidecar).map_err(|_| PortableMigrationValidationError::Sql)?;
    }
    Ok(())
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PortableValidationResult<PathBuf> {
    let file_name = path
        .file_name()
        .ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    let mut sidecar_name = OsString::from(file_name);
    sidecar_name.push(format!("-{suffix}"));
    Ok(path.with_file_name(sidecar_name))
}

async fn validate_connection(connection: &mut SqliteConnection) -> PortableValidationResult<()> {
    let quick_check: String = sqlx::query("PRAGMA quick_check")
        .fetch_one(&mut *connection)
        .await?
        .get(0);
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Err(PortableMigrationValidationError::QuickCheckFailed);
    }
    if sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&mut *connection)
        .await?
        .is_some()
    {
        return Err(PortableMigrationValidationError::ForeignKeyCheckFailed);
    }
    Ok(())
}

fn sqlite_text(value: Option<&Value>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) => Some(text.clone()),
        Some(Value::Bool(value)) => Some(if *value { "1" } else { "0" }.to_string()),
        Some(Value::Number(value)) => Some(value.to_string()),
        Some(Value::Array(_)) | Some(Value::Object(_)) => value.map(Value::to_string),
    }
}

fn sqlite_blob(value: Option<&Value>) -> PortableValidationResult<Vec<u8>> {
    let value = value.ok_or(PortableMigrationValidationError::UnsupportedSchema)?;
    portable_binary_bytes(value).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn writer_rebuilds_from_trusted_migrations_and_upserts_allowed_settings() {
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("target.sqlite");
        let writer = TrustedTargetWriter;
        let batch = TrustedTableBatch {
            table_name: "settings".to_string(),
            rows: vec![PortableRow::from([
                ("key".to_string(), json!("collector_interval_minutes")),
                ("value".to_string(), json!("45")),
                ("updated_at".to_string(), json!("2026-07-29T00:00:00Z")),
            ])],
        };

        writer
            .rebuild_current_database(&target, &[batch])
            .await
            .expect("write target");

        let mut connection = open_trusted_writer(&target).await.expect("connect");
        let value: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'collector_interval_minutes'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("setting");
        connection.close().await.expect("close");

        assert_eq!(value, "45");
        assert!(
            !sqlite_sidecar_path(&target, "wal")
                .expect("wal path")
                .exists(),
            "final target must not leave a WAL sidecar"
        );
        assert!(
            !sqlite_sidecar_path(&target, "shm")
                .expect("shm path")
                .exists(),
            "final target must not leave a SHM sidecar"
        );
    }

    #[tokio::test]
    async fn writer_rejects_unknown_tables_columns_sensitive_residue_and_existing_targets() {
        let directory = tempfile::tempdir().expect("tempdir");
        let writer = TrustedTargetWriter;

        let existing = directory.path().join("existing.sqlite");
        std::fs::write(&existing, b"already here").expect("fixture");
        assert!(matches!(
            writer.rebuild_current_database(&existing, &[]).await,
            Err(PortableMigrationValidationError::UnsupportedSchema)
        ));

        let unknown_table = directory.path().join("unknown-table.sqlite");
        let batch = TrustedTableBatch {
            table_name: "attacker".to_string(),
            rows: Vec::new(),
        };
        assert!(matches!(
            writer
                .rebuild_current_database(&unknown_table, &[batch])
                .await,
            Err(PortableMigrationValidationError::UnsupportedSchemaObject)
        ));

        let unknown_column = directory.path().join("unknown-column.sqlite");
        let batch = TrustedTableBatch {
            table_name: "settings".to_string(),
            rows: vec![PortableRow::from([
                ("key".to_string(), json!("collector_interval_minutes")),
                ("value".to_string(), json!("45")),
                ("updated_at".to_string(), json!("2026-07-29T00:00:00Z")),
                ("injected".to_string(), json!("nope")),
            ])],
        };
        assert!(matches!(
            writer
                .rebuild_current_database(&unknown_column, &[batch])
                .await,
            Err(PortableMigrationValidationError::UnsupportedSchemaObject)
        ));

        let canary = directory.path().join("canary.sqlite");
        let batch = TrustedTableBatch {
            table_name: "settings".to_string(),
            rows: vec![PortableRow::from([
                ("key".to_string(), json!("collector_interval_minutes")),
                ("value".to_string(), json!("sk-validation-canary")),
                ("updated_at".to_string(), json!("2026-07-29T00:00:00Z")),
            ])],
        };
        assert!(matches!(
            writer.rebuild_current_database(&canary, &[batch]).await,
            Err(PortableMigrationValidationError::Transform(_))
        ));
    }

    #[tokio::test]
    async fn writer_preserves_secret_blob_types_from_portable_base64() {
        let directory = tempfile::tempdir().expect("tempdir");
        let target = directory.path().join("secret-blobs.sqlite");
        let writer = TrustedTargetWriter;
        let batch = TrustedTableBatch {
            table_name: "secrets".to_string(),
            rows: vec![PortableRow::from([
                ("id".to_string(), json!("secret-1")),
                ("scope".to_string(), json!("station_key")),
                ("owner_id".to_string(), json!("key-1")),
                ("kind".to_string(), json!("api_key")),
                ("masked_value".to_string(), json!("sk-...nary")),
                (
                    "ciphertext".to_string(),
                    super::portable_binary_bytes_test_value(&[1, 2, 3]),
                ),
                (
                    "nonce".to_string(),
                    super::portable_binary_bytes_test_value(&[4; 12]),
                ),
                ("created_at".to_string(), json!("1")),
                ("updated_at".to_string(), json!("2")),
                ("key_id".to_string(), json!("transport:key")),
                ("encryption_version".to_string(), json!("1")),
                ("value_hash".to_string(), json!("hash")),
            ])],
        };

        writer
            .rebuild_current_database(&target, &[batch])
            .await
            .expect("write target");

        let mut connection = open_trusted_writer(&target).await.expect("connect");
        let ciphertext: Vec<u8> =
            sqlx::query_scalar("SELECT ciphertext FROM secrets WHERE id = 'secret-1'")
                .fetch_one(&mut connection)
                .await
                .expect("ciphertext");
        connection.close().await.expect("close");
        assert_eq!(ciphertext, vec![1, 2, 3]);
    }

    fn common_login_batches(scope: &str, owner_id: &str) -> Vec<TrustedTableBatch> {
        vec![
            TrustedTableBatch {
                table_name: "settings".to_string(),
                rows: vec![PortableRow::from([
                    ("key".to_string(), json!("common_login_catalog_json")),
                    (
                        "value".to_string(),
                        json!(serde_json::json!({
                            "emails": [{"id": "email-1", "email": "person@example.com"}],
                            "passwords": [{
                                "id": "password-1",
                                "passwordSecretId": "secret-1",
                                "secretScope": scope
                            }]
                        })
                        .to_string()),
                    ),
                    ("updated_at".to_string(), json!("1")),
                ])],
            },
            TrustedTableBatch {
                table_name: "secrets".to_string(),
                rows: vec![PortableRow::from([
                    ("id".to_string(), json!("secret-1")),
                    ("scope".to_string(), json!(scope)),
                    ("owner_id".to_string(), json!(owner_id)),
                    ("kind".to_string(), json!("password")),
                    ("masked_value".to_string(), json!("********")),
                    (
                        "ciphertext".to_string(),
                        portable_binary_bytes_test_value(&[1, 2, 3]),
                    ),
                    (
                        "nonce".to_string(),
                        portable_binary_bytes_test_value(&[4; 12]),
                    ),
                    ("created_at".to_string(), json!("1")),
                    ("updated_at".to_string(), json!("1")),
                    ("key_id".to_string(), json!("transport:key")),
                    ("encryption_version".to_string(), json!("1")),
                    ("value_hash".to_string(), json!("hash")),
                ])],
            },
        ]
    }

    #[tokio::test]
    async fn writer_validates_current_and_legacy_common_login_secret_references() {
        let directory = tempfile::tempdir().expect("tempdir");
        for (index, scope) in ["common_login_password", "common_login_profile"]
            .into_iter()
            .enumerate()
        {
            TrustedTargetWriter
                .rebuild_current_database(
                    &directory.path().join(format!("valid-{index}.sqlite")),
                    &common_login_batches(scope, "password-1"),
                )
                .await
                .expect("valid reference");
        }

        let invalid = directory.path().join("invalid-owner.sqlite");
        assert!(matches!(
            TrustedTargetWriter
                .rebuild_current_database(
                    &invalid,
                    &common_login_batches("common_login_password", "different-owner"),
                )
                .await,
            Err(PortableMigrationValidationError::UnsupportedSchema)
        ));

        let missing = directory.path().join("missing-secret.sqlite");
        let mut missing_batches = common_login_batches("common_login_password", "password-1");
        missing_batches.retain(|batch| batch.table_name != "secrets");
        assert!(matches!(
            TrustedTargetWriter
                .rebuild_current_database(&missing, &missing_batches)
                .await,
            Err(PortableMigrationValidationError::UnsupportedSchema)
        ));

        let wrong_scope = directory.path().join("wrong-scope.sqlite");
        let mut wrong_scope_batches = common_login_batches("common_login_password", "password-1");
        wrong_scope_batches[1].rows[0].insert("scope".to_string(), json!("common_login_profile"));
        assert!(matches!(
            TrustedTargetWriter
                .rebuild_current_database(&wrong_scope, &wrong_scope_batches)
                .await,
            Err(PortableMigrationValidationError::UnsupportedSchema)
        ));
    }
}

#[cfg(test)]
fn portable_binary_bytes_test_value(bytes: &[u8]) -> Value {
    super::transform::portable_binary_value(bytes)
}
