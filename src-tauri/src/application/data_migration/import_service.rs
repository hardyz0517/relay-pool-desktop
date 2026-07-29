use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use zeroize::Zeroizing;

use crate::{
    application::{
        data_maintenance::{DataMaintenanceActivity, DataMaintenanceCoordinator},
        data_migration::import_occupancy::ensure_restore_target_is_empty,
        data_migration::import_prepare::{
            build_target_from_inspection, validate_import_mode, PortableImportMode,
            PortableImportPrepareArtifact, PortableImportPrepareRequest,
        },
    },
    services::portable_migration::{
        age_envelope::{AgeEnvelopeError, AgeEnvelopeOptions},
        inspection_registry::{
            ImportInspectionError, ImportInspectionHandle, ImportInspectionRegistry,
            ImportInspectionSummary, ImportPreparationLease,
        },
        path_tokens::{PathTokenError, PathTokenId, PathTokenRegistry},
        schema_reader::ordered_import_tables_v1,
        staging::{stage_and_verify_import_package, PortablePackageStagingError},
        validate::PortableMigrationValidationError,
    },
    services::secrets::rekey::SecretRekeyError,
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

    pub(crate) async fn prepare_portable_import(
        &self,
        request: PortableImportPrepareRequest,
    ) -> Result<PortableImportPrepareArtifact, DataMigrationImportError> {
        validate_import_mode(request.mode, &request.confirmation_text)?;
        let _lease = self
            .maintenance
            .begin(DataMaintenanceActivity::PrepareImport)?;
        if request.mode == PortableImportMode::RestoreIntoEmpty {
            ensure_restore_target_is_empty(&request.active_database_path).await?;
        }
        let import_lease = self
            .inspections
            .consume(&request.inspected_import_id, request.now)?;
        build_target_from_inspection(&import_lease, &request).await
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
    #[error("data migration import inspection handle failed")]
    Inspection(#[from] ImportInspectionError),
    #[error("data migration package staging failed")]
    Package(#[from] PortablePackageStagingError),
    #[error("data migration package envelope failed")]
    Envelope(#[from] AgeEnvelopeError),
    #[error("data migration validation failed")]
    Validation(#[from] PortableMigrationValidationError),
    #[error("data migration secret rekey failed")]
    SecretRekey(#[from] SecretRekeyError),
    #[error("data migration import confirmation text is invalid")]
    ConfirmationTextMismatch,
    #[error("data migration restore target is not empty")]
    RestoreTargetNotEmpty,
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
        application::data_migration::import_prepare::{
            PortableImportMode, PortableImportPrepareRequest, REPLACE_CURRENT_CONFIRMATION,
        },
        models::secrets::canonical_secret_aad,
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
            secrets::{
                crypto::{decrypt_secret, encrypt_secret, EncryptedPayload},
                rekey::TransportSecretKey,
                DeviceKeyId, DeviceKeyResolver, SecretKeyMaterial,
                CURRENT_SECRET_ENCRYPTION_VERSION,
            },
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

    #[tokio::test]
    async fn portable_import_target_rebuilds_restore_into_empty_with_target_key() {
        let directory = tempfile::tempdir().expect("tempdir");
        let active = directory.path().join("active.sqlite3");
        let target = directory.path().join("target.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&active)
            .await
            .expect("active db");
        let active_before = identity_for_path(&active).expect("active before");
        let fixture = valid_package_with_station_secret(directory.path(), "move-passphrase").await;
        let service = service();
        let handle = inspect_package(&service, &fixture.package, directory.path()).await;
        let target_keys = resolver("target-device-key", [13; 32]);

        let artifact = service
            .prepare_portable_import(prepare_request(
                &handle,
                &active,
                &target,
                PortableImportMode::RestoreIntoEmpty,
                "",
                target_keys,
            ))
            .await
            .expect("prepare import");

        assert_eq!(artifact.target_database_path, target);
        assert_eq!(artifact.target_key_id, "target-device-key");
        assert_eq!(artifact.rekey_report.included_rows, 1);
        assert_eq!(artifact.row_counts.get("station_keys"), Some(&1));
        assert_eq!(
            identity_for_path(&active).expect("active after"),
            active_before,
            "prepare must not mutate the active database"
        );
        let mut connection = SqliteConnection::connect(&format!("sqlite:{}", target.display()))
            .await
            .expect("target connect");
        let local_key: String =
            sqlx::query_scalar("SELECT value FROM settings WHERE key = 'local_key'")
                .fetch_one(&mut connection)
                .await
                .expect("local key");
        let start_on_launch: String = sqlx::query_scalar(
            "SELECT value FROM settings WHERE key = 'local_proxy_start_on_launch'",
        )
        .fetch_one(&mut connection)
        .await
        .expect("start setting");
        let secret_key_id: String =
            sqlx::query_scalar("SELECT key_id FROM secrets WHERE id = 'secret-1'")
                .fetch_one(&mut connection)
                .await
                .expect("secret key id");
        connection.close().await.expect("target close");

        assert!(local_key.starts_with("sk-local-"));
        assert_eq!(start_on_launch, "false");
        assert_eq!(secret_key_id, "target-device-key");
        assert!(!artifact.target_sha256.is_empty());
    }

    #[tokio::test]
    async fn portable_import_three_keys_isolates_source_transport_and_target() {
        let directory = tempfile::tempdir().expect("tempdir");
        let active = directory.path().join("active.sqlite3");
        let target = directory.path().join("target.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&active)
            .await
            .expect("active db");
        let fixture = valid_package_with_station_secret(directory.path(), "move-passphrase").await;
        let service = service();
        let handle = inspect_package(&service, &fixture.package, directory.path()).await;
        let target_keys = resolver("target-device-key", [13; 32]);

        service
            .prepare_portable_import(prepare_request(
                &handle,
                &active,
                &target,
                PortableImportMode::RestoreIntoEmpty,
                "",
                target_keys.clone(),
            ))
            .await
            .expect("prepare import");

        let payload = read_secret_payload(&target).await;
        let aad = canonical_secret_aad(
            "station_key",
            "key-1",
            "api_key",
            CURRENT_SECRET_ENCRYPTION_VERSION,
        );
        let payload = EncryptedPayload { aad, ..payload };
        target_keys
            .with_active_key(|key| {
                assert_eq!(
                    decrypt_secret(key, &payload).expect("target decrypts"),
                    fixture.plaintext
                );
            })
            .expect("target key");
        fixture
            .source_key
            .with_active_key(|key| assert!(decrypt_secret(key, &payload).is_err()))
            .expect("source key");
        fixture
            .transport_key
            .with_key(|key| assert!(decrypt_secret(key, &payload).is_err()))
            .expect("transport key");
    }

    #[tokio::test]
    async fn migration_occupancy_rejects_non_empty_user_tables_unknown_settings_and_drafts() {
        let directory = tempfile::tempdir().expect("tempdir");
        let fixture = valid_package(directory.path(), "move-passphrase").await;

        let unknown_setting = directory.path().join("unknown-setting.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&unknown_setting)
            .await
            .expect("active db");
        execute_sql(
            &unknown_setting,
            "INSERT INTO settings (key, value, updated_at) VALUES ('future_setting', '1', '1')",
        )
        .await;
        assert_restore_occupancy_rejected(directory.path(), &fixture, &unknown_setting).await;

        let provider_draft = directory.path().join("provider-draft.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&provider_draft)
            .await
            .expect("active db");
        execute_sql(
            &provider_draft,
            r#"
            INSERT INTO provider_drafts (
                id, revision, state, payload_schema_version, payload_json,
                commit_key, created_at, updated_at, expires_at
            ) VALUES ('draft-1', 1, 'active', 1, '{}', 'commit-1', '1', '1', '2')
            "#,
        )
        .await;
        assert_restore_occupancy_rejected(directory.path(), &fixture, &provider_draft).await;

        let non_device_secret = directory.path().join("non-device-secret.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&non_device_secret)
            .await
            .expect("active db");
        execute_sql(
            &non_device_secret,
            r#"
            INSERT INTO secrets (
                id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                created_at, updated_at, key_id, encryption_version, value_hash
            ) VALUES (
                'secret-x', 'station_key', 'key-x', 'api_key', 'sk-...xxxx',
                X'01', X'02030405060708090A0B0C0D', '1', '1', 'key-x', 1, 'hash'
            )
            "#,
        )
        .await;
        assert_restore_occupancy_rejected(directory.path(), &fixture, &non_device_secret).await;

        let station_data = directory.path().join("station-data.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&station_data)
            .await
            .expect("active db");
        execute_sql(
            &station_data,
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
            ) VALUES ('station-x', 'Station X', 'openai', 'https://example.test', 'https://api.example.test', '1', '1')
            "#,
        )
        .await;
        assert_restore_occupancy_rejected(directory.path(), &fixture, &station_data).await;
    }

    #[tokio::test]
    async fn portable_import_target_replace_current_requires_exact_confirmation_text() {
        let directory = tempfile::tempdir().expect("tempdir");
        let active = directory.path().join("active.sqlite3");
        crate::persistence::migrations::initialize_v2_database(&active)
            .await
            .expect("active db");
        execute_sql(
            &active,
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
            ) VALUES ('station-x', 'Station X', 'openai', 'https://example.test', 'https://api.example.test', '1', '1')
            "#,
        )
        .await;
        let package = valid_package(directory.path(), "move-passphrase").await;
        let service = service();
        let handle = inspect_package(&service, &package, directory.path()).await;
        let target_keys = resolver("target-device-key", [13; 32]);

        let padded = service
            .prepare_portable_import(prepare_request(
                &handle,
                &active,
                &directory.path().join("padded.sqlite3"),
                PortableImportMode::ReplaceCurrent,
                " 替换当前数据 ",
                target_keys.clone(),
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            padded,
            DataMigrationImportError::ConfirmationTextMismatch
        ));

        service
            .prepare_portable_import(prepare_request(
                &handle,
                &active,
                &directory.path().join("replace.sqlite3"),
                PortableImportMode::ReplaceCurrent,
                REPLACE_CURRENT_CONFIRMATION,
                target_keys,
            ))
            .await
            .expect("exact confirmation allows replace preparation");
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

    fn prepare_request(
        handle: &ImportInspectionHandle,
        active_database_path: &Path,
        target_database_path: &Path,
        mode: PortableImportMode,
        confirmation_text: &str,
        target_keys: DeviceKeyResolver,
    ) -> PortableImportPrepareRequest {
        PortableImportPrepareRequest {
            inspected_import_id: handle.id.clone(),
            active_database_path: active_database_path.to_path_buf(),
            target_database_path: target_database_path.to_path_buf(),
            mode,
            confirmation_text: confirmation_text.to_string(),
            target_keys,
            target_updated_at: "1234567890".to_string(),
            now: Instant::now(),
        }
    }

    async fn inspect_package(
        service: &DataMigrationImportService,
        package: &Path,
        directory: &Path,
    ) -> ImportInspectionHandle {
        let token = service
            .path_tokens
            .approve_import_path(package, Instant::now())
            .expect("token");
        service
            .inspect_portable_package_with_options(
                request(
                    token.id,
                    directory.join(format!("scratch-{}", uuid::Uuid::now_v7())),
                    "move-passphrase",
                ),
                AgeEnvelopeOptions::TEST_FAST,
            )
            .await
            .expect("inspection")
    }

    async fn assert_restore_occupancy_rejected(
        directory: &Path,
        package: &Path,
        active_database_path: &Path,
    ) {
        let service = service();
        let handle = inspect_package(&service, package, directory).await;
        let error = service
            .prepare_portable_import(prepare_request(
                &handle,
                active_database_path,
                &directory.join(format!("target-{}.sqlite3", uuid::Uuid::now_v7())),
                PortableImportMode::RestoreIntoEmpty,
                "",
                resolver("target-device-key", [13; 32]),
            ))
            .await
            .unwrap_err();
        assert!(matches!(
            error,
            DataMigrationImportError::RestoreTargetNotEmpty
        ));
    }

    async fn valid_package(directory: &Path, passphrase: &str) -> PathBuf {
        valid_package_fixture(directory, passphrase, None)
            .await
            .package
    }

    async fn valid_package_with_station_secret(
        directory: &Path,
        passphrase: &str,
    ) -> PortablePackageFixture {
        valid_package_fixture(
            directory,
            passphrase,
            Some(SecretFixture {
                source_key: resolver("source-device-key", [7; 32]),
                plaintext: "sk-p8-test-secret-value-0001".to_string(),
            }),
        )
        .await
    }

    struct PortablePackageFixture {
        package: PathBuf,
        source_key: DeviceKeyResolver,
        transport_key: TransportSecretKey,
        plaintext: String,
    }

    struct SecretFixture {
        source_key: DeviceKeyResolver,
        plaintext: String,
    }

    async fn valid_package_fixture(
        directory: &Path,
        passphrase: &str,
        secret_fixture: Option<SecretFixture>,
    ) -> PortablePackageFixture {
        let source = directory.join(format!("source-{}.sqlite3", uuid::Uuid::now_v7()));
        let package = directory.join(format!("portable-{}.rpd-move", uuid::Uuid::now_v7()));
        let portable = directory.join(format!("portable-{}.sqlite3", uuid::Uuid::now_v7()));
        let working = directory.join(format!("export-{}", uuid::Uuid::now_v7()));
        crate::persistence::migrations::initialize_v2_database(&source)
            .await
            .expect("source db");
        let (source_key, plaintext) = match secret_fixture {
            Some(secret_fixture) => {
                insert_station_secret(
                    &source,
                    &secret_fixture.source_key,
                    &secret_fixture.plaintext,
                )
                .await;
                (secret_fixture.source_key, secret_fixture.plaintext)
            }
            None => (DeviceKeyResolver::for_test([7_u8; 32]), String::new()),
        };
        let service =
            DataMigrationExportService::new(DataMaintenanceCoordinator::new(), source_key.clone());
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
        PortablePackageFixture {
            package,
            source_key,
            transport_key: artifact.transport_key,
            plaintext,
        }
    }

    async fn insert_station_secret(source: &Path, source_key: &DeviceKeyResolver, plaintext: &str) {
        let encrypted = source_key
            .with_active_key(|key| {
                encrypt_secret(
                    key,
                    plaintext,
                    &canonical_secret_aad(
                        "station_key",
                        "key-1",
                        "api_key",
                        CURRENT_SECRET_ENCRYPTION_VERSION,
                    ),
                )
                .expect("encrypt fixture secret")
            })
            .expect("source key");
        let ciphertext = general_purpose::STANDARD
            .decode(encrypted.ciphertext)
            .expect("ciphertext");
        let nonce = general_purpose::STANDARD
            .decode(encrypted.nonce)
            .expect("nonce");
        let mut connection = SqliteConnection::connect(&format!("sqlite:{}", source.display()))
            .await
            .expect("connect source");
        sqlx::query(
            r#"
            INSERT INTO stations (
                id, name, station_type, website_url, api_base_url, created_at, updated_at
            ) VALUES ('station-1', 'Station 1', 'openai', 'https://example.test', 'https://api.example.test', '1', '1')
            "#,
        )
        .execute(&mut connection)
        .await
        .expect("station");
        sqlx::query(
            r#"
            INSERT INTO secrets (
                id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                created_at, updated_at, key_id, encryption_version, value_hash
            ) VALUES (
                'secret-1', 'station_key', 'key-1', 'api_key', 'sk-...0001',
                ?1, ?2, '1', '1', ?3, ?4, ?5
            )
            "#,
        )
        .bind(ciphertext)
        .bind(nonce)
        .bind(source_key.active_key_id().as_str())
        .bind(i64::from(CURRENT_SECRET_ENCRYPTION_VERSION))
        .bind(encrypted.value_hash)
        .execute(&mut connection)
        .await
        .expect("secret");
        sqlx::query(
            r#"
            INSERT INTO station_keys (
                id, station_id, name, api_key_secret_id, created_at, updated_at
            ) VALUES ('key-1', 'station-1', 'Default Key', 'secret-1', '1', '1')
            "#,
        )
        .execute(&mut connection)
        .await
        .expect("station key");
        connection.close().await.expect("close source");
    }

    async fn read_secret_payload(target: &Path) -> EncryptedPayload {
        let mut connection = SqliteConnection::connect(&format!("sqlite:{}", target.display()))
            .await
            .expect("target connect");
        let row =
            sqlx::query("SELECT ciphertext, nonce, value_hash FROM secrets WHERE id = 'secret-1'")
                .fetch_one(&mut connection)
                .await
                .expect("secret");
        connection.close().await.expect("target close");
        let ciphertext: Vec<u8> = row.get("ciphertext");
        let nonce: Vec<u8> = row.get("nonce");
        EncryptedPayload {
            ciphertext: general_purpose::STANDARD.encode(ciphertext),
            nonce: general_purpose::STANDARD.encode(nonce),
            aad: String::new(),
            value_hash: row.get("value_hash"),
        }
    }

    fn resolver(key_id: &str, material: [u8; 32]) -> DeviceKeyResolver {
        DeviceKeyResolver::active(
            DeviceKeyId::new(key_id),
            SecretKeyMaterial::from_bytes(material),
            CURRENT_SECRET_ENCRYPTION_VERSION,
        )
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
