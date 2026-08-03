use std::{fs, path::Path};

use serde::Serialize;

use crate::{
    background_tasks::BlockingExecutor,
    persistence::validate_read_only_sqlite,
    services::{
        data_store::{
            atomic_file::{
                ApprovedLeaf, AtomicDatabaseReplacePort, AtomicFileError, AtomicJournalPort,
                LocalAtomicFileAdapter,
            },
            file_identity::{identity_for_path, FileIdentity, FileIdentityError},
        },
        secrets::{validation::validate_database_secrets_with_resolver, DeviceKeyResolver},
    },
};

use super::activation_journal::{
    read_journal, remove_journal, write_journal, PortableActivationArtifact,
    PortableActivationJournal, PortableActivationJournalError, PortableActivationPhase,
};

const ACTIVATION_RECEIPT_FILE: &str = "portable-migration-activation-receipt.json";

#[derive(Debug, Clone)]
pub(crate) enum PortableActivationStartup {
    NoJournal,
    Activated {
        operation_id: String,
        target_key_id: String,
    },
    RolledBack {
        operation_id: String,
    },
    ManualRecoveryRequired {
        reason: PortableActivationManualReason,
    },
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableActivationManualReason {
    PathRejected,
    MissingArtifact,
    IdentityMismatch,
    ReplacementFailed,
    NewActiveInvalid,
    RollbackFailed,
    KeyUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecoveryPlan {
    ReplaceStaged,
    ValidateNewActive,
    CompleteAlreadyValidated,
    KeepRolledBack,
    Manual(PortableActivationManualReason),
}

#[derive(Debug, Clone)]
struct ObservedArtifacts {
    active: Option<FileIdentity>,
    staged: Option<FileIdentity>,
    rollback: Option<FileIdentity>,
    backup: Option<FileIdentity>,
}

pub(crate) async fn recover_portable_activation_for_startup(
    config_dir: &Path,
    default_data_dir: &Path,
    blocking: BlockingExecutor,
) -> Result<PortableActivationStartup, String> {
    let Some(journal) = read_startup_journal(config_dir)? else {
        return Ok(PortableActivationStartup::NoJournal);
    };
    let target_key_id = journal.payload.target_device_key_id.clone();
    let manager = match crate::services::secrets::SecretManager::load_by_key_id(
        blocking,
        target_key_id.clone(),
    )
    .await
    {
        Ok(manager) => manager,
        Err(_) => {
            return Ok(manual_from_journal(
                PortableActivationManualReason::KeyUnavailable,
            ));
        }
    };
    let target_keys = manager.resolver();
    recover_portable_activation_with_resolver(
        config_dir,
        default_data_dir,
        target_keys,
        &LocalAtomicFileAdapter,
    )
    .await
}

pub(crate) async fn recover_portable_activation_with_resolver(
    config_dir: &Path,
    default_data_dir: &Path,
    target_keys: DeviceKeyResolver,
    replace_port: &dyn AtomicDatabaseReplacePort,
) -> Result<PortableActivationStartup, String> {
    let Some(journal) = read_startup_journal(config_dir)? else {
        return Ok(PortableActivationStartup::NoJournal);
    };
    if let Err(reason) = validate_journal_paths(default_data_dir, &journal) {
        let manual = journal
            .advance(PortableActivationPhase::ManualRecoveryRequired, None)
            .unwrap_or_else(|_| journal.clone());
        let _ = write_journal(config_dir, &manual);
        return Ok(manual_from_journal(reason));
    }

    let mut journal = journal;
    loop {
        let observed = observe_artifacts(&journal);
        let plan = plan_recovery(&journal, &observed);
        match plan {
            RecoveryPlan::ReplaceStaged => {
                journal = persist_phase(
                    config_dir,
                    &journal,
                    PortableActivationPhase::ActivationStarted,
                    None,
                )?;
                let active = match approve_artifact_leaf(&journal.payload.active) {
                    Ok(active) => active,
                    Err(_) => {
                        return Ok(mark_manual(
                            config_dir,
                            &journal,
                            PortableActivationManualReason::PathRejected,
                        ));
                    }
                };
                let rollback = match approve_artifact_leaf(&journal.payload.rollback) {
                    Ok(rollback) => rollback,
                    Err(_) => {
                        return Ok(mark_manual(
                            config_dir,
                            &journal,
                            PortableActivationManualReason::PathRejected,
                        ));
                    }
                };
                match replace_port.replace_with_rollback(
                    &journal.payload.staged.path,
                    &active,
                    &rollback,
                ) {
                    Ok(evidence) => {
                        let rollback_file_id = evidence
                            .rollback
                            .identity
                            .file_id
                            .or(Some(evidence.rollback.identity.length as u128));
                        if !artifact_matches_identity(
                            &journal.payload.staged,
                            &evidence.active.identity,
                        ) || !artifact_matches_identity(
                            &journal.payload.rollback,
                            &evidence.rollback.identity,
                        ) {
                            return Ok(mark_manual(
                                config_dir,
                                &journal,
                                PortableActivationManualReason::IdentityMismatch,
                            ));
                        }
                        journal = persist_phase(
                            config_dir,
                            &journal,
                            PortableActivationPhase::ReplacementCommitted,
                            rollback_file_id,
                        )?;
                    }
                    Err(_) => {
                        return Ok(mark_manual(
                            config_dir,
                            &journal,
                            PortableActivationManualReason::ReplacementFailed,
                        ));
                    }
                }
            }
            RecoveryPlan::ValidateNewActive | RecoveryPlan::CompleteAlreadyValidated => {
                if plan == RecoveryPlan::ValidateNewActive {
                    if let Err(_error) = validate_new_active(&journal, &target_keys).await {
                        return rollback_or_manual(config_dir, &journal, replace_port);
                    }
                    journal = persist_phase(
                        config_dir,
                        &journal,
                        PortableActivationPhase::ActivatedValidated,
                        journal.payload.observed_rollback_file_id,
                    )?;
                }
                return Ok(PortableActivationStartup::Activated {
                    operation_id: journal.payload.operation_id,
                    target_key_id: journal.payload.target_device_key_id,
                });
            }
            RecoveryPlan::KeepRolledBack => {
                return Ok(PortableActivationStartup::RolledBack {
                    operation_id: journal.payload.operation_id,
                });
            }
            RecoveryPlan::Manual(reason) => return Ok(mark_manual(config_dir, &journal, reason)),
        }
    }
}

pub(crate) fn complete_portable_activation(
    config_dir: &Path,
    operation_id: &str,
    outcome: &str,
) -> Result<(), String> {
    let Some(journal) = read_journal(config_dir).map_err(|error| error.to_string())? else {
        return Ok(());
    };
    if journal.payload.operation_id != operation_id {
        return Err("portable activation receipt operation mismatch".to_string());
    }
    if !matches!(
        journal.payload.phase,
        PortableActivationPhase::ActivatedValidated | PortableActivationPhase::RolledBack
    ) {
        return Err("portable activation is not ready to complete".to_string());
    }
    let receipt = PortableActivationReceipt {
        version: 1,
        operation_id: operation_id.to_string(),
        outcome: outcome.to_string(),
        target_device_key_id: journal.payload.target_device_key_id.clone(),
        active_sha256: journal.payload.active.sha256.clone(),
        backup_sha256: journal.payload.backup.sha256.clone(),
        completed_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
    };
    write_receipt(config_dir, &receipt)?;
    let completed = journal
        .advance(
            PortableActivationPhase::Completed,
            journal.payload.observed_rollback_file_id,
        )
        .map_err(|error| error.to_string())?;
    write_journal(config_dir, &completed).map_err(|error| error.to_string())?;
    remove_journal(config_dir).map_err(|error| error.to_string())
}

fn read_startup_journal(config_dir: &Path) -> Result<Option<PortableActivationJournal>, String> {
    match read_journal(config_dir) {
        Ok(journal) => Ok(journal),
        Err(PortableActivationJournalError::UnsupportedVersion) => {
            Err("portable activation journal version is unsupported".to_string())
        }
        Err(error) => Err(format!("portable activation journal is malformed: {error}")),
    }
}

fn validate_journal_paths(
    default_data_dir: &Path,
    journal: &PortableActivationJournal,
) -> Result<(), PortableActivationManualReason> {
    let active_parent = journal
        .payload
        .active
        .path
        .parent()
        .ok_or(PortableActivationManualReason::PathRejected)?;
    let active_parent_canonical = active_parent
        .canonicalize()
        .map_err(|_| PortableActivationManualReason::PathRejected)?;
    let default_data_dir_canonical = default_data_dir
        .canonicalize()
        .unwrap_or_else(|_| default_data_dir.to_path_buf());
    if !active_parent_canonical.starts_with(&default_data_dir_canonical)
        && journal.payload.active.path != default_data_dir.join("relay-pool-desktop-v2.sqlite3")
    {
        return Err(PortableActivationManualReason::PathRejected);
    }
    for artifact in [
        &journal.payload.active,
        &journal.payload.staged,
        &journal.payload.rollback,
    ] {
        let parent = artifact
            .path
            .parent()
            .ok_or(PortableActivationManualReason::PathRejected)?;
        if parent
            .canonicalize()
            .map_err(|_| PortableActivationManualReason::PathRejected)?
            != active_parent_canonical
        {
            return Err(PortableActivationManualReason::PathRejected);
        }
    }
    if !journal.payload.backup.path.is_absolute() {
        return Err(PortableActivationManualReason::PathRejected);
    }
    Ok(())
}

fn plan_recovery(
    journal: &PortableActivationJournal,
    observed: &ObservedArtifacts,
) -> RecoveryPlan {
    let active_old = observed
        .active
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.active, identity));
    let active_new = observed
        .active
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.staged, identity));
    let staged_new = observed
        .staged
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.staged, identity));
    let rollback_old = observed
        .rollback
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.rollback, identity));
    let backup_ok = observed
        .backup
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.backup, identity));
    if !backup_ok {
        return RecoveryPlan::Manual(PortableActivationManualReason::MissingArtifact);
    }
    match journal.payload.phase {
        PortableActivationPhase::Prepared | PortableActivationPhase::ActivationStarted => {
            if active_old && staged_new {
                RecoveryPlan::ReplaceStaged
            } else if active_new && rollback_old {
                RecoveryPlan::ValidateNewActive
            } else {
                RecoveryPlan::Manual(PortableActivationManualReason::IdentityMismatch)
            }
        }
        PortableActivationPhase::ReplacementCommitted
        | PortableActivationPhase::RollbackStarted => {
            if active_new && rollback_old {
                RecoveryPlan::ValidateNewActive
            } else if active_old && staged_new {
                RecoveryPlan::ReplaceStaged
            } else {
                RecoveryPlan::Manual(PortableActivationManualReason::IdentityMismatch)
            }
        }
        PortableActivationPhase::ActivatedValidated => {
            if active_new && rollback_old {
                RecoveryPlan::CompleteAlreadyValidated
            } else {
                RecoveryPlan::Manual(PortableActivationManualReason::IdentityMismatch)
            }
        }
        PortableActivationPhase::RolledBack => {
            if active_old {
                RecoveryPlan::KeepRolledBack
            } else {
                RecoveryPlan::Manual(PortableActivationManualReason::IdentityMismatch)
            }
        }
        PortableActivationPhase::ManualRecoveryRequired => {
            RecoveryPlan::Manual(PortableActivationManualReason::IdentityMismatch)
        }
        PortableActivationPhase::Completed => RecoveryPlan::CompleteAlreadyValidated,
    }
}

