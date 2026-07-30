pub(crate) mod errors;
pub(crate) mod export_service;
pub(crate) mod import_prepare;
pub(crate) mod import_service;
pub(crate) mod registry;

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};

use crate::{
    application::data_maintenance::DataMaintenanceCoordinator,
    background_tasks::{OperationFailureCode, OperationRegistry, OperationTerminal},
    ipc::dto::data_migration::{
        InspectPortableImportInputDto, PortableExportResultDto, PortableImportInspectionDto,
        PortableImportModeDto, PortableImportPrepareResultDto, PortableImportRecoveryReasonCodeDto,
        PortableImportRecoveryStateDto, PortableMigrationBlockedReasonDto,
        PortableMigrationCapabilityDto, PortableMigrationLimitsDto, PortableMigrationOperationDto,
        PortableMigrationOperationStartedDto, PortableMigrationResourceKindDto,
        PortablePathTokenDto, PreparePortableImportInputDto, StartPortableExportInputDto,
    },
    persistence::runtime::PersistenceRuntime,
    services::portable_migration::{
        activation_journal::{read_journal, PortableActivationPhase},
        limits::PortableMigrationLimitsV1,
        path_tokens::{PathTokenError, PathTokenRegistry},
    },
    services::{
        data_store::atomic_file::PublishMode,
        portable_migration::inspection_registry::ImportInspectionRegistry,
        proxy::runtime::ProxyRuntimeState, secrets::DeviceKeyResolver,
    },
};
use zeroize::Zeroizing;

use self::{
    errors::{DataMigrationError, DataMigrationImportError},
    export_service::{DataMigrationExportService, PortablePackageExportRequest},
    import_prepare::{PortableImportMode, PortableImportPrepareRequest},
    import_service::{
        DataMigrationImportService, PortableImportActivationPrepareRequest,
        PortableImportInspectionRequest,
    },
    registry::{
        PortableMigrationOperationRegistry, PortableMigrationProgress,
        PortableMigrationRegistryError, PortableMigrationTerminalResult, PortableOperationKind,
    },
};

const SECURITY_POLICY_APPROVED: bool = true;
const PATH_TOKEN_TTL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone)]
pub(crate) struct PortableMigrationCommandFacade {
    config_dir: PathBuf,
    default_data_dir: PathBuf,
    path_tokens: PathTokenRegistry,
    inspections: ImportInspectionRegistry,
    operations: PortableMigrationOperationRegistry,
    raw_operations: OperationRegistry,
    limits: PortableMigrationLimitsV1,
    ready: Arc<Mutex<Option<PortableMigrationReadyServices>>>,
    results: Arc<Mutex<PortableMigrationCommandResults>>,
    idempotency_key: [u8; 32],
}

impl PortableMigrationCommandFacade {
    pub(crate) fn new(
        config_dir: PathBuf,
        default_data_dir: PathBuf,
        operation_registry: OperationRegistry,
    ) -> Self {
        let raw_operations = operation_registry.clone();
        let mut idempotency_key = [0_u8; 32];
        OsRng.fill_bytes(&mut idempotency_key);
        Self {
            config_dir,
            default_data_dir,
            path_tokens: PathTokenRegistry::new(),
            inspections: ImportInspectionRegistry::new(),
            operations: PortableMigrationOperationRegistry::new(operation_registry),
            raw_operations,
            limits: PortableMigrationLimitsV1::CURRENT,
            ready: Arc::new(Mutex::new(None)),
            results: Arc::new(Mutex::new(PortableMigrationCommandResults::default())),
            idempotency_key,
        }
    }

    pub(crate) fn configure_ready_services(
        &self,
        maintenance: DataMaintenanceCoordinator,
        source_database_path: PathBuf,
        device_keys: DeviceKeyResolver,
        runtime: Arc<PersistenceRuntime>,
        proxy: Option<Arc<ProxyRuntimeState>>,
    ) {
        let export = DataMigrationExportService::new(maintenance.clone(), device_keys.clone());
        let import = DataMigrationImportService::new(
            maintenance,
            self.path_tokens.clone(),
            self.inspections.clone(),
        );
        *self.ready.lock().expect("portable migration ready mutex") =
            Some(PortableMigrationReadyServices {
                export,
                import,
                source_database_path,
                device_keys,
                runtime,
                operations: self.raw_operations.clone(),
                proxy,
            });
    }

