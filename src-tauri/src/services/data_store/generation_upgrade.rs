use std::{
    cell::RefCell,
    fs,
    path::{Path, PathBuf},
};

use base64::{engine::general_purpose, Engine as _};
use chrono::{SecondsFormat, Utc};

use crate::{
    persistence::{
        self,
        legacy_import::{
            self, ImportPhase as LegacyImportPhase, LegacySecretTransformer, UpgradeError,
        },
        runtime::PersistenceRuntime,
        upgrade_fault::{
            BackupStep, ImportPhase, NoUpgradeFaults, UpgradeFailpoint, UpgradeFaultInjector,
        },
        upgrade_journal::{
            ReleasedSchemaProfile, Sha256Digest, UpgradeAttemptId, UpgradeJournal, UpgradePhase,
            UtcTimestamp,
        },
        upgrade_recovery_executor::{
            journal_artifact_paths_are_safe, observe_backup, observe_journal,
            observe_legacy_source, publish_v2_candidate_with_faults, read_legacy_tombstone,
            remove_file_and_sync_parent_with_faults, replace_legacy_with_tombstone_with_faults,
            resolve_allowlisted_artifact, sha256_file, write_journal_atomically_with_faults,
            RecoveryExecution, RecoveryExecutor, UpgradeExecutionError, UpgradeIoOperation,
            UpgradeRecoveryIo, UPGRADE_JOURNAL_FILE,
        },
        upgrade_recovery_plan::{
            BackupState, CompatibilityState, ConfigGeneration, JournalState, LegacySidecarState,
            LegacySourceState, ObservedUpgradeState, RecoveryHaltReason, RecoveryPlan,
            V2CandidateState,
        },
    },
    services::{
        data_store::config::{
            create_installation_marker, installation_marker_exists, read_config_v3,
            write_config_v3_with_faults, DataDirConfigV3, DatabaseGeneration,
        },
        secrets::validation::validate_database_secrets,
    },
};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";

pub(crate) async fn initialize_empty_generation_two(path: &Path) -> Result<(), String> {
    let runtime = PersistenceRuntime::initialize_new(path)
        .await
        .map_err(|error| format!("failed to initialize generation 2 data store: {error}"))?;
    runtime
        .close()
        .await
        .map_err(|error| format!("failed to close generation 2 data store: {error}"))
}

pub(crate) fn current_schema_version() -> i64 {
    persistence::current_schema_version()
}

pub(crate) fn prepare_generation_two(
    default_data_dir: &Path,
    active_data_dir: &Path,
    selected_database_path: Option<&Path>,
    data_key: [u8; 32],
) -> Result<(PersistenceRuntime, PathBuf), String> {
    let transformer = DataKeyLegacyTransformer { data_key };
    prepare_generation_two_with_faults(
        default_data_dir,
        active_data_dir,
        selected_database_path,
        data_key,
        &transformer,
        &NoUpgradeFaults,
    )
}

pub(crate) fn commit_explicit_generation_two_recovery(
    default_data_dir: &Path,
    candidate_path: &Path,
    data_key: [u8; 32],
) -> Result<bool, String> {
    let journal_path = default_data_dir.join(UPGRADE_JOURNAL_FILE);
    let observed = observe_journal(&journal_path);
    let Some(journal) = observed.journal else {
        return match observed.state {
            JournalState::Missing => Ok(false),
            JournalState::Invalid | JournalState::Valid(_) => {
                Err("upgrade journal is invalid or unreadable".to_string())
            }
        };
    };
    if journal.payload().phase != UpgradePhase::LegacyDeactivated {
        return Err(
            "explicit generation 2 recovery is not available in this upgrade phase".to_string(),
        );
    }
    let active_data_dir = candidate_path
        .parent()
        .ok_or_else(|| "generation 2 candidate has no parent directory".to_string())?;
    if !journal_artifact_paths_are_safe(active_data_dir, &journal) {
        return Err("upgrade journal contains an unsafe artifact path".to_string());
    }
    let expected_final =
        resolve_allowlisted_artifact(active_data_dir, journal.payload().paths.v2_final())
            .map_err(redacted_execution_error)?;
    let expected_final = expected_final
        .canonicalize()
        .map_err(|_| "generation 2 candidate is unavailable".to_string())?;
    let candidate = candidate_path
        .canonicalize()
        .map_err(|_| "generation 2 candidate is unavailable".to_string())?;
    if candidate != expected_final {
        return Err("selected candidate does not belong to the active upgrade attempt".to_string());
    }
    let source_path = active_data_dir.join(DatabaseGeneration::One.database_file());
    let attempt = read_legacy_tombstone(&source_path)
        .map_err(redacted_execution_error)?
        .ok_or_else(|| "generation 1 deactivation tombstone is unavailable".to_string())?;
    if attempt != journal.payload().attempt_id {
        return Err("generation 1 tombstone belongs to another upgrade attempt".to_string());
    }
    assert_backup_identity(&backup_path(active_data_dir, &journal)?, &journal)?;
    validate_v2_artifact(&candidate, data_key, &NoUpgradeFaults)?;
    commit_generation_two_config(default_data_dir, active_data_dir, &NoUpgradeFaults)?;
    Ok(true)
}

struct DataKeyLegacyTransformer {
    data_key: [u8; 32],
}