async fn validate_new_active(
    journal: &PortableActivationJournal,
    target_keys: &DeviceKeyResolver,
) -> Result<(), String> {
    validate_read_only_sqlite(&journal.payload.active.path)
        .await
        .map_err(|error| format!("new active sqlite validation failed: {error}"))?;
    validate_database_secrets_with_resolver(
        &journal.payload.active.path,
        target_keys,
        &journal.payload.target_device_key_id,
    )
}

fn rollback_or_manual(
    config_dir: &Path,
    journal: &PortableActivationJournal,
    replace_port: &dyn AtomicDatabaseReplacePort,
) -> Result<PortableActivationStartup, String> {
    let observed = observe_artifacts(journal);
    let active_new = observed
        .active
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.staged, identity));
    let rollback_old = observed
        .rollback
        .as_ref()
        .is_some_and(|identity| artifact_matches_identity(&journal.payload.rollback, identity));
    if !(active_new && rollback_old) {
        return Ok(mark_manual(
            config_dir,
            journal,
            PortableActivationManualReason::NewActiveInvalid,
        ));
    }
    let rollback_started = persist_phase(
        config_dir,
        journal,
        PortableActivationPhase::RollbackStarted,
        journal.payload.observed_rollback_file_id.or(Some(0)),
    )?;
    let active = match approve_artifact_leaf(&rollback_started.payload.active) {
        Ok(active) => active,
        Err(_) => {
            return Ok(mark_manual(
                config_dir,
                &rollback_started,
                PortableActivationManualReason::PathRejected,
            ));
        }
    };
    let staged_failed_new = match approve_artifact_leaf(&rollback_started.payload.staged) {
        Ok(staged_failed_new) => staged_failed_new,
        Err(_) => {
            return Ok(mark_manual(
                config_dir,
                &rollback_started,
                PortableActivationManualReason::PathRejected,
            ));
        }
    };
    match replace_port.replace_with_rollback(
        &rollback_started.payload.rollback.path,
        &active,
        &staged_failed_new,
    ) {
        Ok(evidence) => {
            if !artifact_matches_identity(
                &rollback_started.payload.active,
                &evidence.active.identity,
            ) {
                return Ok(mark_manual(
                    config_dir,
                    &rollback_started,
                    PortableActivationManualReason::RollbackFailed,
                ));
            }
            let rolled_back = persist_phase(
                config_dir,
                &rollback_started,
                PortableActivationPhase::RolledBack,
                rollback_started.payload.observed_rollback_file_id,
            )?;
            Ok(PortableActivationStartup::RolledBack {
                operation_id: rolled_back.payload.operation_id,
            })
        }
        Err(_) => Ok(mark_manual(
            config_dir,
            &rollback_started,
            PortableActivationManualReason::RollbackFailed,
        )),
    }
}

