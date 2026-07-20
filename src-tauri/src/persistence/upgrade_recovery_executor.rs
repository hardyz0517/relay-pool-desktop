use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::{
    upgrade_journal::{Sha256Digest, UpgradeAttemptId, UpgradeJournal, UpgradePhase},
    upgrade_recovery_plan::{
        BackupState, JournalState, LegacySourceState, ObservedUpgradeState, RecoveryHaltReason,
        RecoveryPlan, RecoveryPlanner,
    },
};

pub(crate) const UPGRADE_JOURNAL_FILE: &str = "persistence-upgrade-journal.json";
const TOMBSTONE_MAGIC: &[u8] = b"RELAY_POOL_DESKTOP_LEGACY_TOMBSTONE\n";
const TOMBSTONE_FORMAT_VERSION: u32 = 1;
const SQLITE_HEADER: &[u8] = b"SQLite format 3\0";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedJournal {
    pub(crate) state: JournalState,
    pub(crate) journal: Option<UpgradeJournal>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RecoveryExecution {
    plan: RecoveryPlan,
    expected: ObservedUpgradeState,
    expected_journal_checksum: Option<Sha256Digest>,
}

impl RecoveryExecution {
    pub(crate) fn prepare(
        observed: ObservedUpgradeState,
        journal: Option<&UpgradeJournal>,
    ) -> Result<Self, UpgradeExecutionError> {
        let plan = RecoveryPlanner::plan(observed.clone());
        let expected_journal_checksum = match observed.journal {
            JournalState::Valid(phase) => {
                let journal = journal.ok_or(UpgradeExecutionError::JournalUnavailable)?;
                journal
                    .validate()
                    .map_err(|_| UpgradeExecutionError::JournalInvalid)?;
                if journal.payload().phase != phase {
                    return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
                }
                Some(journal.canonical_payload_checksum().clone())
            }
            JournalState::Missing | JournalState::Invalid => None,
        };
        Ok(Self {
            plan,
            expected: observed,
            expected_journal_checksum,
        })
    }

    pub(crate) fn plan(&self) -> RecoveryPlan {
        self.plan
    }
}

pub(crate) trait UpgradeRecoveryIo {
    fn observe(&self) -> Result<ObservedUpgradeState, UpgradeExecutionError>;
    fn load_journal(&self) -> Result<UpgradeJournal, UpgradeExecutionError>;
    fn source_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError>;
    fn backup_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError>;
    fn tombstone_attempt_id(&self) -> Result<UpgradeAttemptId, UpgradeExecutionError>;
    fn artifact_paths_are_safe(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<bool, UpgradeExecutionError>;

    fn restart_from_source(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError>;
    fn rebuild_v2_from_verified_backup(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError>;
    fn activate_validated_v2(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError>;
    fn record_legacy_deactivated(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError>;
    fn restore_generation_1(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError>;
    fn commit_generation_2(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError>;
    fn reopen_generation_2(&self, journal: &UpgradeJournal) -> Result<(), UpgradeExecutionError>;
    fn cleanup_completed_journal(
        &self,
        journal: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError>;
}

pub(crate) struct RecoveryExecutor;

impl RecoveryExecutor {
    pub(crate) fn execute(
        io: &impl UpgradeRecoveryIo,
        execution: &RecoveryExecution,
    ) -> Result<RecoveryPlan, UpgradeExecutionError> {
        if let RecoveryPlan::Halt(reason) = execution.plan {
            return Err(UpgradeExecutionError::Halted(reason));
        }

        let current = io.observe()?;
        if current != execution.expected {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        }

        let journal = io.load_journal()?;
        journal
            .validate()
            .map_err(|_| UpgradeExecutionError::JournalInvalid)?;
        if Some(journal.canonical_payload_checksum())
            != execution.expected_journal_checksum.as_ref()
        {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        }
        let JournalState::Valid(expected_phase) = execution.expected.journal else {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        };
        if journal.payload().phase != expected_phase || !io.artifact_paths_are_safe(&journal)? {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        }

        match execution.expected.source {
            LegacySourceState::Generation1 => {
                if &io.source_sha256()? != &journal.payload().source_candidate_identity {
                    return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
                }
            }
            LegacySourceState::ValidTombstone => {
                if &io.tombstone_attempt_id()? != &journal.payload().attempt_id {
                    return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
                }
            }
            LegacySourceState::Missing | LegacySourceState::Unknown => {
                return Err(UpgradeExecutionError::RecoveryPreconditionChanged)
            }
        }

        if execution.expected.backup == BackupState::Verified
            && Some(&io.backup_sha256()?) != journal.payload().verified_backup_sha256.as_ref()
        {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        }

        match execution.plan {
            RecoveryPlan::RestartFromSource => io.restart_from_source(&journal)?,
            RecoveryPlan::RebuildV2FromVerifiedBackup => {
                io.rebuild_v2_from_verified_backup(&journal)?
            }
            RecoveryPlan::ActivateValidatedV2 => io.activate_validated_v2(&journal)?,
            RecoveryPlan::RecordLegacyDeactivated => io.record_legacy_deactivated(&journal)?,
            RecoveryPlan::RestoreGeneration1 => io.restore_generation_1(&journal)?,
            RecoveryPlan::CommitGeneration2 => io.commit_generation_2(&journal)?,
            RecoveryPlan::ReopenGeneration2 => io.reopen_generation_2(&journal)?,
            RecoveryPlan::CleanupCompletedJournal => io.cleanup_completed_journal(&journal)?,
            RecoveryPlan::Halt(_) => unreachable!("halt returned before precondition validation"),
        }
        Ok(execution.plan)
    }
}

#[derive(Debug, Clone, Copy, thiserror::Error, PartialEq, Eq)]
pub(crate) enum UpgradeExecutionError {
    #[error("upgrade recovery halted: {0:?}")]
    Halted(RecoveryHaltReason),
    #[error("upgrade recovery precondition changed")]
    RecoveryPreconditionChanged,
    #[error("upgrade journal is unavailable")]
    JournalUnavailable,
    #[error("upgrade journal is invalid")]
    JournalInvalid,
    #[error("upgrade recovery I/O failed during {0:?}")]
    Io(UpgradeIoOperation),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpgradeIoOperation {
    Read,
    CreateTemporary,
    Write,
    SyncFile,
    Replace,
    SyncParent,
    RemoveSidecar,
    Verify,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TombstonePayload {
    format_version: u32,
    attempt_id: UpgradeAttemptId,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct LegacyTombstone {
    payload: TombstonePayload,
    canonical_payload_checksum: Sha256Digest,
}

pub(crate) fn replace_legacy_with_tombstone(
    legacy_path: &Path,
    attempt_id: &UpgradeAttemptId,
) -> Result<(), UpgradeExecutionError> {
    if !legacy_path.is_file() {
        return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
    }
    let parent = legacy_path
        .parent()
        .ok_or(UpgradeExecutionError::RecoveryPreconditionChanged)?;
    let payload = TombstonePayload {
        format_version: TOMBSTONE_FORMAT_VERSION,
        attempt_id: attempt_id.clone(),
    };
    let payload_bytes = serde_json::to_vec(&payload)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Write))?;
    let tombstone = LegacyTombstone {
        payload,
        canonical_payload_checksum: digest_bytes(&payload_bytes),
    };
    let mut bytes = TOMBSTONE_MAGIC.to_vec();
    bytes.extend(
        serde_json::to_vec(&tombstone)
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Write))?,
    );

    let temporary = unique_sibling(legacy_path, "tombstone");
    write_new_synced(&temporary, &bytes)?;
    if let Err(error) = replace_existing_file(&temporary, legacy_path) {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_parent(parent)?;

    for suffix in ["-wal", "-shm"] {
        let mut sidecar = legacy_path.as_os_str().to_os_string();
        sidecar.push(suffix);
        let sidecar = PathBuf::from(sidecar);
        match fs::remove_file(&sidecar) {
            Ok(()) => sync_parent(parent)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err(UpgradeExecutionError::Io(UpgradeIoOperation::RemoveSidecar)),
        }
    }
    let verified = read_legacy_tombstone(legacy_path)?
        .ok_or(UpgradeExecutionError::Io(UpgradeIoOperation::Verify))?;
    if &verified != attempt_id {
        return Err(UpgradeExecutionError::Io(UpgradeIoOperation::Verify));
    }
    Ok(())
}

pub(crate) fn read_legacy_tombstone(
    path: &Path,
) -> Result<Option<UpgradeAttemptId>, UpgradeExecutionError> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(UpgradeExecutionError::Io(UpgradeIoOperation::Read)),
    };
    let mut magic = vec![0; TOMBSTONE_MAGIC.len()];
    if file.read_exact(&mut magic).is_err() || magic != TOMBSTONE_MAGIC {
        return Ok(None);
    }
    let mut json = Vec::new();
    file.read_to_end(&mut json)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
    let tombstone: LegacyTombstone = serde_json::from_slice(&json)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Verify))?;
    if tombstone.payload.format_version != TOMBSTONE_FORMAT_VERSION {
        return Err(UpgradeExecutionError::Io(UpgradeIoOperation::Verify));
    }
    let payload_bytes = serde_json::to_vec(&tombstone.payload)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Verify))?;
    if digest_bytes(&payload_bytes) != tombstone.canonical_payload_checksum {
        return Err(UpgradeExecutionError::Io(UpgradeIoOperation::Verify));
    }
    UpgradeAttemptId::parse(tombstone.payload.attempt_id.as_str())
        .map(Some)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Verify))
}

pub(crate) fn write_journal_atomically(
    journal_path: &Path,
    journal: &UpgradeJournal,
) -> Result<(), UpgradeExecutionError> {
    let bytes = journal
        .to_canonical_json()
        .map_err(|_| UpgradeExecutionError::JournalInvalid)?;
    write_same_directory_atomically(journal_path, &bytes)
}

pub(crate) fn observe_journal(journal_path: &Path) -> ObservedJournal {
    let bytes = match fs::read(journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ObservedJournal {
                state: JournalState::Missing,
                journal: None,
            }
        }
        Err(_) => {
            return ObservedJournal {
                state: JournalState::Invalid,
                journal: None,
            }
        }
    };
    match UpgradeJournal::from_json(&bytes) {
        Ok(journal) => ObservedJournal {
            state: JournalState::Valid(journal.payload().phase),
            journal: Some(journal),
        },
        Err(_) => ObservedJournal {
            state: JournalState::Invalid,
            journal: None,
        },
    }
}