impl LegacySecretTransformer for DataKeyLegacyTransformer {
    fn transform(
        &self,
        _profile_id: &str,
        material: legacy_import::LegacySecretMaterial,
    ) -> Result<legacy_import::ImportedEncryptedSecret, UpgradeError> {
        use legacy_import::LegacySecretMaterial;
        let (scope, owner_id, kind, ciphertext, nonce, masked_value) = match material {
            LegacySecretMaterial::Plaintext {
                scope,
                owner_id,
                kind,
                value,
            } => {
                let plaintext = String::from_utf8(value.as_bytes().to_vec())
                    .map_err(|_| UpgradeError::SecretTransformationFailed)?;
                let payload = crate::services::secrets::crypto::encrypt_secret(
                    &self.data_key,
                    &plaintext,
                    &format!("{scope}:{owner_id}:{kind}"),
                )
                .map_err(|_| UpgradeError::SecretTransformationFailed)?;
                let ciphertext = general_purpose::STANDARD
                    .decode(payload.ciphertext)
                    .map_err(|_| UpgradeError::SecretTransformationFailed)?;
                let nonce = general_purpose::STANDARD
                    .decode(payload.nonce)
                    .map_err(|_| UpgradeError::SecretTransformationFailed)?;
                (
                    scope,
                    owner_id,
                    kind,
                    ciphertext,
                    nonce,
                    crate::services::secrets::mask::mask_secret(&plaintext),
                )
            }
            LegacySecretMaterial::EncryptedV1 {
                scope,
                owner_id,
                kind,
                ciphertext,
                nonce,
                aad,
            } => {
                if aad != format!("{scope}:{owner_id}:{kind}") {
                    return Err(UpgradeError::SecretTransformationFailed);
                }
                (
                    scope,
                    owner_id,
                    kind,
                    general_purpose::STANDARD
                        .decode(ciphertext.as_bytes())
                        .map_err(|_| UpgradeError::SecretTransformationFailed)?,
                    general_purpose::STANDARD
                        .decode(nonce.as_bytes())
                        .map_err(|_| UpgradeError::SecretTransformationFailed)?,
                    "****".to_string(),
                )
            }
        };
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        Ok(legacy_import::ImportedEncryptedSecret {
            id: uuid::Uuid::now_v7().to_string(),
            scope,
            owner_id,
            kind,
            masked_value,
            ciphertext,
            nonce,
            created_at: now.clone(),
            updated_at: now,
        })
    }
}

pub(crate) fn prepare_generation_two_with_faults(
    default_data_dir: &Path,
    active_data_dir: &Path,
    selected_database_path: Option<&Path>,
    data_key: [u8; 32],
    transformer: &dyn LegacySecretTransformer,
    faults: &dyn UpgradeFaultInjector,
) -> Result<(PersistenceRuntime, PathBuf), String> {
    let final_path = active_data_dir.join(DatabaseGeneration::Two.database_file());
    let source_path = active_data_dir.join(DatabaseGeneration::One.database_file());
    let journal_path = default_data_dir.join(UPGRADE_JOURNAL_FILE);
    let io = ProductionUpgradeRecoveryIo {
        default_data_dir,
        active_data_dir,
        source_path: &source_path,
        final_path: &final_path,
        journal_path: &journal_path,
        data_key,
        transformer,
        faults,
        action_error: RefCell::new(None),
    };

    if journal_path.is_file() {
        return resume_journaled_upgrade(&io);
    }

    if selected_database_path.is_some_and(|path| path == final_path) {
        let runtime = open_and_validate_v2(&final_path, data_key, faults)?;
        return Ok((runtime, final_path));
    }

    if final_path.exists() {
        return Err("generation 2 candidate exists without an upgrade journal".to_string());
    }

    match selected_database_path {
        Some(path) if path == source_path && path.is_file() => start_legacy_upgrade(&io),
        Some(_) => {
            Err("selected data store does not match a supported generation file".to_string())
        }
        None => prepare_fresh_install(
            default_data_dir,
            active_data_dir,
            &final_path,
            data_key,
            faults,
        ),
    }
}

fn prepare_fresh_install(
    default_data_dir: &Path,
    active_data_dir: &Path,
    final_path: &Path,
    data_key: [u8; 32],
    faults: &dyn UpgradeFaultInjector,
) -> Result<(PersistenceRuntime, PathBuf), String> {
    let staging_path = final_path.with_extension("sqlite3.first-run.tmp");
    remove_database_artifacts(&staging_path)?;
    let runtime = block_on(PersistenceRuntime::initialize_new(&staging_path))
        .map_err(|error| format!("failed to initialize generation 2 data store: {error}"))?;
    block_on(runtime.close())
        .map_err(|error| format!("failed to close generation 2 staging database: {error}"))?;
    validate_v2_artifact(&staging_path, data_key, faults)?;
    publish_v2_candidate_with_faults(&staging_path, final_path, faults)
        .map_err(redacted_execution_error)?;
    commit_generation_two_config(default_data_dir, active_data_dir, faults)?;
    let runtime = open_and_validate_v2(final_path, data_key, faults)?;
    Ok((runtime, final_path.to_path_buf()))
}

fn start_legacy_upgrade(
    io: &ProductionUpgradeRecoveryIo<'_>,
) -> Result<(PersistenceRuntime, PathBuf), String> {
    check(io.faults, UpgradeFailpoint::SourceOpen)?;
    let profile = block_on(legacy_import::detect_profile(io.source_path))
        .map_err(|error| format!("unsupported v0.3.1 legacy database: {error}"))?;
    check(io.faults, UpgradeFailpoint::SourceIntegrityCheck)?;
    let source_identity = legacy_import::source_candidate_identity(io.source_path)
        .map_err(|error| format!("failed to identify v0.3.1 source: {error}"))?;
    let journal = UpgradeJournal::prepared(
        UpgradeAttemptId::parse(&uuid::Uuid::now_v7().hyphenated().to_string())
            .map_err(|error| error.to_string())?,
        ReleasedSchemaProfile::parse(profile.id()).map_err(|error| error.to_string())?,
        Sha256Digest::parse(&source_identity).map_err(|error| error.to_string())?,
        now_timestamp()?,
    )
    .map_err(|error| error.to_string())?;
    persist_journal(io.journal_path, &journal, io.faults)?;
    run_new_upgrade_attempt(io, journal)
}

