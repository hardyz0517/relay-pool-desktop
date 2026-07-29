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
    schema_reader::ordered_import_tables_v1,
    transform::{scan_for_sensitive_residue, PortableRow},
    validate::{
        quote_identifier, validate_closed_sqlite_database, PortableMigrationValidationError,
        PortableValidationResult,
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
                } else {
                    insert_row(&mut transaction, table_name, row).await?;
                }
            }
        }

        transaction.commit().await?;
        validate_connection(connection).await
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
            separated.push_bind(sqlite_text(row.get(*column)));
        }
    }
    builder.push(")");
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
}