    pub(crate) fn capability(&self) -> PortableMigrationCapabilityDto {
        let mut blocked_reasons = Vec::new();
        if !SECURITY_POLICY_APPROVED {
            blocked_reasons.push(PortableMigrationBlockedReasonDto::SecurityPolicyNotApproved);
        }
        if !cfg!(target_os = "windows") {
            blocked_reasons.push(PortableMigrationBlockedReasonDto::UnsupportedPlatform);
        }
        if !self.default_data_dir.exists() {
            blocked_reasons.push(PortableMigrationBlockedReasonDto::DataStoreNotWritable);
        }
        blocked_reasons.dedup();
        PortableMigrationCapabilityDto {
            enabled: blocked_reasons.is_empty(),
            blocked_reasons,
            supported_format: "relay-pool-portable-migration".to_string(),
            supported_profile: "portable-migration-v1".to_string(),
            current_schema_profile: "relay-pool-desktop-v10".to_string(),
            history_supported: true,
            limits: PortableMigrationLimitsDto::from(self.limits),
        }
    }

    pub(crate) fn choose_export_path(
        &self,
    ) -> Result<Option<PortablePathTokenDto>, PortableMigrationCommandError> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Relay Pool migration", &["rpd-move"])
            .set_file_name("relay-pool-data.rpd-move")
            .save_file()
        else {
            return Ok(None);
        };
        let parent = path
            .parent()
            .ok_or(PortableMigrationCommandError::PathRejected)?;
        let leaf = path
            .file_name()
            .ok_or(PortableMigrationCommandError::PathRejected)?
            .to_os_string();
        let token = self
            .path_tokens
            .approve_export_path(parent, leaf, true, Instant::now())?;
        Ok(Some(PortablePathTokenDto {
            path_token: token.id.as_str().to_string(),
            expires_in_ms: PATH_TOKEN_TTL.as_millis().try_into().unwrap_or(u64::MAX),
        }))
    }

    pub(crate) fn choose_import_file(
        &self,
    ) -> Result<Option<PortablePathTokenDto>, PortableMigrationCommandError> {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Relay Pool migration", &["rpd-move"])
            .pick_file()
        else {
            return Ok(None);
        };
        let token = self.path_tokens.approve_import_path(path, Instant::now())?;
        Ok(Some(PortablePathTokenDto {
            path_token: token.id.as_str().to_string(),
            expires_in_ms: PATH_TOKEN_TTL.as_millis().try_into().unwrap_or(u64::MAX),
        }))
    }

    pub(crate) fn start_portable_export(
        &self,
        input: StartPortableExportInputDto,
    ) -> Result<PortableMigrationOperationStartedDto, PortableMigrationCommandError> {
        self.ensure_available()?;
        let digest = self.export_idempotency_digest(&input);
        if let Some(started) = self.idempotency_result(&input.idempotency_key, digest)? {
            return Ok(started);
        }
        let ready = self.ready_services()?;
        let now = Instant::now();
        let lease = self
            .path_tokens
            .consume_export_value(&input.output_path_token, now)?;
        let export_id = uuid::Uuid::now_v7().to_string();
        let package_path = lease.approved_leaf().path();
        let overwrite_existing = lease.mode == PublishMode::ReplaceExisting;
        let request = PortablePackageExportRequest {
            source_database_path: ready.source_database_path.clone(),
            package_path,
            working_directory: self
                .default_data_dir
                .join("portable-migration")
                .join("export-working"),
            include_history: input.options.include_history,
            overwrite_existing,
            passphrase: Zeroizing::new(input.passphrase),
            passphrase_confirmation: Zeroizing::new(input.passphrase_confirmation),
        };
        let service = ready.export.clone();
        let operations = self.operations.clone();
        let results = Arc::clone(&self.results);
        let resource_id = export_id.clone();
        let operation_resource_id = resource_id.clone();
        let operation_id = self.operations.start_portable_operation(
            PortableOperationKind::ExportPackage,
            Some("portable-migration:export".to_string()),
            move |context| {
                Box::pin(async move {
                    let _ = operations.emit_progress_at(
                        context.id,
                        PortableMigrationProgress::Queued,
                        Instant::now(),
                    );
                    let result = service
                        .export_portable_package_with_export_id(
                            request,
                            export_id.clone(),
                            Some(&context.cancellation_token),
                        )
                        .await;
                    match result {
                        Ok(artifact) => {
                            let terminal = PortableMigrationTerminalResult::ExportedPackage {
                                export_id: artifact.export_id.clone(),
                                package_size_bytes: artifact.package_size_bytes,
                            };
                            let _ = operations.record_terminal_result_at(
                                context.id,
                                terminal,
                                Instant::now(),
                            );
                            results
                                .lock()
                                .expect("portable migration result mutex")
                                .exports
                                .insert(
                                    operation_resource_id.clone(),
                                    PortableExportResultDto {
                                        export_id: artifact.export_id,
                                        package_size_bytes: artifact.package_size_bytes,
                                    },
                                );
                            OperationTerminal::Completed
                        }
                        Err(error) if context.cancellation_token.is_cancelled() => {
                            drop(error);
                            OperationTerminal::Cancelled
                        }
                        Err(_) => OperationTerminal::Failed {
                            code: OperationFailureCode::new("portable_export_failed"),
                        },
                    }
                })
            },
        )?;
        let started = PortableMigrationOperationStartedDto {
            operation_id: operation_id.as_u64().to_string(),
            resource_id,
            resource_kind: PortableMigrationResourceKindDto::Export,
        };
        self.remember_idempotency(input.idempotency_key, digest, started.clone())?;
        Ok(started)
    }

    pub(crate) fn get_portable_export_result(
        &self,
        resource_id: String,
    ) -> Result<PortableExportResultDto, PortableMigrationCommandError> {
        self.results
            .lock()
            .expect("portable migration result mutex")
            .exports
            .get(&resource_id)
            .cloned()
            .ok_or(PortableMigrationCommandError::ResultUnknown)
    }

    pub(crate) fn start_portable_import_inspection(
        &self,
        input: InspectPortableImportInputDto,
    ) -> Result<PortableMigrationOperationStartedDto, PortableMigrationCommandError> {
        self.ensure_available()?;
        let digest = self.inspect_idempotency_digest(&input);
        if let Some(started) = self.idempotency_result(&input.idempotency_key, digest)? {
            return Ok(started);
        }
        let ready = self.ready_services()?;
        let now = Instant::now();
        let import_token = self
            .path_tokens
            .import_token_by_value(&input.input_path_token, now)?;
        let inspection_id = uuid::Uuid::now_v7().to_string();
        let request = PortableImportInspectionRequest {
            import_token,
            scratch_directory: self
                .default_data_dir
                .join("portable-migration")
                .join(format!("import-inspection-{inspection_id}")),
            passphrase: Zeroizing::new(input.passphrase),
            rate_limit_key: input.input_path_token,
            now,
        };
        let service = ready.import.clone();
        let operations = self.operations.clone();
        let results = Arc::clone(&self.results);
        let resource_id = inspection_id.clone();
        let operation_resource_id = resource_id.clone();
        let operation_id = self.operations.start_portable_operation(
            PortableOperationKind::InspectPackage,
            Some("portable-migration:inspect".to_string()),
            move |context| {
                Box::pin(async move {
                    let _ = operations.emit_progress_at(
                        context.id,
                        PortableMigrationProgress::Queued,
                        Instant::now(),
                    );
                    let result = service
                        .inspect_portable_package_with_inspection_id(request, inspection_id.clone())
                        .await;
                    match result {
                        Ok(handle) => {
                            let terminal = PortableMigrationTerminalResult::InspectedPackage {
                                export_id: handle.summary.export_id.clone(),
                                source_platform: handle.summary.source_platform.clone(),
                                included_categories: handle.summary.included_categories.clone(),
                                sqlite_size_bytes: handle.summary.sqlite_size_bytes,
                            };
                            let _ = operations.record_terminal_result_at(
                                context.id,
                                terminal,
                                Instant::now(),
                            );
                            results
                                .lock()
                                .expect("portable migration result mutex")
                                .inspections
                                .insert(
                                    operation_resource_id.clone(),
                                    PortableImportInspectionDto {
                                        inspection_id: handle.id.as_str().to_string(),
                                        export_id: handle.summary.export_id,
                                        source_platform: handle.summary.source_platform,
                                        included_categories: handle.summary.included_categories,
                                        include_history: handle.summary.include_history,
                                        sqlite_size_bytes: handle.summary.sqlite_size_bytes,
                                    },
                                );
                            OperationTerminal::Completed
                        }
                        Err(error) if context.cancellation_token.is_cancelled() => {
                            drop(error);
                            OperationTerminal::Cancelled
                        }
                        Err(_) => OperationTerminal::Failed {
                            code: OperationFailureCode::new("portable_import_inspection_failed"),
                        },
                    }
                })
            },
        )?;
        let started = PortableMigrationOperationStartedDto {
            operation_id: operation_id.as_u64().to_string(),
            resource_id,
            resource_kind: PortableMigrationResourceKindDto::Inspection,
        };
        self.remember_idempotency(input.idempotency_key, digest, started.clone())?;
        Ok(started)
    }

    pub(crate) fn get_portable_import_inspection(
        &self,
        resource_id: String,
    ) -> Result<PortableImportInspectionDto, PortableMigrationCommandError> {
        self.results
            .lock()
            .expect("portable migration result mutex")
            .inspections
            .get(&resource_id)
            .cloned()
            .ok_or(PortableMigrationCommandError::ResultUnknown)
    }

    pub(crate) fn start_portable_import_prepare(
        &self,
        input: PreparePortableImportInputDto,
    ) -> Result<PortableMigrationOperationStartedDto, PortableMigrationCommandError> {
        self.ensure_available()?;
        let digest = self.prepare_idempotency_digest(&input);
        if let Some(started) = self.idempotency_result(&input.idempotency_key, digest)? {
            return Ok(started);
        }
        let ready = self.ready_services()?;
        let now = Instant::now();
        let inspected_import_id = self
            .inspections
            .id_for_value(&input.inspected_import_id, now)
            .map_err(DataMigrationImportError::from)?;
        let inspected_summary = self
            .inspections
            .summary(&inspected_import_id, now)
            .map_err(DataMigrationImportError::from)?;
        let import_id = uuid::Uuid::now_v7().to_string();
        let target_database_path = self
            .default_data_dir
            .join(format!("portable-import-target-{import_id}.sqlite3"));
        let request = PortableImportActivationPrepareRequest {
            import: PortableImportPrepareRequest {
                inspected_import_id,
                active_database_path: ready.source_database_path.clone(),
                target_database_path,
                mode: import_mode_from_dto(input.mode),
                confirmation_text: input.confirmation_text,
                target_keys: ready.device_keys.clone(),
                target_updated_at: chrono::Utc::now().timestamp_millis().to_string(),
                now,
            },
            app_config_dir: self.config_dir.clone(),
            default_app_data_dir: self.default_data_dir.clone(),
            freeze_deadline: self.limits.prepare_deadline(),
        };
        let service = ready.import.clone();
        let runtime = Arc::clone(&ready.runtime);
        let raw_operations = ready.operations.clone();
        let proxy = ready.proxy.clone();
        let operations = self.operations.clone();
        let results = Arc::clone(&self.results);
        let resource_id = import_id.clone();
        let operation_resource_id = resource_id.clone();
        let source_export_id = inspected_summary.export_id;
        let operation_id = self.operations.start_portable_operation(
            PortableOperationKind::PrepareImport,
            Some("portable-migration:prepare".to_string()),
            move |context| {
                Box::pin(async move {
                    context.enter_commit_barrier();
                    let _ = operations.emit_progress_at(
                        context.id,
                        PortableMigrationProgress::Queued,
                        Instant::now(),
                    );
                    let result = service
                        .prepare_portable_import_for_activation_with_import_id(
                            request,
                            import_id.clone(),
                            Some(context.id),
                            &runtime,
                            &raw_operations,
                            None,
                            proxy.as_deref(),
                        )
                        .await;
                    match result {
                        Ok(artifact) => {
                            let target_rows = artifact
                                .artifact
                                .row_counts
                                .values()
                                .try_fold(0_u64, |total, count| total.checked_add(*count as u64))
                                .unwrap_or(u64::MAX);
                            let terminal = PortableMigrationTerminalResult::PreparedImport {
                                export_id: source_export_id,
                                target_rows,
                            };
                            let _ = operations.record_terminal_result_at(
                                context.id,
                                terminal,
                                Instant::now(),
                            );
                            results
                                .lock()
                                .expect("portable migration result mutex")
                                .prepares
                                .insert(
                                    operation_resource_id.clone(),
                                    PortableImportPrepareResultDto {
                                        import_id: operation_resource_id.clone(),
                                        restart_required: artifact.restart_required,
                                    },
                                );
                            OperationTerminal::Completed
                        }
                        Err(error) if context.cancellation_token.is_cancelled() => {
                            drop(error);
                            OperationTerminal::Cancelled
                        }
                        Err(_) => OperationTerminal::Failed {
                            code: OperationFailureCode::new("portable_import_prepare_failed"),
                        },
                    }
                })
            },
        )?;
        let started = PortableMigrationOperationStartedDto {
            operation_id: operation_id.as_u64().to_string(),
            resource_id,
            resource_kind: PortableMigrationResourceKindDto::Import,
        };
        self.remember_idempotency(input.idempotency_key, digest, started.clone())?;
        Ok(started)
    }

    pub(crate) fn get_portable_import_prepare_result(
        &self,
        resource_id: String,
    ) -> Result<PortableImportPrepareResultDto, PortableMigrationCommandError> {
        self.results
            .lock()
            .expect("portable migration result mutex")
            .prepares
            .get(&resource_id)
            .cloned()
            .ok_or(PortableMigrationCommandError::ResultUnknown)
    }

    pub(crate) fn operation(
        &self,
        operation_id: crate::background_tasks::OperationId,
    ) -> Result<PortableMigrationOperationDto, PortableMigrationCommandError> {
        self.operations
            .get_portable_migration_operation(operation_id, Instant::now())
            .map(PortableMigrationOperationDto::from)
            .map_err(PortableMigrationCommandError::Registry)
    }

    pub(crate) fn recovery_state(&self) -> PortableImportRecoveryStateDto {
        match read_journal(&self.config_dir) {
            Ok(Some(journal)) => {
                let import_id = journal.payload.operation_id;
                match journal.payload.phase {
                    PortableActivationPhase::Prepared
                    | PortableActivationPhase::ActivationStarted
                    | PortableActivationPhase::ReplacementCommitted
                    | PortableActivationPhase::ActivatedValidated => {
                        PortableImportRecoveryStateDto::ActivationPending { import_id }
                    }
                    PortableActivationPhase::RolledBack => {
                        PortableImportRecoveryStateDto::RolledBack {
                            import_id,
                            reason_code:
                                PortableImportRecoveryReasonCodeDto::ActivationValidationFailed,
                        }
                    }
                    PortableActivationPhase::ManualRecoveryRequired => {
                        PortableImportRecoveryStateDto::ManualRecoveryRequired {
                            import_id: Some(import_id),
                            reason_code:
                                PortableImportRecoveryReasonCodeDto::ArtifactIdentityMismatch,
                        }
                    }
                    PortableActivationPhase::RollbackStarted => {
                        PortableImportRecoveryStateDto::ManualRecoveryRequired {
                            import_id: Some(import_id),
                            reason_code:
                                PortableImportRecoveryReasonCodeDto::RollbackValidationFailed,
                        }
                    }
                    PortableActivationPhase::Completed => {
                        PortableImportRecoveryStateDto::Activated { import_id }
                    }
                }
            }
            Ok(None) => PortableImportRecoveryStateDto::None,
            Err(_) => PortableImportRecoveryStateDto::ManualRecoveryRequired {
                import_id: None,
                reason_code: PortableImportRecoveryReasonCodeDto::JournalInvalid,
            },
        }
    }

    fn ensure_available(&self) -> Result<(), PortableMigrationCommandError> {
        if self.capability().enabled {
            Ok(())
        } else {
            Err(PortableMigrationCommandError::FeatureUnavailable)
        }
    }

    fn ready_services(
        &self,
    ) -> Result<PortableMigrationReadyServices, PortableMigrationCommandError> {
        self.ready
            .lock()
            .expect("portable migration ready mutex")
            .clone()
            .ok_or(PortableMigrationCommandError::ReadyServicesUnavailable)
    }

    fn idempotency_result(
        &self,
        key: &str,
        digest: [u8; 32],
    ) -> Result<Option<PortableMigrationOperationStartedDto>, PortableMigrationCommandError> {
        let results = self
            .results
            .lock()
            .expect("portable migration result mutex");
        match results.idempotency.get(key) {
            Some(binding) if binding.digest == digest => Ok(Some(binding.started.clone())),
            Some(_) => Err(PortableMigrationCommandError::IdempotencyConflict),
            None => Ok(None),
        }
    }

    fn remember_idempotency(
        &self,
        key: String,
        digest: [u8; 32],
        started: PortableMigrationOperationStartedDto,
    ) -> Result<(), PortableMigrationCommandError> {
        let mut results = self
            .results
            .lock()
            .expect("portable migration result mutex");
        match results.idempotency.get(&key) {
            Some(binding) if binding.digest == digest => Ok(()),
            Some(_) => Err(PortableMigrationCommandError::IdempotencyConflict),
            None => {
                results
                    .idempotency
                    .insert(key, PortableMigrationIdempotencyBinding { digest, started });
                Ok(())
            }
        }
    }

    fn export_idempotency_digest(&self, input: &StartPortableExportInputDto) -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(b"portable-export\0");
        update_digest_field(&mut data, input.output_path_token.as_bytes());
        update_digest_bool(&mut data, input.options.include_history);
        update_digest_secret(&mut data, input.passphrase.as_bytes());
        update_digest_secret(&mut data, input.passphrase_confirmation.as_bytes());
        keyed_hmac(&self.idempotency_key, &data)
    }

    fn inspect_idempotency_digest(&self, input: &InspectPortableImportInputDto) -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(b"portable-inspect\0");
        update_digest_field(&mut data, input.input_path_token.as_bytes());
        update_digest_secret(&mut data, input.passphrase.as_bytes());
        keyed_hmac(&self.idempotency_key, &data)
    }

    fn prepare_idempotency_digest(&self, input: &PreparePortableImportInputDto) -> [u8; 32] {
        let mut data = Vec::new();
        data.extend_from_slice(b"portable-prepare\0");
        update_digest_field(&mut data, input.inspected_import_id.as_bytes());
        update_digest_field(
            &mut data,
            match input.mode {
                PortableImportModeDto::RestoreIntoEmpty => b"restoreIntoEmpty",
                PortableImportModeDto::ReplaceCurrent => b"replaceCurrent",
            },
        );
        update_digest_field(&mut data, input.confirmation_text.as_bytes());
        keyed_hmac(&self.idempotency_key, &data)
    }
}

