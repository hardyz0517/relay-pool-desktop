use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use tokio_util::sync::CancellationToken;

use crate::{
    application::data_maintenance::{DataMaintenanceActivity, DataMaintenanceCoordinator},
    services::{
        portable_migration::{
            schema_reader::{ordered_import_tables_v1, PortableSchemaReader},
            snapshot::{create_consistent_snapshot, remove_snapshot_file},
            target_writer::{TrustedTableBatch, TrustedTargetWriter},
            transform::{
                encrypted_secret_from_portable_row, portable_row_from_encrypted_secret,
                PortableRow, TransformOptions,
            },
            validate::PortableMigrationValidationError,
        },
        secrets::{
            rekey::{
                BufferedSecretRekeyWriter, SecretRekeyPolicy, SecretRekeyReport,
                SecretRekeyService, TransportSecretKey,
            },
            DeviceKeyResolver, CURRENT_SECRET_ENCRYPTION_VERSION,
        },
    },
};

use super::errors::{DataMigrationError, DataMigrationResult};

#[derive(Debug, Clone)]
pub(crate) struct PortableExportRequest {
    pub(crate) source_database_path: PathBuf,
    pub(crate) portable_database_path: PathBuf,
    pub(crate) working_directory: PathBuf,
    pub(crate) include_history: bool,
}

#[derive(Debug)]
pub(crate) struct PortableExportArtifact {
    pub(crate) portable_database_path: PathBuf,
    pub(crate) snapshot_created_at: String,
    pub(crate) row_counts: BTreeMap<String, usize>,
    pub(crate) rekey_report: SecretRekeyReport,
    pub(crate) transport_key: TransportSecretKey,
}

#[derive(Debug, Clone)]
pub(crate) struct DataMigrationExportService {
    maintenance: DataMaintenanceCoordinator,
    source_keys: DeviceKeyResolver,
}

impl DataMigrationExportService {
    pub(crate) fn new(
        maintenance: DataMaintenanceCoordinator,
        source_keys: DeviceKeyResolver,
    ) -> Self {
        Self {
            maintenance,
            source_keys,
        }
    }

    pub(crate) async fn export_portable_sqlite(
        &self,
        request: PortableExportRequest,
        cancellation: Option<&CancellationToken>,
    ) -> DataMigrationResult<PortableExportArtifact> {
        if request.portable_database_path.exists() {
            return Err(DataMigrationError::TargetExists);
        }
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(DataMigrationError::Snapshot(
                crate::services::portable_migration::snapshot::PortableSnapshotError::Cancelled,
            ));
        }
        fs::create_dir_all(&request.working_directory)
            .map_err(|_| DataMigrationError::CleanupFailed)?;
        let snapshot_path = unique_snapshot_path(&request.working_directory);

        let snapshot = {
            let _lease = self.maintenance.begin(DataMaintenanceActivity::Export)?;
            create_consistent_snapshot(&request.source_database_path, &snapshot_path, cancellation)
                .await?
        };

        let transport_key = TransportSecretKey::generate();
        let export_result = self
            .build_portable_database_from_snapshot(
                &snapshot.snapshot_path,
                &request.portable_database_path,
                request.include_history,
                transport_key,
                cancellation,
            )
            .await;