fn resume_journaled_upgrade(
    io: &ProductionUpgradeRecoveryIo<'_>,
) -> Result<(PersistenceRuntime, PathBuf), String> {
    let mut deactivated_in_this_run = false;
    loop {
        let observed = io.observe().map_err(redacted_execution_error)?;
        let source_was_generation_one = observed.source == LegacySourceState::Generation1;
        let observed_journal = observe_journal(io.journal_path);
        let execution = RecoveryExecution::prepare(observed, observed_journal.journal.as_ref())
            .map_err(redacted_execution_error)?;
        let plan = execution.plan();
        if plan == RecoveryPlan::Halt(RecoveryHaltReason::UserDecisionRequired)
            && deactivated_in_this_run
        {
            commit_generation_two_config(io.default_data_dir, io.active_data_dir, io.faults)?;
            continue;
        }
        if let Err(error) = RecoveryExecutor::execute(io, &execution) {
            if let Some(action_error) = io.take_action_error() {
                return Err(action_error);
            }
            return Err(recovery_execution_error(error));
        }
        if plan == RecoveryPlan::ActivateValidatedV2 && source_was_generation_one {
            deactivated_in_this_run = true;
        }
        if plan == RecoveryPlan::CleanupCompletedJournal {
            let runtime = open_and_validate_v2(io.final_path, io.data_key, io.faults)?;
            return Ok((runtime, io.final_path.to_path_buf()));
        }
    }
}

fn run_new_upgrade_attempt(
    io: &ProductionUpgradeRecoveryIo<'_>,
    mut journal: UpgradeJournal,
) -> Result<(PersistenceRuntime, PathBuf), String> {
    loop {
        match journal.payload().phase {
            UpgradePhase::Prepared => {
                journal = execute_prepared_step(io, &journal)?;
            }
            UpgradePhase::BackupVerified => {
                journal = execute_backup_verified_step(io, &journal)?;
            }
            UpgradePhase::V2Validated => {
                journal = execute_v2_validated_step(io, &journal)?;
            }
            UpgradePhase::LegacyDeactivated => {
                commit_generation_two_config(io.default_data_dir, io.active_data_dir, io.faults)?;
                journal = record_generation_committed(io.journal_path, &journal, io.faults)?;
            }
            UpgradePhase::GenerationCommitted => {
                let runtime = open_and_validate_v2(io.final_path, io.data_key, io.faults)?;
                record_v2_reopened(io.journal_path, &journal, io.faults)?;
                cleanup_journal(io.journal_path, io.faults)?;
                return Ok((runtime, io.final_path.to_path_buf()));
            }
            UpgradePhase::V2Reopened => {
                let runtime = open_and_validate_v2(io.final_path, io.data_key, io.faults)?;
                cleanup_journal(io.journal_path, io.faults)?;
                return Ok((runtime, io.final_path.to_path_buf()));
            }
        }
    }
}

fn execute_prepared_step(
    io: &ProductionUpgradeRecoveryIo<'_>,
    journal: &UpgradeJournal,
) -> Result<UpgradeJournal, String> {
    let backup_path = backup_path(io.active_data_dir, journal)?;
    remove_owned_backup_if_present(&backup_path)?;
    let temporary_path = temporary_path(io.active_data_dir, journal)?;
    remove_database_artifacts(&temporary_path)?;
    assert_source_identity(io.source_path, journal)?;
    check(io.faults, UpgradeFailpoint::Backup(BackupStep::Create))?;
    block_on(persistence::create_verified_backup_from_path(
        io.source_path,
        &backup_path,
    ))
    .map_err(|error| format!("failed to create verified generation 1 backup: {error}"))?;
    check(io.faults, UpgradeFailpoint::Backup(BackupStep::FileSync))?;
    block_on(persistence::validate_read_only_sqlite(&backup_path))
        .map_err(|error| format!("failed to verify generation 1 backup: {error}"))?;
    check(io.faults, UpgradeFailpoint::Backup(BackupStep::Verify))?;
    let backup_hash = sha256_file(&backup_path).map_err(redacted_execution_error)?;
    let next = journal
        .clone()
        .advance(
            UpgradePhase::BackupVerified,
            Some(backup_hash),
            now_timestamp()?,
        )
        .map_err(|error| error.to_string())?;
    persist_journal(io.journal_path, &next, io.faults)?;
    Ok(next)
}