#[derive(Clone)]
struct PortableMigrationReadyServices {
    export: DataMigrationExportService,
    import: DataMigrationImportService,
    source_database_path: PathBuf,
    device_keys: DeviceKeyResolver,
    runtime: Arc<PersistenceRuntime>,
    operations: OperationRegistry,
    proxy: Option<Arc<ProxyRuntimeState>>,
}

#[derive(Default)]
struct PortableMigrationCommandResults {
    exports: HashMap<String, PortableExportResultDto>,
    inspections: HashMap<String, PortableImportInspectionDto>,
    prepares: HashMap<String, PortableImportPrepareResultDto>,
    idempotency: HashMap<String, PortableMigrationIdempotencyBinding>,
}

struct PortableMigrationIdempotencyBinding {
    digest: [u8; 32],
    started: PortableMigrationOperationStartedDto,
}

fn update_digest_bool(data: &mut Vec<u8>, value: bool) {
    data.extend_from_slice(&[u8::from(value)]);
}

fn update_digest_field(data: &mut Vec<u8>, value: &[u8]) {
    data.extend_from_slice(&(value.len() as u64).to_be_bytes());
    data.extend_from_slice(value);
    data.extend_from_slice(b"\0");
}

fn update_digest_secret(data: &mut Vec<u8>, value: &[u8]) {
    update_digest_field(data, value);
}