        let snapshot_cleanup = remove_snapshot_file(&snapshot.snapshot_path);
        match (export_result, snapshot_cleanup) {
            (Ok(mut artifact), Ok(())) => {
                artifact.snapshot_created_at = snapshot.created_at;
                Ok(artifact)
            }
            (Err(error), _) => {
                cleanup_unpublished_target(&request.portable_database_path)?;
                Err(error)
            }
            (Ok(_), Err(error)) => {
                cleanup_unpublished_target(&request.portable_database_path)?;
                Err(DataMigrationError::Snapshot(error))
            }
        }
    }

    async fn build_portable_database_from_snapshot(
        &self,
        snapshot_path: &Path,
        portable_database_path: &Path,
        include_history: bool,
        transport_key: TransportSecretKey,
        cancellation: Option<&CancellationToken>,
    ) -> DataMigrationResult<PortableExportArtifact> {
        let mut reader = PortableSchemaReader::open_v1(snapshot_path).await?;
        let options = TransformOptions { include_history };
        let mut batches = Vec::new();
        let mut row_counts = BTreeMap::new();
        let mut rekey_report = None;

        for table_name in ordered_import_tables_v1() {
            if cancellation.is_some_and(CancellationToken::is_cancelled) {
                reader.close().await?;
                return Err(DataMigrationError::Snapshot(
                    crate::services::portable_migration::snapshot::PortableSnapshotError::Cancelled,
                ));
            }
            let rows = reader.read_transformed_table(table_name, options).await?;
            let rows = if table_name == "secrets" {
                let (secret_rows, report) =
                    self.rekey_secret_rows(rows, &transport_key, cancellation)?;
                rekey_report = Some(report);
                secret_rows
            } else {
                rows
            };
            row_counts.insert(table_name.to_string(), rows.len());
            batches.push(TrustedTableBatch {
                table_name: table_name.to_string(),
                rows,
            });
        }
        reader.close().await?;

        TrustedTargetWriter
            .rebuild_current_database(portable_database_path, &batches)
            .await?;

        Ok(PortableExportArtifact {
            portable_database_path: portable_database_path.to_path_buf(),
            snapshot_created_at: String::new(),
            row_counts,
            rekey_report: rekey_report.unwrap_or(SecretRekeyReport {
                from_key_id: self.source_keys.active_key_id().as_str().to_string(),
                to_key_id: transport_key.key_id().to_string(),
                included_rows: 0,
                dropped_rows: 0,
                reset_rows: 0,
                code: "ok",
            }),
            transport_key,
        })
    }

    fn rekey_secret_rows(
        &self,
        rows: Vec<PortableRow>,
        transport_key: &TransportSecretKey,
        cancellation: Option<&CancellationToken>,
    ) -> DataMigrationResult<(Vec<PortableRow>, SecretRekeyReport)> {
        let mut templates = BTreeMap::new();
        let mut secrets = Vec::with_capacity(rows.len());
        for row in rows {
            let secret = encrypted_secret_from_portable_row(&row)
                .map_err(PortableMigrationValidationError::from)?;
            templates.insert(secret.id.clone(), row);
            secrets.push(secret);
        }

        let service = SecretRekeyService::new(
            self.source_keys.clone(),
            transport_key.resolver(),
            CURRENT_SECRET_ENCRYPTION_VERSION,
        );
        let mut writer = BufferedSecretRekeyWriter::create_new();
        let report = service.rekey(
            secrets,
            &SecretRekeyPolicy::include_all(),
            &mut writer,
            cancellation,
        )?;

        let rows = writer
            .rows()
            .iter()
            .map(|secret| {
                let template = templates
                    .get(&secret.id)
                    .expect("rekey writer preserves source secret IDs");
                portable_row_from_encrypted_secret(template, secret)
            })
            .collect();
        Ok((rows, report))
    }
}

fn unique_snapshot_path(directory: &Path) -> PathBuf {
    directory.join(format!(
        "portable-export-{}.snapshot.sqlite3",
        uuid::Uuid::now_v7()
    ))
}

fn cleanup_unpublished_target(path: &Path) -> DataMigrationResult<()> {
    for candidate in [
        path.to_path_buf(),
        sqlite_sidecar_path(path, "wal"),
        sqlite_sidecar_path(path, "shm"),
    ] {
        if candidate.exists() {
            fs::remove_file(candidate).map_err(|_| DataMigrationError::CleanupFailed)?;
        }
    }
    Ok(())
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    let mut file_name = path
        .file_name()
        .map(std::ffi::OsString::from)
        .unwrap_or_else(|| std::ffi::OsString::from("portable.sqlite3"));
    file_name.push(format!("-{suffix}"));
    path.with_file_name(file_name)
}

#[cfg(test)]
mod tests {
    use base64::{engine::general_purpose, Engine as _};
    use sqlx::{Connection, SqliteConnection};

    use crate::{
        models::secrets::canonical_secret_aad,
        services::{
            portable_migration::snapshot::PortableSnapshotError,
            secrets::{crypto, DeviceKeyResolver},
        },
    };

    use super::*;

    #[tokio::test]
    async fn export_builds_snapshot_rekeys_secrets_and_omits_later_writes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        let output = directory.path().join("portable.sqlite3");
        let working = directory.path().join("working");
        let source_material = [7_u8; 32];
        crate::persistence::migrations::initialize_v2_database(&source)
            .await
            .expect("source db");
        seed_secret_database(&source, source_material)
            .await
            .expect("seed");

        let service = DataMigrationExportService::new(
            DataMaintenanceCoordinator::new(),
            DeviceKeyResolver::for_test(source_material),
        );
        let artifact = service
            .export_portable_sqlite(
                PortableExportRequest {
                    source_database_path: source.clone(),
                    portable_database_path: output.clone(),
                    working_directory: working,
                    include_history: false,
                },
                None,
            )
            .await
            .expect("export");

        mutate_source_after_export(&source)
            .await
            .expect("mutate source");

        assert_eq!(artifact.rekey_report.included_rows, 1);
        assert!(artifact.transport_key.key_id().starts_with("transport:"));
        assert_eq!(artifact.row_counts.get("secrets"), Some(&1));
        assert!(!artifact.snapshot_created_at.is_empty());
        assert_eq!(artifact.portable_database_path, output);

