use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

use serde_json::Value;
#[cfg(test)]
use sha2::{Digest, Sha256};
use sqlx::{Connection, Row, SqliteConnection};

use super::{
    catalog::{
        migration_data_catalog, validate_schema_snapshot, DependencyStage, TablePolicy,
        EXPECTED_USER_TABLE_COUNT_V1,
    },
    format::{
        PortableMigrationManifest, PORTABLE_MIGRATION_DATABASE_GENERATION,
        PORTABLE_MIGRATION_ENCRYPTION_VERSION, PORTABLE_MIGRATION_EXPORT_POLICY_VERSION,
        PORTABLE_MIGRATION_FORMAT_VERSION, PORTABLE_MIGRATION_MIN_SCHEMA_VERSION,
        PORTABLE_MIGRATION_SCHEMA_PROFILE,
    },
    limits::PortableMigrationLimitsV1,
    transform::{
        portable_binary_value, transform_row, PortableRow, RowTransform, TransformOptions,
    },
    validate::{
        open_read_only_sqlite, quote_identifier, validate_foreign_keys, validate_quick_check,
        PortableMigrationValidationError, PortableValidationResult,
    },
};

const TRUSTED_INDEXES_V1: &[&str] = &[
    "idx_app_secret_bindings_secret_id",
    "idx_balance_snapshots_latest_station_scope",
    "idx_balance_snapshots_station_scope_updated",
    "idx_change_events_page",
    "idx_change_events_station_key_updated",
    "idx_change_events_station_page",
    "idx_change_events_station_updated",
    "idx_change_events_status_severity_updated",
    "idx_channel_monitor_runs_monitor_started",
    "idx_channel_monitor_runs_station_started",
    "idx_channel_monitor_templates_list",
    "idx_channel_monitor_attempts_execution",
    "idx_channel_monitor_executions_monitor_started",
    "idx_channel_monitor_target_results_monitor_finished",
    "idx_channel_monitor_target_results_monitor_station_finished",
    "idx_channel_monitors_due",
    "idx_channel_monitors_list",
    "idx_channel_monitors_template",
    "idx_channel_monitors_v2_due",
    "idx_collector_runs_parent",
    "idx_collector_runs_station_created",
    "idx_collector_snapshots_station_created",
    "idx_collector_task_state_due",
    "idx_group_bindings_key_group_key",
    "idx_group_bindings_station_group_key",
    "idx_group_bindings_station_status",
    "idx_group_rate_records_binding_checked",
    "idx_group_rate_records_comparison",
    "idx_group_rate_records_station_checked",
    "idx_model_aliases_client_upstream",
    "idx_model_base_prices_selection",
    "idx_pricing_rules_comparison",
    "idx_pricing_rules_selection",
    "idx_pricing_rules_station_model",
    "idx_provider_drafts_active_updated",
    "idx_provider_drafts_single_active_create",
    "idx_remote_station_keys_discovery_order",
    "idx_remote_station_keys_one_local_owner",
    "idx_route_candidate_decisions_decision",
    "idx_route_candidate_decisions_request",
    "idx_route_decisions_created_at",
    "idx_route_decisions_cursor",
    "idx_request_attempts_station_key_terminal",
    "idx_request_logs_created",
    "idx_routing_attempt_costs_request",
    "idx_routing_request_cost_aggregates_updated",
    "idx_station_key_health_observations_key_observed",
    "idx_station_keys_order",
    "idx_station_keys_routing_order",
    "idx_station_keys_station_id",
    "idx_stations_order",
];

const IGNORED_DERIVED_TABLES_V1: &[&str] = &[
    "dashboard_request_metric_rollups",
    "dashboard_request_cost_rollups",
    "dashboard_request_cost_totals_rollups",
];