pub(crate) fn observe_legacy_source(path: &Path) -> LegacySourceState {
    match read_legacy_tombstone(path) {
        Ok(Some(_)) => return LegacySourceState::ValidTombstone,
        Err(_) => return LegacySourceState::Unknown,
        Ok(None) => {}
    }
    let mut header = [0_u8; 16];
    match File::open(path).and_then(|mut file| file.read_exact(&mut header)) {
        Ok(()) if header == SQLITE_HEADER => LegacySourceState::Generation1,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => LegacySourceState::Missing,
        Ok(()) | Err(_) => LegacySourceState::Unknown,
    }
}

pub(crate) fn observe_backup(path: &Path, expected: &Sha256Digest) -> BackupState {
    if !path.exists() {
        return BackupState::Missing;
    }
    match sha256_file(path) {
        Ok(actual) if &actual == expected => BackupState::Verified,
        Ok(_) | Err(_) => BackupState::Invalid,
    }
}

pub(crate) fn journal_artifact_paths_are_safe(root: &Path, journal: &UpgradeJournal) -> bool {
    let paths = &journal.payload().paths;
    [paths.backup(), paths.v2_temporary(), paths.v2_final()]
        .into_iter()
        .all(|relative| resolve_allowlisted_artifact(root, relative).is_ok())
}