        let mut exported =
            SqliteConnection::connect(&format!("sqlite:{}?mode=ro", output.display()))
                .await
                .expect("open output");
        let key_id: String = sqlx::query_scalar("SELECT key_id FROM secrets WHERE id = 'secret-1'")
            .fetch_one(&mut exported)
            .await
            .expect("key id");
        let station_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM stations WHERE id = 'station-after-export'")
                .fetch_one(&mut exported)
                .await
                .expect("station count");
        let ciphertext: Vec<u8> =
            sqlx::query_scalar("SELECT ciphertext FROM secrets WHERE id = 'secret-1'")
                .fetch_one(&mut exported)
                .await
                .expect("ciphertext");
        exported.close().await.expect("close");

        assert_eq!(key_id, artifact.transport_key.key_id());
        assert_ne!(ciphertext, vec![9, 9, 9]);
        assert_eq!(station_count, 0);
    }

    #[tokio::test]
    async fn export_fails_closed_when_source_secret_cannot_decrypt() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        let output = directory.path().join("portable.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&source)
            .await
            .expect("source db");
        seed_secret_database(&source, [1_u8; 32])
            .await
            .expect("seed");

        let service = DataMigrationExportService::new(
            DataMaintenanceCoordinator::new(),
            DeviceKeyResolver::for_test([2_u8; 32]),
        );
        let error = service
            .export_portable_sqlite(
                PortableExportRequest {
                    source_database_path: source,
                    portable_database_path: output.clone(),
                    working_directory: directory.path().join("working"),
                    include_history: false,
                },
                None,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, DataMigrationError::SecretRekey(_)));
        assert!(!output.exists(), "failed export must not leave target DB");
    }

    #[tokio::test]
    async fn export_honors_cancellation_before_snapshot() {
        let directory = tempfile::tempdir().expect("tempdir");
        let token = CancellationToken::new();
        token.cancel();
        let service = DataMigrationExportService::new(
            DataMaintenanceCoordinator::new(),
            DeviceKeyResolver::for_test([1_u8; 32]),
        );

        let error = service
            .export_portable_sqlite(
                PortableExportRequest {
                    source_database_path: directory.path().join("missing.sqlite3"),
                    portable_database_path: directory.path().join("portable.sqlite3"),
                    working_directory: directory.path().join("working"),
                    include_history: false,
                },
                Some(&token),
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            DataMigrationError::Snapshot(PortableSnapshotError::Cancelled)
        ));
    }

    async fn seed_secret_database(path: &Path, material: [u8; 32]) -> Result<(), sqlx::Error> {
        let mut connection =
            SqliteConnection::connect(&format!("sqlite:{}", path.display())).await?;
        let aad = canonical_secret_aad("station_key", "key-1", "api_key", 1);
        let payload = crypto::encrypt_secret(&material, "sk-p8-secret-plaintext-canary", &aad)
            .expect("encrypt");
        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, endpoint_revision,
                api_key, enabled, priority, created_at, updated_at
            ) VALUES (
                'station-1', 'Station', 'newapi', 'https://example.test',
                'https://example.test/v1', 1, '', 1, 0, '1', '1'
            )
            "#,
        )
        .execute(&mut connection)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO secrets (
                id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                created_at, updated_at, key_id, encryption_version, value_hash
            ) VALUES (
                'secret-1', 'station_key', 'key-1', 'api_key', 'sk-...nary',
                ?1, ?2, '1', '1', 'test-device-key', 1, ?3
            )
            "#,
        )
        .bind(
            general_purpose::STANDARD
                .decode(payload.ciphertext)
                .expect("ciphertext"),
        )
        .bind(
            general_purpose::STANDARD
                .decode(payload.nonce)
                .expect("nonce"),
        )
        .bind(payload.value_hash)
        .execute(&mut connection)
        .await?;
        sqlx::query(
            r#"
            INSERT INTO station_keys (
                id, station_id, name, api_key, api_key_secret_id, enabled, priority,
                max_concurrency, created_at, updated_at
            ) VALUES (
                'key-1', 'station-1', 'Key', '', 'secret-1', 1, 0, 3, '1', '1'
            )
            "#,
        )
        .execute(&mut connection)
        .await?;
        connection.close().await
    }

    async fn mutate_source_after_export(path: &Path) -> Result<(), sqlx::Error> {
        let mut connection =
            SqliteConnection::connect(&format!("sqlite:{}", path.display())).await?;
        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, endpoint_revision,
                api_key, enabled, priority, created_at, updated_at
            ) VALUES (
                'station-after-export', 'Late Station', 'newapi', 'https://late.test',
                'https://late.test/v1', 1, '', 1, 0, '2', '2'
            )
            "#,
        )
        .execute(&mut connection)
        .await?;
        connection.close().await
    }
}