fn persist_phase(
    config_dir: &Path,
    journal: &PortableActivationJournal,
    phase: PortableActivationPhase,
    observed_rollback_file_id: Option<u128>,
) -> Result<PortableActivationJournal, String> {
    let next = journal
        .advance(phase, observed_rollback_file_id)
        .map_err(|error| error.to_string())?;
    write_journal(config_dir, &next).map_err(|error| error.to_string())?;
    read_journal(config_dir)
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "portable activation journal disappeared after publish".to_string())
}

fn mark_manual(
    config_dir: &Path,
    journal: &PortableActivationJournal,
    reason: PortableActivationManualReason,
) -> PortableActivationStartup {
    let manual = journal
        .advance(
            PortableActivationPhase::ManualRecoveryRequired,
            journal.payload.observed_rollback_file_id,
        )
        .unwrap_or_else(|_| journal.clone());
    let _ = write_journal(config_dir, &manual);
    manual_from_journal(reason)
}

fn manual_from_journal(reason: PortableActivationManualReason) -> PortableActivationStartup {
    PortableActivationStartup::ManualRecoveryRequired { reason }
}

fn observe_artifacts(journal: &PortableActivationJournal) -> ObservedArtifacts {
    ObservedArtifacts {
        active: optional_identity(&journal.payload.active.path),
        staged: optional_identity(&journal.payload.staged.path),
        rollback: optional_identity(&journal.payload.rollback.path),
        backup: optional_identity(&journal.payload.backup.path),
    }
}

