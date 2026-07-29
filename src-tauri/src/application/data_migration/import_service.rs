use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zeroize::Zeroizing;

use crate::{
    application::data_maintenance::{DataMaintenanceActivity, DataMaintenanceCoordinator},
    services::portable_migration::{
        age_envelope::{AgeEnvelopeError, AgeEnvelopeOptions},
        inspection_registry::{
            ImportInspectionHandle, ImportInspectionRegistry, ImportInspectionSummary,
            ImportPreparationLease,
        },
        path_tokens::{PathTokenError, PathTokenId, PathTokenRegistry},
        schema_reader::ordered_import_tables_v1,
        staging::{stage_and_verify_import_package, PortablePackageStagingError},
    },
};

const FAILURE_BACKOFF_THRESHOLD: u8 = 5;
const FAILURE_BACKOFF: Duration = Duration::from_secs(60);

#[derive(Clone, Debug)]
pub(crate) struct DataMigrationImportService {
    maintenance: DataMaintenanceCoordinator,
    path_tokens: PathTokenRegistry,
    inspections: ImportInspectionRegistry,
    failures: Arc<Mutex<HashMap<String, ImportFailureState>>>,
}

impl DataMigrationImportService {
    pub(crate) fn new(
        maintenance: DataMaintenanceCoordinator,
        path_tokens: PathTokenRegistry,
        inspections: ImportInspectionRegistry,
    ) -> Self {
        Self {
            maintenance,
            path_tokens,
            inspections,
            failures: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub(crate) async fn inspect_portable_package(
        &self,
        request: PortableImportInspectionRequest,
    ) -> Result<ImportInspectionHandle, DataMigrationImportError> {
        self.inspect_portable_package_with_options(request, AgeEnvelopeOptions::CURRENT)
            .await
    }

    async fn inspect_portable_package_with_options(
        &self,
        request: PortableImportInspectionRequest,
        options: AgeEnvelopeOptions,
    ) -> Result<ImportInspectionHandle, DataMigrationImportError> {
        self.ensure_not_backing_off(&request.rate_limit_key, request.now)?;
        let result = self.inspect_once(&request, options).await;
        match result {
            Ok(handle) => {
                self.record_success(&request.rate_limit_key);
                Ok(handle)
            }
            Err(error) => {
                self.record_failure(&request.rate_limit_key, request.now);
                Err(error)
            }
        }
    }

    async fn inspect_once(
        &self,
        request: &PortableImportInspectionRequest,
        options: AgeEnvelopeOptions,
    ) -> Result<ImportInspectionHandle, DataMigrationImportError> {
        let _lease = self
            .maintenance
            .begin(DataMaintenanceActivity::InspectImport)?;
        let import_lease = self
            .path_tokens
            .consume_import(&request.import_token, request.now)?;
        let staged = stage_and_verify_import_package(
            import_lease.file,
            &request.scratch_directory,
            request.passphrase.as_str(),
            &ordered_import_tables_v1(),
            options,
        )
        .await?;
        let summary = ImportInspectionSummary {
            export_id: staged.manifest.export_id.clone(),
            created_at: staged.manifest.created_at.clone(),
            source_app_version: staged.manifest.source_app_version.clone(),
            source_platform: staged.manifest.source_platform.clone(),
            included_categories: staged.manifest.included_categories.clone(),
            include_history: staged
                .manifest
                .included_categories
                .iter()
                .any(|category| category == "history"),
            record_counts: staged
                .row_counts
                .iter()
                .map(|(table, count)| (table.clone(), *count))
                .collect(),
            sqlite_size_bytes: staged.manifest.sqlite_size_bytes,
        };
        let preparation = ImportPreparationLease {
            source_identity: import_lease.identity,
            staging_path: staged.staging_path,
            staging_identity: staged.staging_identity,
            reader_kind: staged.reader_kind,
            manifest: staged.manifest,
            sqlite_sha256: staged.sqlite_sha256,
            transport_key: staged.transport_key,
        };
        Ok(self.inspections.register(preparation, summary, request.now))
    }

    fn ensure_not_backing_off(
        &self,
        key: &str,
        now: Instant,
    ) -> Result<(), DataMigrationImportError> {
        let failures = self.failures.lock().expect("import failure mutex");
        let Some(state) = failures.get(key) else {
            return Ok(());
        };
        if state.consecutive_failures >= FAILURE_BACKOFF_THRESHOLD
            && state
                .blocked_until
                .is_some_and(|blocked_until| now < blocked_until)
        {
            return Err(DataMigrationImportError::TemporarilyBlocked);
        }
        Ok(())
    }

    fn record_success(&self, key: &str) {
        self.failures
            .lock()
            .expect("import failure mutex")
            .remove(key);
    }

    fn record_failure(&self, key: &str, now: Instant) {
        let mut failures = self.failures.lock().expect("import failure mutex");
        let state = failures.entry(key.to_string()).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);
        if state.consecutive_failures >= FAILURE_BACKOFF_THRESHOLD {
            state.blocked_until = Some(now + FAILURE_BACKOFF);
        }
    }
}

#[derive(Debug)]
pub(crate) struct PortableImportInspectionRequest {
    pub(crate) import_token: PathTokenId,
    pub(crate) scratch_directory: PathBuf,
    pub(crate) passphrase: Zeroizing<String>,
    pub(crate) rate_limit_key: String,
    pub(crate) now: Instant,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ImportFailureState {
    consecutive_failures: u8,
    blocked_until: Option<Instant>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DataMigrationImportError {
    #[error("data migration maintenance coordinator rejected inspection")]
    Maintenance(#[from] crate::application::data_maintenance::DataMaintenanceError),
    #[error("data migration import path token failed")]
    PathToken(#[from] PathTokenError),
    #[error("data migration package staging failed")]
    Package(#[from] PortablePackageStagingError),
    #[error("data migration package envelope failed")]
    Envelope(#[from] AgeEnvelopeError),
    #[error("data migration import inspection is temporarily blocked")]
    TemporarilyBlocked,
}

#[cfg(test)]
mod tests {
    use std::{
        collections::BTreeMap,
        fs::File,
        io::{Cursor, Write},
        path::Path,
    };

    use base64::{engine::general_purpose, Engine as _};
    use chrono::Utc;
    use sha2::{Digest, Sha256};
    use sqlx::{Connection, Row, SqliteConnection};

    use super::*;
    use crate::{
        application::data_migration::export_service::{
            DataMigrationExportService, PortableExportRequest,
        },
        services::{
            data_store::file_identity::identity_for_path,
            portable_migration::{
                age_envelope::encrypt_framed_payload,
                format::{
                    build_manifest_v1, PortableMigrationManifest, PortableMigrationManifestInput,
                    TransportKeyMaterial,
                },
                schema_reader::ordered_import_tables_v1,
                validate::PortableMigrationValidationError,
            },
            secrets::DeviceKeyResolver,
        },
    };

    #[tokio::test]
    async fn portable_import_inspection_registers_verified_package_without_touching_active_db() {
        let directory = tempfile::tempdir().expect("tempdir");
        let active = directory.path().join("active.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&active)
            .await
            .expect("active db");
        let active_before = identity_for_path(&active).expect("active before");
        let package = valid_package(directory.path(), "move-passphrase").await;
        let service = service();
        let token = service
            .path_tokens
            .approve_import_path(&package, Instant::now())
            .expect("token");

        let handle = service
            .inspect_portable_package_with_options(
                request(
                    token.id,
                    directory.path().join("scratch"),
                    "move-passphrase",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .expect("inspection");

        assert_eq!(handle.summary.source_platform, std::env::consts::OS);
        assert_eq!(handle.summary.include_history, false);
        assert!(handle
            .summary
            .record_counts
            .iter()
            .any(|(table, _)| table == "settings"));
        let lease = service
            .inspections
            .consume(&handle.id, Instant::now())
            .expect("consume preparation");
        assert!(lease.staging_path.exists());
        lease
            .transport_key
            .with_bytes(|bytes| assert_ne!(bytes, &[0; 32]));
        assert_eq!(
            identity_for_path(&active).expect("active after"),
            active_before
        );
    }

    #[tokio::test]
    async fn malicious_portable_package_wrong_password_truncation_and_toctou_fail_closed() {
        let directory = tempfile::tempdir().expect("tempdir");
        let active = directory.path().join("active.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&active)
            .await
            .expect("active db");
        let active_before = identity_for_path(&active).expect("active before");

        let wrong_password_package = valid_package(directory.path(), "right-passphrase").await;
        let service = service();
        let wrong_token = service
            .path_tokens
            .approve_import_path(&wrong_password_package, Instant::now())
            .expect("wrong token");
        let wrong = service
            .inspect_portable_package_with_options(
                request(
                    wrong_token.id,
                    directory.path().join("scratch-wrong"),
                    "wrong",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            wrong,
            DataMigrationImportError::Package(PortablePackageStagingError::Envelope(_))
        ));

        let truncated = directory.path().join("truncated.rpd-move");
        let bytes = std::fs::read(&wrong_password_package).expect("read package");
        std::fs::write(&truncated, &bytes[..bytes.len().saturating_sub(8)])
            .expect("write truncated");
        let truncated_token = service
            .path_tokens
            .approve_import_path(&truncated, Instant::now())
            .expect("truncated token");
        let error = service
            .inspect_portable_package_with_options(
                request(
                    truncated_token.id,
                    directory.path().join("scratch-truncated"),
                    "right-passphrase",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DataMigrationImportError::Package(PortablePackageStagingError::Envelope(_))
        ));

        let toctou = directory.path().join("toctou.rpd-move");
        std::fs::write(&toctou, b"first").expect("first");
        let toctou_token = service
            .path_tokens
            .approve_import_path(&toctou, Instant::now())
            .expect("toctou token");
        match std::fs::write(&toctou, b"second") {
            Ok(()) => {
                let error = service
                    .inspect_portable_package_with_options(
                        request(
                            toctou_token.id,
                            directory.path().join("scratch-toctou"),
                            "p",
                        ),
                        AgeEnvelopeOptions::TEST_FAST,
                    )
                    .await
                    .unwrap_err();
                assert!(matches!(
                    error,
                    DataMigrationImportError::PathToken(PathTokenError::SelectedFileChanged)
                ));
            }
            Err(_) => {
                let error = service
                    .inspect_portable_package_with_options(
                        request(
                            toctou_token.id,
                            directory.path().join("scratch-toctou-locked"),
                            "p",
                        ),
                        AgeEnvelopeOptions::TEST_FAST,
                    )
                    .await
                    .unwrap_err();
                assert!(matches!(
                    error,
                    DataMigrationImportError::Package(PortablePackageStagingError::Envelope(_))
                ));
            }
        }
        assert_eq!(
            identity_for_path(&active).expect("active after"),
            active_before
        );
    }

    #[tokio::test]
    async fn malicious_portable_package_rejects_non_sqlite_and_schema_object_attacks() {
        let directory = tempfile::tempdir().expect("tempdir");
        let service = service();

        let non_sqlite = encrypted_package_from_bytes(
            directory.path(),
            b"not sqlite",
            "passphrase",
            BTreeMap::from_iter(
                ordered_import_tables_v1()
                    .into_iter()
                    .map(|table| (table.to_string(), 0_u64)),
            ),
        );
        let token = service
            .path_tokens
            .approve_import_path(&non_sqlite, Instant::now())
            .expect("non sqlite token");
        let error = service
            .inspect_portable_package_with_options(
                request(
                    token.id,
                    directory.path().join("scratch-non-sqlite"),
                    "passphrase",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DataMigrationImportError::Package(PortablePackageStagingError::Validation(
                PortableMigrationValidationError::OpenFailed
                    | PortableMigrationValidationError::QuickCheckFailed
                    | PortableMigrationValidationError::Sql
            ))
        ));

        let attacked_db = directory.path().join("attacked.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&attacked_db)
            .await
            .expect("attacked db");
        execute_sql(
            &attacked_db,
            "CREATE TRIGGER injected_trigger AFTER INSERT ON settings BEGIN SELECT 1; END",
        )
        .await;
        let counts = record_counts(&attacked_db).await;
        let attacked =
            encrypted_package_from_file(directory.path(), &attacked_db, "passphrase", counts);
        let token = service
            .path_tokens
            .approve_import_path(&attacked, Instant::now())
            .expect("attacked token");
        let error = service
            .inspect_portable_package_with_options(
                request(
                    token.id,
                    directory.path().join("scratch-attacked"),
                    "passphrase",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DataMigrationImportError::Package(PortablePackageStagingError::Validation(
                PortableMigrationValidationError::UnsupportedSchemaObject
                    | PortableMigrationValidationError::CatalogDrift(_)
            ))
        ));
    }

    #[tokio::test]
    async fn portable_import_inspection_backs_off_after_five_consecutive_failures() {
        let directory = tempfile::tempdir().expect("tempdir");
        let service = service();

        for index in 0..5 {
            let path = directory.path().join(format!("bad-{index}.rpd-move"));
            std::fs::write(&path, b"not an age package").expect("bad package");
            let token = service
                .path_tokens
                .approve_import_path(&path, Instant::now())
                .expect("token");
            service
                .inspect_portable_package_with_options(
                    request(
                        token.id,
                        directory.path().join(format!("scratch-{index}")),
                        "p",
                    ),
                    AgeEnvelopeOptions::TEST_FAST,
                )
                .await
                .expect_err("bad package rejected");
        }

        let valid = valid_package(directory.path(), "passphrase").await;
        let token = service
            .path_tokens
            .approve_import_path(&valid, Instant::now())
            .expect("valid token");
        let blocked = service
            .inspect_portable_package_with_options(
                request(
                    token.id,
                    directory.path().join("scratch-blocked"),
                    "passphrase",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .unwrap_err();
        assert!(matches!(
            blocked,
            DataMigrationImportError::TemporarilyBlocked
        ));
    }

    fn service() -> DataMigrationImportService {
        DataMigrationImportService::new(
            DataMaintenanceCoordinator::new(),
            PathTokenRegistry::new(),
            ImportInspectionRegistry::new(),
        )
    }

    fn request(
        import_token: PathTokenId,
        scratch_directory: PathBuf,
        passphrase: &str,
    ) -> PortableImportInspectionRequest {
        PortableImportInspectionRequest {
            import_token,
            scratch_directory,
            passphrase: Zeroizing::new(passphrase.to_string()),
            rate_limit_key: "test-reader".to_string(),
            now: Instant::now(),
        }
    }

    async fn valid_package(directory: &Path, passphrase: &str) -> PathBuf {
        let source = directory.join(format!("source-{}.sqlite3", uuid::Uuid::now_v7()));
        let package = directory.join(format!("portable-{}.rpd-move", uuid::Uuid::now_v7()));
        let portable = directory.join(format!("portable-{}.sqlite3", uuid::Uuid::now_v7()));
        let working = directory.join(format!("export-{}", uuid::Uuid::now_v7()));
        crate::persistence::migrations::initialize_v2_database(&source)
            .await
            .expect("source db");
        let service = DataMigrationExportService::new(
            DataMaintenanceCoordinator::new(),
            DeviceKeyResolver::for_test([7_u8; 32]),
        );
        let artifact = service
            .export_portable_sqlite(
                PortableExportRequest {
                    source_database_path: source.clone(),
                    portable_database_path: portable.clone(),
                    working_directory: working,
                    include_history: false,
                },
                None,
            )
            .await
            .expect("portable sqlite export");
        let sqlite_bytes = std::fs::read(&portable).expect("portable sqlite bytes");
        let record_counts = artifact
            .row_counts
            .iter()
            .map(|(table, count)| (table.clone(), *count as u64))
            .collect();
        let expected_keys = ordered_import_tables_v1();
        let manifest = build_manifest_v1(
            PortableMigrationManifestInput {
                export_id: uuid::Uuid::now_v7().to_string(),
                created_at: Utc::now(),
                source_app_version: env!("CARGO_PKG_VERSION").to_string(),
                source_platform: std::env::consts::OS.to_string(),
                minimum_importer_version: env!("CARGO_PKG_VERSION").to_string(),
                transport_key_id: artifact.transport_key.key_id().to_string(),
                include_history: false,
                record_counts,
                sqlite_size_bytes: sqlite_bytes.len() as u64,
                sqlite_sha256: Sha256::digest(&sqlite_bytes).into(),
            },
            &expected_keys,
            AgeEnvelopeOptions::TEST_FAST.limits,
        )
        .expect("manifest");
        let transport_key = artifact
            .transport_key
            .with_key(|bytes| TransportKeyMaterial::from_bytes(*bytes))
            .expect("transport key");
        let mut file = File::create(&package).expect("package file");
        encrypt_framed_payload(
            &mut file,
            passphrase,
            &manifest,
            &transport_key,
            Cursor::new(sqlite_bytes),
            &expected_keys,
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect("encrypt valid package");
        file.flush().expect("flush package");
        package
    }

    fn encrypted_package_from_bytes(
        directory: &Path,
        sqlite_bytes: &[u8],
        passphrase: &str,
        record_counts: BTreeMap<String, u64>,
    ) -> PathBuf {
        let path = directory.join(format!("malicious-{}.rpd-move", uuid::Uuid::now_v7()));
        let mut file = File::create(&path).expect("package file");
        let manifest = manifest(sqlite_bytes, record_counts);
        encrypt_framed_payload(
            &mut file,
            passphrase,
            &manifest,
            &TransportKeyMaterial::from_bytes([9; 32]),
            Cursor::new(sqlite_bytes),
            &ordered_import_tables_v1(),
            AgeEnvelopeOptions::TEST_FAST,
        )
        .expect("encrypt malicious package");
        file.flush().expect("flush package");
        path
    }

    fn encrypted_package_from_file(
        directory: &Path,
        sqlite_path: &Path,
        passphrase: &str,
        record_counts: BTreeMap<String, u64>,
    ) -> PathBuf {
        let bytes = std::fs::read(sqlite_path).expect("sqlite bytes");
        encrypted_package_from_bytes(directory, &bytes, passphrase, record_counts)
    }

    fn manifest(
        sqlite_bytes: &[u8],
        record_counts: BTreeMap<String, u64>,
    ) -> PortableMigrationManifest {
        PortableMigrationManifest {
            format: "relay-pool-portable-migration".to_string(),
            format_version: 1,
            export_id: "018f7f9a-1111-7000-8000-000000000001".to_string(),
            created_at: "2026-07-29T00:00:00Z".to_string(),
            source_app_version: "0.3.3".to_string(),
            source_platform: "windows".to_string(),
            database_generation: 2,
            database_schema_version: 10,
            portable_schema_profile: "encrypted-secrets-v1".to_string(),
            minimum_importer_version: "0.3.3".to_string(),
            transport_key_id: "transport:018f7f9a-1111-7000-8000-000000000002".to_string(),
            encryption_version: 1,
            export_policy_version: 1,
            required_features: vec![],
            extensions: serde_json::json!({}),
            included_categories: vec!["core_data".to_string()],
            excluded_categories: vec![
                "history".to_string(),
                "session_credentials".to_string(),
                "local_proxy_access_key".to_string(),
                "device_runtime_state".to_string(),
                "provider_drafts".to_string(),
            ],
            record_counts,
            sqlite_size_bytes: sqlite_bytes.len() as u64,
            sqlite_sha256: general_purpose::STANDARD.encode(Sha256::digest(sqlite_bytes)),
        }
    }

    async fn record_counts(path: &Path) -> BTreeMap<String, u64> {
        let mut connection = SqliteConnection::connect(&format!("sqlite:{}", path.display()))
            .await
            .expect("connect");
        let mut counts = BTreeMap::new();
        for table in ordered_import_tables_v1() {
            let sql = format!("SELECT COUNT(*) FROM \"{}\"", table.replace('"', "\"\""));
            let row = sqlx::query(&sql)
                .fetch_one(&mut connection)
                .await
                .expect("count");
            let count: i64 = row.get(0);
            counts.insert(table.to_string(), count as u64);
        }
        connection.close().await.expect("close");
        counts
    }

    async fn execute_sql(path: &Path, sql: &str) {
        let mut connection = SqliteConnection::connect(&format!("sqlite:{}", path.display()))
            .await
            .expect("connect");
        sqlx::query(sql)
            .execute(&mut connection)
            .await
            .expect("execute");
        connection.close().await.expect("close");
    }
}