const IGNORED_DERIVED_INDEXES_V1: &[&str] = &[
    "idx_request_logs_received_at",
    "idx_request_logs_dashboard_metrics_range",
    "idx_request_logs_terminal_received_at",
    "idx_dashboard_request_metric_rollups_range",
    "idx_dashboard_request_cost_rollups_range",
    "idx_dashboard_request_cost_totals_rollups_range",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PortableReaderKind {
    V1EncryptedSecrets,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum ReaderCompatibilityError {
    #[error("portable migration format version is unsupported")]
    UnsupportedFormatVersion,
    #[error("portable migration schema profile is unsupported")]
    UnsupportedSchemaProfile,
    #[error("portable migration database generation is unsupported")]
    UnsupportedDatabaseGeneration,
    #[error("portable migration database schema is unsupported")]
    UnsupportedDatabaseSchema,
    #[error("portable migration export policy version is unsupported")]
    UnsupportedExportPolicy,
    #[error("portable migration encryption version is unsupported")]
    UnsupportedEncryption,
    #[error("portable migration required feature is unsupported")]
    UnsupportedRequiredFeature,
}

#[derive(Debug, Default, Clone, Copy)]
pub(crate) struct PortableMigrationCompatibilityRegistry;

impl PortableMigrationCompatibilityRegistry {
    pub(crate) fn select_reader(
        self,
        manifest: &PortableMigrationManifest,
    ) -> Result<PortableReaderKind, ReaderCompatibilityError> {
        if manifest.format_version != PORTABLE_MIGRATION_FORMAT_VERSION {
            return Err(ReaderCompatibilityError::UnsupportedFormatVersion);
        }
        if manifest.portable_schema_profile != PORTABLE_MIGRATION_SCHEMA_PROFILE {
            return Err(ReaderCompatibilityError::UnsupportedSchemaProfile);
        }
        if manifest.database_generation != PORTABLE_MIGRATION_DATABASE_GENERATION {
            return Err(ReaderCompatibilityError::UnsupportedDatabaseGeneration);
        }
        if manifest.database_schema_version < PORTABLE_MIGRATION_MIN_SCHEMA_VERSION
            || manifest.database_schema_version
                > crate::persistence::current_schema_version() as u64
        {
            return Err(ReaderCompatibilityError::UnsupportedDatabaseSchema);
        }
        if manifest.export_policy_version != PORTABLE_MIGRATION_EXPORT_POLICY_VERSION {
            return Err(ReaderCompatibilityError::UnsupportedExportPolicy);
        }
        if manifest.encryption_version != PORTABLE_MIGRATION_ENCRYPTION_VERSION {
            return Err(ReaderCompatibilityError::UnsupportedEncryption);
        }
        if !manifest.required_features.is_empty() {
            return Err(ReaderCompatibilityError::UnsupportedRequiredFeature);
        }
        Ok(PortableReaderKind::V1EncryptedSecrets)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortableSchemaFingerprint {
    pub(crate) table_count: usize,
    pub(crate) index_count: usize,
    pub(crate) sha256: String,
}

#[derive(Debug)]
pub(crate) struct PortableSchemaReader {
    connection: SqliteConnection,
}

impl PortableSchemaReader {
    pub(crate) async fn open_v1(path: &Path) -> PortableValidationResult<Self> {
        let mut connection = open_read_only_sqlite(path).await?;
        validate_quick_check(&mut connection).await?;
        validate_schema_objects(&mut connection).await?;
        validate_declared_compatibility(&mut connection).await?;
        validate_foreign_keys(&mut connection).await?;
        Ok(Self { connection })
    }

    pub(crate) async fn read_transformed_table(
        &mut self,
        table_name: &str,
        options: TransformOptions,
    ) -> PortableValidationResult<Vec<PortableRow>> {
        super::catalog::table_catalog(table_name)
            .ok_or(PortableMigrationValidationError::UnsupportedSchemaObject)?;
        let mut rows = Vec::new();
        for row in self.read_raw_table(table_name).await? {
            match transform_row(
                table_name,
                &row,
                options,
                PortableMigrationLimitsV1::CURRENT,
            )? {
                RowTransform::Keep(row) => rows.push(row),
                RowTransform::Omit { .. } | RowTransform::Rebuild => {}
            }
        }
        Ok(rows)
    }

    async fn read_raw_table(
        &mut self,
        table_name: &str,
    ) -> PortableValidationResult<Vec<PortableRow>> {
        let catalog = super::catalog::table_catalog(table_name)
            .ok_or(PortableMigrationValidationError::UnsupportedSchemaObject)?;
        let table_identifier = quote_identifier(catalog.name)?;
        let mut select_columns = Vec::with_capacity(catalog.columns.len());
        for column in catalog.columns {
            let quoted = quote_identifier(column)?;
            if table_name == "secrets" && matches!(*column, "ciphertext" | "nonce") {
                select_columns.push(quoted);
            } else {
                select_columns.push(format!("CAST({quoted} AS TEXT)"));
            }
        }
        let sql = format!(
            "SELECT {} FROM {} ORDER BY rowid",
            select_columns.join(", "),
            table_identifier
        );
        let sqlite_rows = sqlx::query(&sql).fetch_all(&mut self.connection).await?;
        let mut output = Vec::with_capacity(sqlite_rows.len());
        for sqlite_row in sqlite_rows {
            let mut row = BTreeMap::new();
            for (index, column) in catalog.columns.iter().enumerate() {
                let value = if table_name == "secrets" && matches!(*column, "ciphertext" | "nonce")
                {
                    match sqlite_row.try_get::<Option<Vec<u8>>, _>(index)? {
                        Some(bytes) => portable_binary_value(&bytes),
                        None => Value::Null,
                    }
                } else {
                    match sqlite_row.try_get::<Option<String>, _>(index)? {
                        Some(text) => Value::String(text),
                        None => Value::Null,
                    }
                };
                row.insert((*column).to_string(), value);
            }
            output.push(row);
        }
        Ok(output)
    }

    pub(crate) async fn close(self) -> PortableValidationResult<()> {
        self.connection.close().await?;
        Ok(())
    }
}

#[cfg(test)]
pub(crate) fn trusted_schema_fingerprint_v1() -> PortableSchemaFingerprint {
    let mut hasher = Sha256::new();
    for table in migration_data_catalog() {
        hasher.update(b"table:");
        hasher.update(table.name.as_bytes());
        hasher.update(b"\npolicy:");
        hasher.update(format!("{:?}", table.policy).as_bytes());
        hasher.update(b"\ncategory:");
        hasher.update(format!("{:?}", table.category).as_bytes());
        hasher.update(b"\nstage:");
        hasher.update(format!("{:?}", table.dependency_stage).as_bytes());
        for column in table.columns {
            hasher.update(b"\ncolumn:");
            hasher.update(column.as_bytes());
        }
        hasher.update(b"\n");
    }
    for index in TRUSTED_INDEXES_V1 {
        hasher.update(b"index:");
        hasher.update(index.as_bytes());
        hasher.update(b"\n");
    }
    PortableSchemaFingerprint {
        table_count: migration_data_catalog().len(),
        index_count: TRUSTED_INDEXES_V1.len(),
        sha256: encode_hex(&hasher.finalize()),
    }
}

pub(crate) fn ordered_import_tables_v1() -> Vec<&'static str> {
    let stages = [
        DependencyStage::Internal,
        DependencyStage::Stations,
        DependencyStage::Secrets,
        DependencyStage::StationChildren,
        DependencyStage::Routing,
        DependencyStage::Pricing,
        DependencyStage::History,
    ];
    let mut tables = Vec::new();
    for stage in stages {
        for table in migration_data_catalog() {
            if table.dependency_stage == stage
                && matches!(
                    table.policy,
                    TablePolicy::Include
                        | TablePolicy::IncludeWithTransform
                        | TablePolicy::OptionalHistory
                )
            {
                tables.push(table.name);
            }
        }
    }
    tables
}

#[cfg(test)]
pub(crate) fn occupancy_categories_v1() -> BTreeSet<&'static str> {
    migration_data_catalog()
        .iter()
        .filter(|table| table.counts_for_occupancy)
        .map(|table| match table.category {
            super::catalog::DataCategory::CoreData => "core_data",
            super::catalog::DataCategory::History => "history",
            super::catalog::DataCategory::SessionCredentials => "session_credentials",
            super::catalog::DataCategory::DeviceRuntimeState => "device_runtime_state",
            super::catalog::DataCategory::ProviderDrafts => "provider_drafts",
        })
        .collect()
}

async fn validate_schema_objects(
    connection: &mut SqliteConnection,
) -> PortableValidationResult<()> {
    let rows = sqlx::query(
        r#"
        SELECT type, name, tbl_name, sql
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
          AND name != '_sqlx_migrations'
        ORDER BY type, name
        "#,
    )
    .fetch_all(&mut *connection)
    .await?;

    let trusted_tables = migration_data_catalog()
        .iter()
        .map(|table| table.name)
        .collect::<BTreeSet<_>>();
    let trusted_indexes = TRUSTED_INDEXES_V1.iter().copied().collect::<BTreeSet<_>>();

    for row in rows {
        let object_type: String = row.get("type");
        let name: String = row.get("name");
        let table_name: String = row.get("tbl_name");
        let ddl: Option<String> = row.get("sql");
        match object_type.as_str() {
            "table" => {
                if IGNORED_DERIVED_TABLES_V1.contains(&name.as_str()) {
                    continue;
                }
                if !trusted_tables.contains(name.as_str()) {
                    return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
                }
                if ddl
                    .as_deref()
                    .is_some_and(|sql| sql.to_ascii_uppercase().contains("VIRTUAL TABLE"))
                {
                    return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
                }
            }
            "index" => {
                if IGNORED_DERIVED_INDEXES_V1.contains(&name.as_str()) {
                    continue;
                }
                if !trusted_indexes.contains(name.as_str())
                    || !trusted_tables.contains(table_name.as_str())
                {
                    return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
                }
            }
            "view" | "trigger" => {
                return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
            }
            _ => return Err(PortableMigrationValidationError::UnsupportedSchemaObject),
        }
    }

    let mut actual = Vec::new();
    for table in migration_data_catalog() {
        actual.push((
            table.name,
            read_table_columns(connection, table.name).await?,
        ));
    }
    let actual_refs = actual
        .iter()
        .map(|(name, columns)| {
            (
                *name,
                columns.iter().map(String::as_str).collect::<Vec<_>>(),
            )
        })
        .collect::<Vec<_>>();
    let snapshot = actual_refs
        .iter()
        .map(|(name, columns)| (*name, columns.as_slice()))
        .collect::<Vec<_>>();
    validate_schema_snapshot(&snapshot)?;

    if migration_data_catalog().len() != EXPECTED_USER_TABLE_COUNT_V1 {
        return Err(PortableMigrationValidationError::UnsupportedSchema);
    }

    Ok(())
}

async fn read_table_columns(
    connection: &mut SqliteConnection,
    table_name: &str,
) -> PortableValidationResult<Vec<String>> {
    let table_identifier = quote_identifier(table_name)?;
    let pragma = format!("PRAGMA table_info({table_identifier})");
    let rows = sqlx::query(&pragma).fetch_all(&mut *connection).await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect())
}

async fn validate_declared_compatibility(
    connection: &mut SqliteConnection,
) -> PortableValidationResult<()> {
    let row = sqlx::query(
        r#"
        SELECT database_generation, schema_version
        FROM persistence_schema_compatibility
        WHERE singleton_key = 1
        "#,
    )
    .fetch_one(&mut *connection)
    .await?;
    let generation: i64 = row.get("database_generation");
    let schema_version: i64 = row.get("schema_version");
    let current_schema = crate::persistence::current_schema_version();
    if generation != PORTABLE_MIGRATION_DATABASE_GENERATION as i64
        || schema_version < PORTABLE_MIGRATION_MIN_SCHEMA_VERSION as i64
        || schema_version > current_schema
    {
        return Err(PortableMigrationValidationError::UnsupportedSchema);
    }
    Ok(())
}

#[cfg(test)]
fn encode_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use serde_json::json;
    use sqlx::{
        sqlite::{SqliteConnectOptions, SqliteJournalMode},
        ConnectOptions, Connection, Executor, SqliteConnection,
    };

    use super::*;
    use crate::persistence::migrations::initialize_v2_database;

    #[test]
    fn compatibility_registry_is_exact_and_fail_closed() {
        let mut manifest = fixture_manifest();
        let registry = PortableMigrationCompatibilityRegistry;

        assert_eq!(
            registry.select_reader(&manifest).expect("supported"),
            PortableReaderKind::V1EncryptedSecrets
        );

        manifest.required_features.push("future".to_string());
        assert!(matches!(
            registry.select_reader(&manifest),
            Err(ReaderCompatibilityError::UnsupportedRequiredFeature)
        ));
        manifest.required_features.clear();

        manifest.export_policy_version += 1;
        assert!(matches!(
            registry.select_reader(&manifest),
            Err(ReaderCompatibilityError::UnsupportedExportPolicy)
        ));
    }

    #[test]
    fn trusted_schema_fingerprint_matches_fixture() {
        let fixture =
            include_str!("../../../tests/fixtures/portable-migration/v1/schema-fingerprint.txt")
                .trim();
        let fingerprint = trusted_schema_fingerprint_v1();

        assert_eq!(fingerprint.sha256, fixture);
        assert_eq!(fingerprint.table_count, 43);
        assert_eq!(fingerprint.index_count, 51);
    }

    #[tokio::test]
    async fn reader_accepts_current_schema_and_reads_with_fixed_selects() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("source.sqlite");
        initialize_v2_database(&path).await.expect("initialize");
        execute_sql(
            &path,
            r#"
            UPDATE settings
            SET value = '35'
            WHERE key = 'collector_interval_minutes'
            "#,
        )
        .await;

        let mut reader = PortableSchemaReader::open_v1(&path).await.expect("reader");
        let rows = reader
            .read_transformed_table("settings", TransformOptions::default())
            .await
            .expect("settings");
        reader.close().await.expect("close");

        assert!(rows.iter().any(|row| {
            row.get("key") == Some(&json!("collector_interval_minutes"))
                && row.get("value") == Some(&json!("35"))
        }));
    }

    #[tokio::test]
    async fn reader_rejects_unknown_schema_objects_columns_and_spoofed_versions() {
        let directory = tempfile::tempdir().expect("tempdir");

        let unknown_table = create_current_database(directory.path(), "unknown-table.sqlite").await;
        execute_sql(&unknown_table, "CREATE TABLE attacker_owned (id TEXT)").await;
        assert_unsupported_object(&unknown_table).await;

        let unknown_column =
            create_current_database(directory.path(), "unknown-column.sqlite").await;
        execute_sql(
            &unknown_column,
            "ALTER TABLE settings ADD COLUMN injected TEXT",
        )
        .await;
        assert_unsupported_object(&unknown_column).await;

        let trigger = create_current_database(directory.path(), "trigger.sqlite").await;
        execute_sql(
            &trigger,
            "CREATE TRIGGER injected_trigger AFTER INSERT ON settings BEGIN SELECT 1; END",
        )
        .await;
        assert_unsupported_object(&trigger).await;

        let view = create_current_database(directory.path(), "view.sqlite").await;
        execute_sql(&view, "CREATE VIEW injected_view AS SELECT 1 AS value").await;
        assert_unsupported_object(&view).await;

        let spoofed_version = create_current_database(directory.path(), "spoofed.sqlite").await;
        execute_sql(
            &spoofed_version,
            "UPDATE persistence_schema_compatibility SET schema_version = 999 WHERE singleton_key = 1",
        )
        .await;
        let error = PortableSchemaReader::open_v1(&spoofed_version)
            .await
            .expect_err("spoofed version rejected");
        assert!(matches!(
            error,
            PortableMigrationValidationError::UnsupportedSchema
        ));
    }

    async fn create_current_database(directory: &Path, name: &str) -> std::path::PathBuf {
        let path = directory.join(name);
        initialize_v2_database(&path).await.expect("initialize");
        path
    }

    async fn assert_unsupported_object(path: &Path) {
        let error = PortableSchemaReader::open_v1(path)
            .await
            .expect_err("schema object rejected");
        assert!(matches!(
            error,
            PortableMigrationValidationError::UnsupportedSchemaObject
                | PortableMigrationValidationError::CatalogDrift(_)
        ));
    }

    async fn execute_sql(path: &Path, sql: &str) {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .disable_statement_logging();
        let mut connection = SqliteConnection::connect_with(&options)
            .await
            .expect("connect");
        connection.execute(sql).await.expect("execute");
        connection.close().await.expect("close");
    }

    fn fixture_manifest() -> PortableMigrationManifest {
        PortableMigrationManifest {
            format: "relay-pool-portable-migration".to_string(),
            format_version: PORTABLE_MIGRATION_FORMAT_VERSION,
            export_id: "019fad3d-631e-76d0-8659-ac335efde02d".to_string(),
            created_at: "2026-07-29T00:00:00Z".to_string(),
            source_app_version: "0.3.1".to_string(),
            source_platform: "windows".to_string(),
            database_generation: PORTABLE_MIGRATION_DATABASE_GENERATION,
            database_schema_version: PORTABLE_MIGRATION_MIN_SCHEMA_VERSION,
            portable_schema_profile: PORTABLE_MIGRATION_SCHEMA_PROFILE.to_string(),
            minimum_importer_version: "0.3.1".to_string(),
            transport_key_id: "019fad3d-631e-76d0-8659-ac335efde02e".to_string(),
            encryption_version: PORTABLE_MIGRATION_ENCRYPTION_VERSION,
            export_policy_version: PORTABLE_MIGRATION_EXPORT_POLICY_VERSION,
            required_features: Vec::new(),
            extensions: json!({}),
            included_categories: vec!["core_data".to_string()],
            excluded_categories: Vec::new(),
            record_counts: BTreeMap::new(),
            sqlite_size_bytes: 0,
            sqlite_sha256: "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=".to_string(),
        }
    }
}