fn optional_identity(path: &Path) -> Option<FileIdentity> {
    match identity_for_path(path) {
        Ok(identity) => Some(identity),
        Err(FileIdentityError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => None,
    }
}

fn artifact_matches_identity(
    artifact: &PortableActivationArtifact,
    identity: &FileIdentity,
) -> bool {
    artifact.length == identity.length
        && artifact.sha256 == identity.sha256
        && artifact
            .volume_serial
            .is_none_or(|expected| identity.volume_serial == Some(expected))
        && artifact
            .file_id
            .is_none_or(|expected| identity.file_id == Some(expected))
}

fn approve_artifact_leaf(
    artifact: &PortableActivationArtifact,
) -> Result<ApprovedLeaf, AtomicFileError> {
    let parent = artifact
        .path
        .parent()
        .ok_or(AtomicFileError::PathRejected)?;
    let leaf = artifact
        .path
        .file_name()
        .ok_or(AtomicFileError::PathRejected)?
        .to_os_string();
    ApprovedLeaf::approve(parent, leaf)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PortableActivationReceipt {
    version: u32,
    operation_id: String,
    outcome: String,
    target_device_key_id: String,
    active_sha256: String,
    backup_sha256: String,
    completed_at: String,
}

fn write_receipt(config_dir: &Path, receipt: &PortableActivationReceipt) -> Result<(), String> {
    fs::create_dir_all(config_dir)
        .map_err(|error| format!("failed to create portable activation receipt dir: {error}"))?;
    let path = config_dir.join(ACTIVATION_RECEIPT_FILE);
    let bytes = serde_json::to_vec_pretty(receipt)
        .map_err(|error| format!("failed to serialize portable activation receipt: {error}"))?;
    let target = ApprovedLeaf::approve(config_dir, ACTIVATION_RECEIPT_FILE)
        .map_err(|error| format!("failed to approve portable activation receipt path: {error}"))?;
    let readback = crate::services::data_store::atomic_file::LocalAtomicFileAdapter
        .publish_and_readback(&bytes, &target)
        .map_err(|error| format!("failed to publish portable activation receipt: {error}"))?;
    if readback != bytes {
        return Err("portable activation receipt readback mismatch".to_string());
    }
    if !path.is_file() {
        return Err("portable activation receipt missing after publish".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;
    use crate::services::portable_migration::activation_journal::{
        write_prepared_journal, PortableActivationArtifact, ACTIVATION_JOURNAL_FILE,
    };

    #[test]
    fn recovery_plan_is_closed_over_phase_and_file_state() {
        let root = tempfile::tempdir().expect("tempdir");
        let journal = sample_journal(root.path(), false);
        let old = identity(b"old");
        let new = identity(b"new");
        let backup = identity(b"backup");

        let prepared = ObservedArtifacts {
            active: Some(old.clone()),
            staged: Some(new.clone()),
            rollback: None,
            backup: Some(backup.clone()),
        };
        assert_eq!(
            plan_recovery(&journal, &prepared),
            RecoveryPlan::ReplaceStaged
        );

        let committed = journal
            .advance(PortableActivationPhase::ReplacementCommitted, Some(7))
            .expect("advance");
        let replaced = ObservedArtifacts {
            active: Some(new),
            staged: None,
            rollback: Some(old),
            backup: Some(backup),
        };
        assert_eq!(
            plan_recovery(&committed, &replaced),
            RecoveryPlan::ValidateNewActive
        );

        let missing = ObservedArtifacts {
            active: None,
            staged: None,
            rollback: None,
            backup: None,
        };
        assert!(matches!(
            plan_recovery(&committed, &missing),
            RecoveryPlan::Manual(_)
        ));
    }

    #[tokio::test]
    async fn prepared_journal_replaces_staged_and_returns_target_key() {
        let root = tempfile::tempdir().expect("tempdir");
        let config = root.path().join("config");
        let data = root.path().join("data");
        fs::create_dir_all(&config).expect("config");
        fs::create_dir_all(&data).expect("data");
        let active = data.join("relay-pool-desktop-v2.sqlite3");
        let staged = data.join("staged.sqlite3");
        let rollback = data.join("rollback.sqlite3");
        let backup = data.join("backup.sqlite3");
        fs::write(&active, b"old").expect("active");
        fs::write(&staged, b"new").expect("staged");
        fs::write(&backup, b"backup").expect("backup");
        let journal = journal_for_paths(&active, &staged, &rollback, &backup);
        write_prepared_journal(&config, &journal).expect("journal");

        let outcome = recover_portable_activation_with_resolver(
            &config,
            &data,
            DeviceKeyResolver::for_test([3; 32]),
            &LocalAtomicFileAdapter,
        )
        .await
        .expect("recover");

        assert!(matches!(
            outcome,
            PortableActivationStartup::ManualRecoveryRequired {
                reason: PortableActivationManualReason::NewActiveInvalid,
                ..
            } | PortableActivationStartup::RolledBack { .. }
        ));
        assert_eq!(fs::read(&active).expect("active restored"), b"old");
    }

    #[test]
    fn malformed_journal_is_not_treated_as_absent() {
        let root = tempfile::tempdir().expect("tempdir");
        let config = root.path().join("config");
        fs::create_dir_all(&config).expect("config");
        fs::write(config.join(ACTIVATION_JOURNAL_FILE), b"{not-json").expect("bad journal");

        let error = read_startup_journal(&config).expect_err("malformed");

        assert!(error.contains("malformed"));
    }

    fn sample_journal(root: &Path, files: bool) -> PortableActivationJournal {
        let active = root.join("active.sqlite3");
        let staged = root.join("staged.sqlite3");
        let rollback = root.join("rollback.sqlite3");
        let backup = root.join("backup.sqlite3");
        if files {
            fs::write(&active, b"old").expect("active");
            fs::write(&staged, b"new").expect("staged");
            fs::write(&backup, b"backup").expect("backup");
        }
        journal_for_paths(&active, &staged, &rollback, &backup)
    }

    fn journal_for_paths(
        active: &Path,
        staged: &Path,
        rollback: &Path,
        backup: &Path,
    ) -> PortableActivationJournal {
        PortableActivationJournal::prepared(
            uuid::Uuid::now_v7().to_string(),
            "replaceCurrent".to_string(),
            "test-device-key".to_string(),
            artifact(active, b"old"),
            artifact(staged, b"new"),
            artifact(rollback, b"old"),
            artifact(backup, b"backup"),
        )
        .expect("journal")
    }

    fn artifact(path: &Path, bytes: &[u8]) -> PortableActivationArtifact {
        PortableActivationArtifact {
            path: path.to_path_buf(),
            volume_serial: None,
            file_id: None,
            length: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }

    fn identity(bytes: &[u8]) -> FileIdentity {
        FileIdentity {
            volume_serial: None,
            file_id: None,
            length: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
}