fn keyed_hmac(key: &[u8; 32], value: &[u8]) -> [u8; 32] {
    let mut ipad = [0x36_u8; 64];
    let mut opad = [0x5c_u8; 64];
    for (index, byte) in key.iter().enumerate() {
        ipad[index] ^= byte;
        opad[index] ^= byte;
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(value);
    let inner_digest = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_digest);
    outer.finalize().into()
}

fn import_mode_from_dto(mode: PortableImportModeDto) -> PortableImportMode {
    match mode {
        PortableImportModeDto::RestoreIntoEmpty => PortableImportMode::RestoreIntoEmpty,
        PortableImportModeDto::ReplaceCurrent => PortableImportMode::ReplaceCurrent,
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortableMigrationCommandError {
    #[error("portable migration is disabled by policy")]
    FeatureUnavailable,
    #[error("portable migration result is unavailable")]
    ResultUnknown,
    #[error("portable migration path was rejected")]
    PathRejected,
    #[error("portable migration path token failed")]
    PathToken(#[from] PathTokenError),
    #[error("portable migration operation registry failed")]
    Registry(#[from] PortableMigrationRegistryError),
    #[error("portable migration data export failed")]
    Export(#[from] DataMigrationError),
    #[error("portable migration data import failed")]
    Import(#[from] DataMigrationImportError),
    #[error("portable migration ready services are unavailable")]
    ReadyServicesUnavailable,
    #[error("portable migration idempotency key is already bound to different input")]
    IdempotencyConflict,
}