fn execute_backup_verified_step(
    io: &ProductionUpgradeRecoveryIo<'_>,
    journal: &UpgradeJournal,
) -> Result<UpgradeJournal, String> {
    let backup_path = backup_path(io.active_data_dir, journal)?;
    assert_backup_identity(&backup_path, journal)?;
    let temporary_path = temporary_path(io.active_data_dir, journal)?;
    remove_database_artifacts(&temporary_path)?;
    let runtime = block_on(PersistenceRuntime::initialize_new(&temporary_path))
        .map_err(|error| format!("failed to initialize V2 staging database: {error}"))?;
    let profile = block_on(legacy_import::detect_profile(&backup_path))
        .map_err(|error| format!("verified backup profile is unsupported: {error}"))?;
    let mut injected = None;
    let mut phase_hook = |phase| match io
        .faults
        .check(UpgradeFailpoint::Import(map_import_phase(phase)))
    {
        Ok(()) => Ok(()),
        Err(error) => {
            injected = Some(error);
            Err(UpgradeError::ValidationFailed)
        }
    };
    let import = block_on(legacy_import::import_profile_with_secrets_and_phase_hook(
        &profile,
        &backup_path,
        &runtime.handle(),
        io.transformer,
        &mut phase_hook,
    ));
    let health = block_on(runtime.health());
    let close = block_on(runtime.close());
    if let Some(error) = injected {
        close.map_err(|close_error| {
            format!("failed to close V2 staging database after import failure: {close_error}")
        })?;
        remove_database_artifacts(&temporary_path)?;
        return Err(error.to_string());
    }
    import.map_err(|error| format!("failed to import v0.3.1 backup: {error}"))?;
    let health = health.map_err(|error| format!("V2 staging health check failed: {error}"))?;
    close.map_err(|error| format!("failed to close V2 staging database: {error}"))?;
    if health.open_mode != "writable" {
        return Err("V2 staging database is not writable".to_string());
    }
    check(io.faults, UpgradeFailpoint::CompatibilityValidation)?;
    validate_v2_artifact(&temporary_path, io.data_key, io.faults)?;
    let next = journal
        .clone()
        .advance(UpgradePhase::V2Validated, None, now_timestamp()?)
        .map_err(|error| error.to_string())?;
    persist_journal(io.journal_path, &next, io.faults)?;
    Ok(next)
}

fn execute_v2_validated_step(
    io: &ProductionUpgradeRecoveryIo<'_>,
    journal: &UpgradeJournal,
) -> Result<UpgradeJournal, String> {
    let temporary_path = temporary_path(io.active_data_dir, journal)?;
    if temporary_path.is_file() {
        if io.final_path.exists() {
            return Err(
                "V2 activation precondition failed because final candidate already exists"
                    .to_string(),
            );
        }
        if temporary_path.parent() != io.final_path.parent() {
            return Err(
                "V2 activation precondition failed because artifact roots differ".to_string(),
            );
        }
        publish_v2_candidate_with_faults(&temporary_path, io.final_path, io.faults).map_err(
            |error| format!("V2 activation failed: {}", redacted_execution_error(error)),
        )?;
    } else if !io.final_path.is_file() {
        return Err("validated V2 candidate is missing".to_string());
    }
    validate_v2_artifact(io.final_path, io.data_key, io.faults)?;
    assert_backup_identity(&backup_path(io.active_data_dir, journal)?, journal)?;
    check(io.faults, UpgradeFailpoint::SourceIdentityRecheck)?;
    assert_source_or_tombstone_identity(io.source_path, journal)?;

    // Replacing an already valid tombstone is idempotent and retries WAL/SHM cleanup
    // after a crash between the main-file replacement and sidecar removal.
    replace_legacy_with_tombstone_with_faults(
        io.source_path,
        &journal.payload().attempt_id,
        io.faults,
    )
    .map_err(|error| {
        format!(
            "generation 1 deactivation failed: {}",
            redacted_execution_error(error)
        )
    })?;
    let next = journal
        .clone()
        .advance(UpgradePhase::LegacyDeactivated, None, now_timestamp()?)
        .map_err(|error| error.to_string())?;
    persist_journal(io.journal_path, &next, io.faults)?;
    Ok(next)
}

fn record_generation_committed(
    journal_path: &Path,
    journal: &UpgradeJournal,
    faults: &dyn UpgradeFaultInjector,
) -> Result<UpgradeJournal, String> {
    let next = journal
        .clone()
        .advance(UpgradePhase::GenerationCommitted, None, now_timestamp()?)
        .map_err(|error| error.to_string())?;
    persist_journal(journal_path, &next, faults)?;
    Ok(next)
}

fn record_v2_reopened(
    journal_path: &Path,
    journal: &UpgradeJournal,
    faults: &dyn UpgradeFaultInjector,
) -> Result<UpgradeJournal, String> {
    let next = journal
        .clone()
        .advance(UpgradePhase::V2Reopened, None, now_timestamp()?)
        .map_err(|error| error.to_string())?;
    persist_journal(journal_path, &next, faults)?;
    Ok(next)
}

struct ProductionUpgradeRecoveryIo<'a> {
    default_data_dir: &'a Path,
    active_data_dir: &'a Path,
    source_path: &'a Path,
    final_path: &'a Path,
    journal_path: &'a Path,
    data_key: [u8; 32],
    transformer: &'a dyn LegacySecretTransformer,
    faults: &'a dyn UpgradeFaultInjector,
    action_error: RefCell<Option<String>>,
}

impl ProductionUpgradeRecoveryIo<'_> {
    fn current_journal(&self) -> Result<UpgradeJournal, UpgradeExecutionError> {
        let observed = observe_journal(self.journal_path);
        observed
            .journal
            .ok_or(if observed.state == JournalState::Invalid {
                UpgradeExecutionError::JournalInvalid
            } else {
                UpgradeExecutionError::JournalUnavailable
            })
    }

    fn record_action_result<T>(
        &self,
        result: Result<T, String>,
    ) -> Result<(), UpgradeExecutionError> {
        result.map(|_| ()).map_err(|error| {
            *self.action_error.borrow_mut() = Some(error);
            UpgradeExecutionError::Io(UpgradeIoOperation::Verify)
        })
    }

    fn take_action_error(&self) -> Option<String> {
        self.action_error.borrow_mut().take()
    }
}