pub(crate) fn publish_v2_candidate(
    temporary_path: &Path,
    final_path: &Path,
) -> Result<(), UpgradeExecutionError> {
    if final_path.exists() || temporary_path.parent() != final_path.parent() {
        return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
    }
    fs::rename(temporary_path, final_path)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Replace))?;
    sync_parent(
        final_path
            .parent()
            .ok_or(UpgradeExecutionError::RecoveryPreconditionChanged)?,
    )
}

pub(crate) fn remove_file_and_sync_parent(path: &Path) -> Result<(), UpgradeExecutionError> {
    let parent = path
        .parent()
        .ok_or(UpgradeExecutionError::RecoveryPreconditionChanged)?;
    fs::remove_file(path).map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Replace))?;
    sync_parent(parent)
}

pub(crate) fn sha256_file(path: &Path) -> Result<Sha256Digest, UpgradeExecutionError> {
    let mut file =
        File::open(path).map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Verify))
}

pub(crate) fn resolve_allowlisted_artifact(
    root: &Path,
    relative: &str,
) -> Result<PathBuf, UpgradeExecutionError> {
    let relative_path = Path::new(relative);
    if relative_path.is_absolute()
        || relative_path.components().any(|component| {
            !matches!(component, Component::Normal(_))
                || !component.as_os_str().to_string_lossy().bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'.' | b'-' | b'_')
                })
        })
    {
        return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
    }

    let canonical_root = root
        .canonicalize()
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
    let mut cursor = canonical_root.clone();
    for component in relative_path.components() {
        let Component::Normal(part) = component else {
            return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
        };
        cursor.push(part);
        if cursor.exists() {
            let metadata = fs::symlink_metadata(&cursor)
                .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?;
            if metadata.file_type().is_symlink()
                || !cursor
                    .canonicalize()
                    .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Read))?
                    .starts_with(&canonical_root)
            {
                return Err(UpgradeExecutionError::RecoveryPreconditionChanged);
            }
        }
    }
    Ok(cursor)
}

