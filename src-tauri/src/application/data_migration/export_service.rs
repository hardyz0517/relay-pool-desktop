use std::{
    collections::BTreeMap,
    fmt, fs,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sha2::{Digest, Sha256};
use tokio_util::sync::CancellationToken;
use zeroize::Zeroizing;

use crate::{
    application::data_maintenance::{DataMaintenanceActivity, DataMaintenanceCoordinator},
    services::{
        data_store::atomic_file::{
            ApprovedLeaf, LocalAtomicFileAdapter, PublishEvidence, PublishMode,
        },
        portable_migration::{
            age_envelope::{AgeEnvelopeError, AgeEnvelopeErrorCode, AgeEnvelopeOptions},
            format::{
                build_manifest_v1, PortableMigrationManifest, PortableMigrationManifestInput,
                TransportKeyMaterial,
            },
            schema_reader::{ordered_import_tables_v1, PortableSchemaReader},
            snapshot::{create_consistent_snapshot, remove_snapshot_file},
            staging::{
                publish_verified_partial, remove_file_if_exists, self_test_encrypted_package,
                write_encrypted_partial, PortablePackageSelfTestReport,
            },
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

pub(crate) struct PortablePackageExportRequest {
    pub(crate) source_database_path: PathBuf,
    pub(crate) package_path: PathBuf,
    pub(crate) working_directory: PathBuf,
    pub(crate) include_history: bool,
    pub(crate) overwrite_existing: bool,
    pub(crate) passphrase: Zeroizing<String>,
    pub(crate) passphrase_confirmation: Zeroizing<String>,
}

impl fmt::Debug for PortablePackageExportRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PortablePackageExportRequest")
            .field("source_database_path", &self.source_database_path)
            .field("package_path", &self.package_path)
            .field("working_directory", &self.working_directory)
            .field("include_history", &self.include_history)
            .field("overwrite_existing", &self.overwrite_existing)
            .field("passphrase", &"<redacted>")
            .field("passphrase_confirmation", &"<redacted>")
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct PortablePackageExportArtifact {
    pub(crate) package_path: PathBuf,
    pub(crate) export_id: String,
    pub(crate) package_size_bytes: u64,
    pub(crate) publish_evidence: PublishEvidence,
    pub(crate) manifest: PortableMigrationManifest,
    pub(crate) pre_publish_self_test: PortablePackageSelfTestReport,
    pub(crate) published_self_test: PortablePackageSelfTestReport,
    pub(crate) row_counts: BTreeMap<String, usize>,
    pub(crate) rekey_report: SecretRekeyReport,
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

    pub(crate) async fn export_portable_package(
        &self,
        request: PortablePackageExportRequest,
        cancellation: Option<&CancellationToken>,
    ) -> DataMigrationResult<PortablePackageExportArtifact> {
        self.export_portable_package_with_options(
            request,
            cancellation,
            AgeEnvelopeOptions::CURRENT,
        )
        .await
    }

    async fn export_portable_package_with_options(
        &self,
        request: PortablePackageExportRequest,
        cancellation: Option<&CancellationToken>,
        age_options: AgeEnvelopeOptions,
    ) -> DataMigrationResult<PortablePackageExportArtifact> {
        if request.passphrase.as_str() != request.passphrase_confirmation.as_str() {
            return Err(DataMigrationError::PassphraseConfirmationMismatch);
        }
        age_options
            .limits
            .validate_passphrase(request.passphrase.as_str())
            .map_err(|_| {
                DataMigrationError::Envelope(AgeEnvelopeError::new(
                    AgeEnvelopeErrorCode::LimitExceeded,
                ))
            })?;
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(DataMigrationError::Snapshot(
                crate::services::portable_migration::snapshot::PortableSnapshotError::Cancelled,
            ));
        }

        let target_parent = request
            .package_path
            .parent()
            .ok_or(DataMigrationError::Validation(
                PortableMigrationValidationError::AtomicPublish,
            ))?;
        let target_leaf =
            request
                .package_path
                .file_name()
                .ok_or(DataMigrationError::Validation(
                    PortableMigrationValidationError::AtomicPublish,
                ))?;
        let approved_target = ApprovedLeaf::approve(target_parent, target_leaf).map_err(|_| {
            DataMigrationError::Validation(PortableMigrationValidationError::AtomicPublish)
        })?;
        if approved_target.path().exists() && !request.overwrite_existing {
            return Err(DataMigrationError::TargetExists);
        }
        fs::create_dir_all(&request.working_directory)
            .map_err(|_| DataMigrationError::CleanupFailed)?;

        let export_id = uuid::Uuid::now_v7().to_string();
        let portable_sqlite_path =
            unique_portable_sqlite_path(&request.working_directory, &export_id);
        let sqlite_artifact_result = self
            .export_portable_sqlite(
                PortableExportRequest {
                    source_database_path: request.source_database_path.clone(),
                    portable_database_path: portable_sqlite_path.clone(),
                    working_directory: request.working_directory.clone(),
                    include_history: request.include_history,
                },
                cancellation,
            )
            .await;
        let sqlite_artifact = match sqlite_artifact_result {
            Ok(artifact) => artifact,
            Err(error) => {
                cleanup_unpublished_target(&portable_sqlite_path)?;
                return Err(error);
            }
        };

        let package_result = self
            .publish_package_from_portable_sqlite(
                &request,
                &approved_target,
                &export_id,
                &sqlite_artifact,
                cancellation,
                age_options,
            )
            .await;
        let sqlite_cleanup = cleanup_unpublished_target(&portable_sqlite_path);
        match (package_result, sqlite_cleanup) {
            (Ok(artifact), Ok(())) => Ok(artifact),
            (Err(error), _) => Err(error),
            (Ok(_), Err(error)) => Err(error),
        }
    }

    async fn publish_package_from_portable_sqlite(
        &self,
        request: &PortablePackageExportRequest,
        approved_target: &ApprovedLeaf,
        export_id: &str,
        sqlite_artifact: &PortableExportArtifact,
        cancellation: Option<&CancellationToken>,
        age_options: AgeEnvelopeOptions,
    ) -> DataMigrationResult<PortablePackageExportArtifact> {
        if cancellation.is_some_and(CancellationToken::is_cancelled) {
            return Err(DataMigrationError::Snapshot(
                crate::services::portable_migration::snapshot::PortableSnapshotError::Cancelled,
            ));
        }
        let (sqlite_size_bytes, sqlite_sha256) =
            file_len_and_sha256(&sqlite_artifact.portable_database_path)?;
        let record_counts = sqlite_artifact
            .row_counts
            .iter()
            .map(|(table, count)| {
                Ok((
                    table.clone(),
                    u64::try_from(*count).map_err(|_| {
                        DataMigrationError::Validation(
                            PortableMigrationValidationError::UnsupportedSchema,
                        )
                    })?,
                ))
            })
            .collect::<DataMigrationResult<BTreeMap<_, _>>>()?;
        let expected_keys = ordered_import_tables_v1();
        let manifest = build_manifest_v1(
            PortableMigrationManifestInput {
                export_id: export_id.to_string(),
                created_at: Utc::now(),
                source_app_version: env!("CARGO_PKG_VERSION").to_string(),
                source_platform: std::env::consts::OS.to_string(),
                minimum_importer_version: env!("CARGO_PKG_VERSION").to_string(),
                transport_key_id: sqlite_artifact.transport_key.key_id().to_string(),
                include_history: request.include_history,
                record_counts,
                sqlite_size_bytes,
                sqlite_sha256,
            },
            &expected_keys,
            age_options.limits,
        )
        .map_err(|_| {
            DataMigrationError::Validation(PortableMigrationValidationError::UnsupportedSchema)
        })?;
        let transport_material = sqlite_artifact
            .transport_key
            .with_key(|bytes| TransportKeyMaterial::from_bytes(*bytes))
            .map_err(|_| DataMigrationError::TransportKeyUnavailable)?;
        let sqlite_file = File::open(&sqlite_artifact.portable_database_path).map_err(|_| {
            DataMigrationError::Validation(PortableMigrationValidationError::OpenFailed)
        })?;
        let partial_path = write_encrypted_partial(
            approved_target,
            export_id,
            request.passphrase.as_str(),
            &manifest,
            &transport_material,
            sqlite_file,
            &expected_keys,
            age_options,
        )?;

        let package_result = async {
            let pre_publish_self_test = self_test_encrypted_package(
                &partial_path,
                &request.working_directory,
                request.passphrase.as_str(),
                &expected_keys,
                age_options,
            )
            .await?;
            let mode = if request.overwrite_existing {
                PublishMode::ReplaceExisting
            } else {
                PublishMode::CreateNew
            };
            let publisher = LocalAtomicFileAdapter;
            let publish_evidence =
                publish_verified_partial(&publisher, &partial_path, approved_target, mode)?;
            let published_self_test = self_test_encrypted_package(
                &publish_evidence.target,
                &request.working_directory,
                request.passphrase.as_str(),
                &expected_keys,
                age_options,
            )
            .await?;
            let package_size_bytes = fs::metadata(&publish_evidence.target)
                .map_err(|_| {
                    DataMigrationError::Validation(PortableMigrationValidationError::OpenFailed)
                })?
                .len();
            Ok(PortablePackageExportArtifact {
                package_path: publish_evidence.target.clone(),
                export_id: export_id.to_string(),
                package_size_bytes,
                publish_evidence,
                manifest,
                pre_publish_self_test,
                published_self_test,
                row_counts: sqlite_artifact.row_counts.clone(),
                rekey_report: sqlite_artifact.rekey_report.clone(),
            })
        }
        .await;

        match package_result {
            Ok(artifact) => Ok(artifact),
            Err(error) => {
                remove_file_if_exists(&partial_path)?;
                Err(error)
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

fn unique_portable_sqlite_path(directory: &Path, export_id: &str) -> PathBuf {
    directory.join(format!("portable-export-{export_id}.sqlite3"))
}

fn file_len_and_sha256(path: &Path) -> DataMigrationResult<(u64, [u8; 32])> {
    let mut file = File::open(path).map_err(|_| {
        DataMigrationError::Validation(PortableMigrationValidationError::OpenFailed)
    })?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|_| {
            DataMigrationError::Validation(PortableMigrationValidationError::OpenFailed)
        })?;
        if read == 0 {
            break;
        }
        total = total
            .checked_add(read as u64)
            .ok_or(DataMigrationError::Validation(
                PortableMigrationValidationError::UnsupportedSchema,
            ))?;
        hasher.update(&buffer[..read]);
    }
    Ok((total, hasher.finalize().into()))
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
    use zeroize::Zeroizing;

    use crate::{
        models::secrets::canonical_secret_aad,
        services::{
            portable_migration::age_envelope::AgeEnvelopeOptions,
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

    #[tokio::test]
    async fn portable_export_package_writes_self_verified_age_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        let package = directory.path().join("portable.rpd-move");
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
            .export_portable_package_with_options(
                package_request(source, package.clone(), working, false, "move-passphrase"),
                None,
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .expect("package export");

        assert_eq!(artifact.package_path, package);
        assert_eq!(artifact.manifest, artifact.published_self_test.manifest);
        assert_eq!(
            artifact.pre_publish_self_test.row_counts,
            artifact.published_self_test.row_counts
        );
        assert_eq!(artifact.row_counts.get("secrets"), Some(&1));
        assert!(artifact.package_size_bytes > 0);
        assert_ne!(
            &std::fs::read(&artifact.package_path).expect("package")[..8],
            b"SQLite f"
        );
        let package_bytes = std::fs::read(&artifact.package_path).expect("package bytes");
        assert!(
            !contains_bytes(&package_bytes, &source_material),
            "portable package must not contain source device key bytes"
        );
        assert!(
            !contains_bytes(&package_bytes, b"sk-p8-secret-plaintext-canary"),
            "portable package must not contain source plaintext secret canary"
        );
        assert!(
            !working_tree_contains_bytes(
                directory.path().join("working").as_path(),
                &source_material
            ),
            "working directory must not retain source device key bytes"
        );
        assert!(
            std::fs::read_dir(directory.path().join("working"))
                .expect("working dir")
                .all(|entry| !entry
                    .expect("entry")
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".sqlite3")),
            "plaintext staging sqlite must be cleaned after publish"
        );
    }

    #[tokio::test]
    async fn portable_export_package_rejects_wrong_confirmation_before_writes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        let package = directory.path().join("portable.rpd-move");
        let service = DataMigrationExportService::new(
            DataMaintenanceCoordinator::new(),
            DeviceKeyResolver::for_test([1_u8; 32]),
        );

        let error = service
            .export_portable_package_with_options(
                PortablePackageExportRequest {
                    source_database_path: source,
                    package_path: package.clone(),
                    working_directory: directory.path().join("working"),
                    include_history: false,
                    overwrite_existing: false,
                    passphrase: Zeroizing::new("first".to_string()),
                    passphrase_confirmation: Zeroizing::new("second".to_string()),
                },
                None,
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();

        assert!(matches!(
            error,
            DataMigrationError::PassphraseConfirmationMismatch
        ));
        assert!(!package.exists());
    }

    #[tokio::test]
    async fn portable_export_package_preserves_existing_without_overwrite() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        let package = directory.path().join("portable.rpd-move");
        std::fs::write(&package, b"old-package").expect("old");
        let service = DataMigrationExportService::new(
            DataMaintenanceCoordinator::new(),
            DeviceKeyResolver::for_test([1_u8; 32]),
        );

        let error = service
            .export_portable_package_with_options(
                package_request(
                    source,
                    package.clone(),
                    directory.path().join("working"),
                    false,
                    "p",
                ),
                None,
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();

        assert!(matches!(error, DataMigrationError::TargetExists));
        assert_eq!(std::fs::read(package).expect("read old"), b"old-package");
    }

    fn package_request(
        source: PathBuf,
        package: PathBuf,
        working: PathBuf,
        overwrite_existing: bool,
        passphrase: &str,
    ) -> PortablePackageExportRequest {
        PortablePackageExportRequest {
            source_database_path: source,
            package_path: package,
            working_directory: working,
            include_history: false,
            overwrite_existing,
            passphrase: Zeroizing::new(passphrase.to_string()),
            passphrase_confirmation: Zeroizing::new(passphrase.to_string()),
        }
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

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        !needle.is_empty()
            && haystack
                .windows(needle.len())
                .any(|window| window == needle)
    }

    fn working_tree_contains_bytes(root: &Path, needle: &[u8]) -> bool {
        if !root.exists() {
            return false;
        }
        std::fs::read_dir(root)
            .expect("read working tree")
            .any(|entry| {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    working_tree_contains_bytes(&path, needle)
                } else {
                    std::fs::read(&path)
                        .map(|bytes| contains_bytes(&bytes, needle))
                        .unwrap_or(false)
                }
            })
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