impl UpgradeRecoveryIo for ProductionUpgradeRecoveryIo<'_> {
    fn observe(&self) -> Result<ObservedUpgradeState, UpgradeExecutionError> {
        observe_production_upgrade(self)
    }

    fn load_journal(&self) -> Result<UpgradeJournal, UpgradeExecutionError> {
        self.current_journal()
    }

    fn source_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError> {
        let identity = legacy_import::source_candidate_identity(self.source_path)
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Verify))?;
        Sha256Digest::parse(&identity)
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Verify))
    }

    fn backup_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError> {
        let journal = self.current_journal()?;
        let path = backup_path(self.active_data_dir, &journal)
            .map_err(|_| UpgradeExecutionError::RecoveryPreconditionChanged)?;
        sha256_file(&path)
    }

    fn tombstone_attempt_id(&self) -> Result<UpgradeAttemptId, UpgradeExecutionError> {
        read_legacy_tombstone(self.source_path)?
            .ok_or(UpgradeExecutionError::RecoveryPreconditionChanged)
    }

    fn artifact_paths_are_safe(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<bool, UpgradeExecutionError> {
        Ok(journal_artifact_paths_are_safe(
            self.active_data_dir,
            journal,
        ))
    }

    fn restart_from_source(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record_action_result(execute_prepared_step(self, journal))
    }

    fn rebuild_v2_from_verified_backup(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError> {
        self.record_action_result(execute_backup_verified_step(self, journal))
    }

    fn activate_validated_v2(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record_action_result(execute_v2_validated_step(self, journal))
    }

    fn record_legacy_deactivated(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError> {
        self.activate_validated_v2(journal)
    }

    fn restore_generation_1(&self, _journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        Err(UpgradeExecutionError::Halted(
            RecoveryHaltReason::UserDecisionRequired,
        ))
    }

    fn commit_generation_2(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        let config = read_config_v3(&self.default_data_dir.join(DATA_DIR_CONFIG_FILE))
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
        if !config.as_ref().is_some_and(|config| {
            config.database_generation == DatabaseGeneration::Two
                && config.active_data_dir.as_deref() == Some(self.active_data_dir)
        }) {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        }
        self.record_action_result(record_generation_committed(
            self.journal_path,
            journal,
            self.faults,
        ))
    }

    fn reopen_generation_2(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        let runtime = match open_and_validate_v2(self.final_path, self.data_key, self.faults) {
            Ok(runtime) => runtime,
            Err(error) => return self.record_action_result::<()>(Err(error)),
        };
        block_on(runtime.close())
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Write))?;
        self.record_action_result(record_v2_reopened(self.journal_path, journal, self.faults))
    }

    fn cleanup_completed_journal(
        &self,
        _journal: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError> {
        self.record_action_result(cleanup_journal(self.journal_path, self.faults))
    }
}

fn observe_production_upgrade(
    io: &ProductionUpgradeRecoveryIo<'_>,
) -> Result<ObservedUpgradeState, UpgradeExecutionError> {
    let observed_journal = observe_journal(io.journal_path);
    let config = read_config_v3(&io.default_data_dir.join(DATA_DIR_CONFIG_FILE))
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
    let config_generation = match config.as_ref().map(|config| config.database_generation) {
        Some(DatabaseGeneration::Two) => ConfigGeneration::Generation2,
        Some(DatabaseGeneration::One) | None => ConfigGeneration::Generation1,
    };
    let relocation_intent = config.as_ref().is_some_and(|config| {
        config.version == 3
            && config.active_data_dir == config.source_data_dir
            && config.pending_data_dir.is_some()
            && config.pending_data_dir != config.source_data_dir
    });

    let Some(journal) = observed_journal.journal else {
        return Ok(ObservedUpgradeState {
            journal: observed_journal.state,
            config_generation,
            source: observe_legacy_source(io.source_path),
            backup: BackupState::Missing,
            candidate: V2CandidateState::Missing,
            relocation_intent,
            compatibility: CompatibilityState::Unknown,
            orphan_artifacts: io.final_path.exists(),
            sidecars: observe_legacy_sidecars(io.source_path),
        });
    };
    if !journal_artifact_paths_are_safe(io.active_data_dir, &journal) {
        return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
    }
    let backup = journal
        .payload()
        .verified_backup_sha256
        .as_ref()
        .map(|expected| {
            backup_path(io.active_data_dir, &journal)
                .map(|path| observe_backup(&path, expected))
                .unwrap_or(BackupState::Invalid)
        })
        .unwrap_or(BackupState::Missing);
    let (candidate, compatibility) = observe_v2_candidate(io, &journal)?;

    Ok(ObservedUpgradeState {
        journal: observed_journal.state,
        config_generation,
        source: observe_legacy_source(io.source_path),
        backup,
        candidate,
        relocation_intent,
        compatibility,
        orphan_artifacts: false,
        sidecars: observe_legacy_sidecars(io.source_path),
    })
}

fn observe_v2_candidate(
    io: &ProductionUpgradeRecoveryIo<'_>,
    journal: &UpgradeJournal,
) -> Result<(V2CandidateState, CompatibilityState), UpgradeExecutionError> {
    let temporary = temporary_path(io.active_data_dir, journal)
        .map_err(|_| UpgradeExecutionError::RecoveryPreconditionChanged)?;
    let temporary_exists = temporary.is_file();
    let final_exists = io.final_path.is_file();
    if temporary_exists && final_exists {
        return Ok((V2CandidateState::Invalid, CompatibilityState::Unknown));
    }
    if matches!(
        journal.payload().phase,
        UpgradePhase::Prepared | UpgradePhase::BackupVerified
    ) {
        return Ok((
            if temporary_exists {
                V2CandidateState::InactiveTemporary
            } else if final_exists {
                V2CandidateState::Invalid
            } else {
                V2CandidateState::Missing
            },
            CompatibilityState::NotApplicable,
        ));
    }
    let (path, state) = if final_exists {
        (io.final_path, V2CandidateState::ValidatedFinal)
    } else if temporary_exists {
        (temporary.as_path(), V2CandidateState::ValidatedTemporary)
    } else {
        return Ok((V2CandidateState::Missing, CompatibilityState::Unknown));
    };
    if validate_v2_artifact(path, io.data_key, &NoUpgradeFaults).is_ok() {
        Ok((state, CompatibilityState::Writable))
    } else {
        Ok((V2CandidateState::Invalid, CompatibilityState::Incompatible))
    }
}

fn observe_legacy_sidecars(source_path: &Path) -> LegacySidecarState {
    let mut unknown = false;
    for suffix in ["-wal", "-shm"] {
        let mut path = source_path.as_os_str().to_os_string();
        path.push(suffix);
        let path = PathBuf::from(path);
        match fs::metadata(path) {
            Ok(_) => return LegacySidecarState::Present,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => unknown = true,
        }
    }
    if unknown {
        LegacySidecarState::Unknown
    } else {
        LegacySidecarState::Absent
    }
}

fn recovery_execution_error(error: UpgradeExecutionError) -> String {
    match error {
        UpgradeExecutionError::Halted(RecoveryHaltReason::UserDecisionRequired) => {
            "generation 1 is deactivated and requires explicit recovery before config commit"
                .to_string()
        }
        other => redacted_execution_error(other),
    }
}

fn persist_journal(
    journal_path: &Path,
    journal: &UpgradeJournal,
    faults: &dyn UpgradeFaultInjector,
) -> Result<(), String> {
    write_journal_atomically_with_faults(journal_path, journal, faults)
        .map_err(redacted_execution_error)
}

fn cleanup_journal(journal_path: &Path, faults: &dyn UpgradeFaultInjector) -> Result<(), String> {
    remove_file_and_sync_parent_with_faults(journal_path, faults).map_err(redacted_execution_error)
}

fn validate_v2_artifact(
    path: &Path,
    data_key: [u8; 32],
    faults: &dyn UpgradeFaultInjector,
) -> Result<(), String> {
    block_on(persistence::validate_read_only_sqlite(path))
        .map_err(|error| format!("V2 database validation failed: {error}"))?;
    check(faults, UpgradeFailpoint::SecretValidation)?;
    validate_database_secrets(path, &data_key)
}

fn open_and_validate_v2(
    final_path: &Path,
    data_key: [u8; 32],
    faults: &dyn UpgradeFaultInjector,
) -> Result<PersistenceRuntime, String> {
    check(faults, UpgradeFailpoint::V2Reopen)?;
    let runtime = block_on(PersistenceRuntime::open_current(final_path))
        .map_err(|error| format!("failed to open generation 2 database: {error}"))?;
    let health = block_on(runtime.health())
        .map_err(|error| format!("generation 2 health check failed: {error}"))?;
    if health.open_mode != "writable" {
        return Err("generation 2 database opened in inspection-only mode".to_string());
    }
    validate_v2_artifact(final_path, data_key, faults)?;
    Ok(runtime)
}

fn commit_generation_two_config(
    default_data_dir: &Path,
    active_data_dir: &Path,
    faults: &dyn UpgradeFaultInjector,
) -> Result<(), String> {
    let config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
    let previous = read_config_v3(&config_path)?;
    if previous.as_ref().is_some_and(|config| {
        config.database_generation == DatabaseGeneration::Two
            && config.active_data_dir.as_deref() == Some(active_data_dir)
    }) && installation_marker_exists(default_data_dir)
    {
        return Ok(());
    }
    write_config_v3_with_faults(
        &config_path,
        &DataDirConfigV3 {
            version: 3,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: previous
                .as_ref()
                .and_then(|config| config.pending_data_dir.clone()),
            source_data_dir: previous
                .as_ref()
                .and_then(|config| config.source_data_dir.clone()),
            database_generation: DatabaseGeneration::Two,
            updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        },
        faults,
    )?;
    create_installation_marker(default_data_dir)?;
    Ok(())
}

fn assert_source_identity(source_path: &Path, journal: &UpgradeJournal) -> Result<(), String> {
    let identity = legacy_import::source_candidate_identity(source_path)
        .map_err(|error| format!("failed to verify generation 1 source identity: {error}"))?;
    if identity != journal.payload().source_candidate_identity.as_str() {
        return Err("generation 1 source identity changed during upgrade".to_string());
    }
    Ok(())
}

fn assert_source_or_tombstone_identity(
    source_path: &Path,
    journal: &UpgradeJournal,
) -> Result<(), String> {
    if let Some(attempt) = read_legacy_tombstone(source_path).map_err(redacted_execution_error)? {
        if attempt == journal.payload().attempt_id {
            return Ok(());
        }
        return Err("generation 1 tombstone belongs to another upgrade attempt".to_string());
    }
    assert_source_identity(source_path, journal)
}

fn assert_backup_identity(backup_path: &Path, journal: &UpgradeJournal) -> Result<(), String> {
    let expected = journal
        .payload()
        .verified_backup_sha256
        .as_ref()
        .ok_or_else(|| "upgrade journal does not contain a verified backup hash".to_string())?;
    let actual = sha256_file(backup_path).map_err(redacted_execution_error)?;
    if &actual != expected {
        return Err("verified generation 1 backup identity changed".to_string());
    }
    Ok(())
}

fn backup_path(root: &Path, journal: &UpgradeJournal) -> Result<PathBuf, String> {
    resolve_allowlisted_artifact(root, journal.payload().paths.backup()).map_err(|error| {
        format!(
            "backup artifact path rejected: {}",
            redacted_execution_error(error)
        )
    })
}

fn temporary_path(root: &Path, journal: &UpgradeJournal) -> Result<PathBuf, String> {
    resolve_allowlisted_artifact(root, journal.payload().paths.v2_temporary()).map_err(|error| {
        format!(
            "V2 temporary path rejected: {}",
            redacted_execution_error(error)
        )
    })
}

fn remove_owned_backup_if_present(path: &Path) -> Result<(), String> {
    let temporary = persistence::temporary_backup_path(path);
    for candidate in [path, temporary.as_path()] {
        if candidate.is_file() {
            fs::remove_file(candidate)
                .map_err(|error| format!("failed to reset owned backup artifact: {error}"))?;
        }
    }
    Ok(())
}

fn remove_database_artifacts(path: &Path) -> Result<(), String> {
    for artifact in [
        path.to_path_buf(),
        PathBuf::from(format!("{}-wal", path.display())),
        PathBuf::from(format!("{}-shm", path.display())),
    ] {
        if artifact.is_file() {
            fs::remove_file(&artifact)
                .map_err(|error| format!("failed to remove owned V2 artifact: {error}"))?;
        }
    }
    Ok(())
}

fn map_import_phase(phase: LegacyImportPhase) -> ImportPhase {
    match phase {
        LegacyImportPhase::SettingsAndInstallation => ImportPhase::SettingsAndInstallation,
        LegacyImportPhase::StationsAndEndpointRevision => ImportPhase::StationsAndEndpointRevisions,
        LegacyImportPhase::SecretsAndCredentials => ImportPhase::CredentialsAndSecretReferences,
        LegacyImportPhase::KeysAndCapabilities => ImportPhase::StationKeysAndCapabilities,
        LegacyImportPhase::RoutingGroupsAliasesRemoteKeys => {
            ImportPhase::GroupsRoutingAndModelAliases
        }
        LegacyImportPhase::MonitorDefinitions => ImportPhase::ChannelMonitors,
        LegacyImportPhase::Pricing => ImportPhase::Pricing,
        LegacyImportPhase::HistoricalEvidence => ImportPhase::EvidenceAndHistory,
        LegacyImportPhase::HealthAndChanges => ImportPhase::HealthAndChangeEvents,
        LegacyImportPhase::DerivedProjectionsAndIndexes => {
            ImportPhase::DerivedProjectionsAndIndexes
        }
    }
}

fn now_timestamp() -> Result<UtcTimestamp, String> {
    UtcTimestamp::parse(&Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true))
        .map_err(|error| error.to_string())
}