fn write_same_directory_atomically(
    final_path: &Path,
    bytes: &[u8],
) -> Result<(), UpgradeExecutionError> {
    let parent = final_path
        .parent()
        .ok_or(UpgradeExecutionError::RecoveryPreconditionChanged)?;
    let temporary = unique_sibling(final_path, "write");
    write_new_synced(&temporary, bytes)?;
    let replace_result = if final_path.exists() {
        replace_existing_file(&temporary, final_path)
    } else {
        fs::rename(&temporary, final_path)
            .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Replace))
    };
    if let Err(error) = replace_result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    sync_parent(parent)
}

fn write_new_synced(path: &Path, bytes: &[u8]) -> Result<(), UpgradeExecutionError> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::CreateTemporary))?;
    file.write_all(bytes)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Write))?;
    file.sync_all()
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::SyncFile))
}

fn unique_sibling(path: &Path, operation: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    path.with_extension(format!("{operation}-{}-{unique}.tmp", std::process::id()))
}

#[cfg(windows)]
fn replace_existing_file(
    temporary: &Path,
    destination: &Path,
) -> Result<(), UpgradeExecutionError> {
    use std::{os::windows::ffi::OsStrExt, ptr};
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let temporary: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
    let ok = unsafe {
        ReplaceFileW(
            destination.as_ptr(),
            temporary.as_ptr(),
            ptr::null(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        Err(UpgradeExecutionError::Io(UpgradeIoOperation::Replace))
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_existing_file(
    temporary: &Path,
    destination: &Path,
) -> Result<(), UpgradeExecutionError> {
    fs::rename(temporary, destination)
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::Replace))
}

#[cfg(not(windows))]
fn sync_parent(parent: &Path) -> Result<(), UpgradeExecutionError> {
    File::open(parent)
        .and_then(|file| file.sync_all())
        .map_err(|_| UpgradeExecutionError::Io(UpgradeIoOperation::SyncParent))
}

#[cfg(windows)]
fn sync_parent(_parent: &Path) -> Result<(), UpgradeExecutionError> {
    // Windows has no supported directory fsync. ReplaceFileW provides the durable atomic
    // replacement primitive here; each temporary file is flushed before it is published.
    Ok(())
}

fn digest_bytes(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest::parse(&format!("{:x}", Sha256::digest(bytes)))
        .expect("SHA-256 formatter returns a valid lowercase digest")
}

pub(crate) fn next_phase_for(plan: RecoveryPlan) -> Option<UpgradePhase> {
    match plan {
        RecoveryPlan::RestartFromSource => Some(UpgradePhase::BackupVerified),
        RecoveryPlan::RebuildV2FromVerifiedBackup => Some(UpgradePhase::V2Validated),
        RecoveryPlan::ActivateValidatedV2 | RecoveryPlan::RecordLegacyDeactivated => {
            Some(UpgradePhase::LegacyDeactivated)
        }
        RecoveryPlan::CommitGeneration2 => Some(UpgradePhase::GenerationCommitted),
        RecoveryPlan::ReopenGeneration2 => Some(UpgradePhase::V2Reopened),
        RecoveryPlan::RestoreGeneration1
        | RecoveryPlan::CleanupCompletedJournal
        | RecoveryPlan::Halt(_) => None,
    }
}