fn check(faults: &dyn UpgradeFaultInjector, failpoint: UpgradeFailpoint) -> Result<(), String> {
    faults.check(failpoint).map_err(|error| error.to_string())
}

fn block_on<F: std::future::Future>(future: F) -> F::Output {
    tauri::async_runtime::block_on(future)
}

fn redacted_execution_error(
    error: persistence::upgrade_recovery_executor::UpgradeExecutionError,
) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;
    use crate::{
        persistence::{
            upgrade_fault::{AtomicStep, UpgradeInjectedFailure, UPGRADE_INJECTED_FAILURE_CODE},
            upgrade_recovery_executor::observe_journal,
            upgrade_recovery_plan::JournalState,
        },
        services::secrets,
    };

    #[test]
    fn released_v031_fixture_upgrades_and_restarts_on_generation_two() {
        let fixture = UpgradeFixture::new("complete");
        let before = fs::read(&fixture.source).expect("source bytes");

        let (runtime, final_path) = fixture.run(None).expect("upgrade");

        assert_eq!(final_path, fixture.final_path);
        assert!(read_legacy_tombstone(&fixture.source)
            .expect("tombstone")
            .is_some());
        assert_ne!(fs::read(&fixture.source).expect("tombstone bytes"), before);
        assert!(!fixture.journal.exists());
        assert!(fixture.backup_files().iter().any(|path| path.is_file()));
        let config = read_config_v3(&fixture.config)
            .expect("config")
            .expect("config present");
        assert_eq!(config.database_generation, DatabaseGeneration::Two);
        block_on(runtime.close()).expect("close upgraded runtime");

        let (restarted, restarted_path) = fixture
            .run_with_selected(Some(&fixture.final_path), None)
            .expect("restart generation two");
        assert_eq!(restarted_path, fixture.final_path);
        block_on(restarted.close()).expect("close restarted runtime");
    }

    #[test]
    fn every_import_phase_failure_rolls_back_and_restarts_from_verified_backup() {
        for (index, phase) in ImportPhase::ALL.into_iter().enumerate() {
            let fixture = UpgradeFixture::new(&format!("import-failure-{index}"));
            let before = fs::read(&fixture.source).expect("source bytes");
            let injector = OneShotFault::new(UpgradeFailpoint::Import(phase));

            let error = fixture.run(Some(&injector)).expect_err("injected failure");

            assert!(error.contains(UPGRADE_INJECTED_FAILURE_CODE));
            assert_eq!(fs::read(&fixture.source).expect("source unchanged"), before);
            assert!(!fixture.final_path.exists());
            assert!(read_config_v3(&fixture.config).expect("config").is_none());
            assert_eq!(
                observe_journal(&fixture.journal).state,
                JournalState::Valid(UpgradePhase::BackupVerified)
            );

            let (runtime, _) = fixture.run(None).expect("deterministic restart");
            assert!(!fixture.journal.exists());
            assert!(read_legacy_tombstone(&fixture.source)
                .expect("tombstone")
                .is_some());
            block_on(runtime.close()).expect("close recovered runtime");
        }
    }

    #[test]
    fn every_durable_journal_phase_has_a_deterministic_restart() {
        for (index, phase) in UpgradePhase::ALL.into_iter().enumerate() {
            let fixture = UpgradeFixture::new(&format!("journal-phase-{index}"));
            let injector = OneShotFault::new(UpgradeFailpoint::Journal {
                phase,
                edge: AtomicStep::AfterDurableSync,
            });

            let error = fixture.run(Some(&injector)).expect_err("journal fault");
            assert!(error.contains(UPGRADE_INJECTED_FAILURE_CODE));
            assert_eq!(
                observe_journal(&fixture.journal).state,
                JournalState::Valid(phase)
            );

            if phase == UpgradePhase::LegacyDeactivated {
                let recovery = fixture.run(None).expect_err("explicit recovery required");
                assert!(recovery.contains("explicit recovery"));
                assert!(commit_explicit_generation_two_recovery(
                    &fixture.default_data_dir,
                    &fixture.final_path,
                    fixture.data_key,
                )
                .expect("explicit generation 2 activation"));
            }

            let (runtime, _) = fixture.run(None).expect("restart converges");
            assert!(!fixture.journal.exists());
            block_on(runtime.close()).expect("close restarted runtime");
        }
    }

    #[test]
    fn recovery_retries_sidecar_cleanup_after_tombstone_is_durable() {
        let fixture = UpgradeFixture::new("tombstone-sidecar-retry");
        let injector = OneShotFault::new(UpgradeFailpoint::Tombstone(
            crate::persistence::upgrade_fault::TombstoneStep::AfterDurableSync,
        ));

        let error = fixture.run(Some(&injector)).expect_err("tombstone fault");
        assert!(error.contains(UPGRADE_INJECTED_FAILURE_CODE));
        assert_eq!(
            observe_journal(&fixture.journal).state,
            JournalState::Valid(UpgradePhase::V2Validated)
        );
        assert!(read_legacy_tombstone(&fixture.source)
            .expect("durable tombstone")
            .is_some());

        let wal = PathBuf::from(format!("{}-wal", fixture.source.display()));
        let shm = PathBuf::from(format!("{}-shm", fixture.source.display()));
        fs::write(&wal, b"stale wal").expect("seed stale wal");
        fs::write(&shm, b"stale shm").expect("seed stale shm");

        let recovery = fixture.run(None).expect_err("explicit recovery required");
        assert!(recovery.contains("explicit recovery"));
        assert!(!wal.exists());
        assert!(!shm.exists());
        assert_eq!(
            observe_journal(&fixture.journal).state,
            JournalState::Valid(UpgradePhase::LegacyDeactivated)
        );
        assert!(read_config_v3(&fixture.config)
            .expect("config remains readable")
            .is_none());
    }

    struct UpgradeFixture {
        _root: tempfile::TempDir,
        default_data_dir: PathBuf,
        active_data_dir: PathBuf,
        source: PathBuf,
        final_path: PathBuf,
        journal: PathBuf,
        config: PathBuf,
        data_key: [u8; 32],
    }

    impl UpgradeFixture {
        fn new(label: &str) -> Self {
            let root = tempfile::Builder::new()
                .prefix(&format!("persistence-generation-upgrade-{label}-"))
                .tempdir()
                .expect("tempdir");
            let default_data_dir = root.path().join("default");
            let active_data_dir = root.path().join("active");
            fs::create_dir_all(&default_data_dir).expect("default dir");
            fs::create_dir_all(&active_data_dir).expect("active dir");
            let source = active_data_dir.join(DatabaseGeneration::One.database_file());
            fs::copy(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("tests/persistence_upgrade/fixtures/profile_001/source.sqlite3"),
                &source,
            )
            .expect("copy released fixture");
            Self {
                final_path: active_data_dir.join(DatabaseGeneration::Two.database_file()),
                journal: default_data_dir.join(UPGRADE_JOURNAL_FILE),
                config: default_data_dir.join(DATA_DIR_CONFIG_FILE),
                data_key: secrets::crypto::generate_data_key(),
                _root: root,
                default_data_dir,
                active_data_dir,
                source,
            }
        }

        fn run(
            &self,
            injector: Option<&dyn UpgradeFaultInjector>,
        ) -> Result<(PersistenceRuntime, PathBuf), String> {
            self.run_with_selected(Some(&self.source), injector)
        }

        fn run_with_selected(
            &self,
            selected: Option<&Path>,
            injector: Option<&dyn UpgradeFaultInjector>,
        ) -> Result<(PersistenceRuntime, PathBuf), String> {
            let transformer = DataKeyLegacyTransformer {
                data_key: self.data_key,
            };
            prepare_generation_two_with_faults(
                &self.default_data_dir,
                &self.active_data_dir,
                selected,
                self.data_key,
                &transformer,
                injector.unwrap_or(&NoUpgradeFaults),
            )
        }

        fn backup_files(&self) -> Vec<PathBuf> {
            let root = self.active_data_dir.join("backups");
            let mut files = Vec::new();
            if let Ok(attempts) = fs::read_dir(root) {
                for attempt in attempts.flatten() {
                    files.push(attempt.path().join(DatabaseGeneration::One.database_file()));
                }
            }
            files
        }
    }

    struct OneShotFault {
        target: UpgradeFailpoint,
        fired: Mutex<bool>,
    }

    impl OneShotFault {
        fn new(target: UpgradeFailpoint) -> Self {
            Self {
                target,
                fired: Mutex::new(false),
            }
        }
    }

    impl UpgradeFaultInjector for OneShotFault {
        fn check(&self, failpoint: UpgradeFailpoint) -> Result<(), UpgradeInjectedFailure> {
            let mut fired = self.fired.lock().expect("fault mutex");
            if !*fired && failpoint == self.target {
                *fired = true;
                return Err(UpgradeInjectedFailure::new(failpoint));
            }
            Ok(())
        }
    }
}
