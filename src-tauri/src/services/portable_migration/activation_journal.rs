use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{DateTime, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::services::data_store::{
    atomic_file::{
        sync_parent, ApprovedLeaf, AtomicFileError, AtomicJournalPort, LocalAtomicFileAdapter,
    },
    file_identity::FileIdentity,
};

pub(crate) const ACTIVATION_JOURNAL_FILE: &str = "portable-migration-activation-journal.json";
const JOURNAL_VERSION: u32 = 1;
const MAX_JOURNAL_BYTES: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PortableActivationPhase {
    Prepared,
    ActivationStarted,
    ReplacementCommitted,
    ActivatedValidated,
    RollbackStarted,
    RolledBack,
    ManualRecoveryRequired,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PortableActivationArtifact {
    pub(crate) path: PathBuf,
    pub(crate) volume_serial: Option<u64>,
    pub(crate) file_id: Option<u128>,
    pub(crate) length: u64,
    pub(crate) sha256: String,
}

impl PortableActivationArtifact {
    pub(crate) fn from_identity(path: impl Into<PathBuf>, identity: &FileIdentity) -> Self {
        Self {
            path: path.into(),
            volume_serial: identity.volume_serial,
            file_id: identity.file_id,
            length: identity.length,
            sha256: identity.sha256.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PortableActivationPayload {
    pub(crate) operation_id: String,
    pub(crate) phase: PortableActivationPhase,
    pub(crate) mode: String,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
    pub(crate) target_device_key_id: String,
    pub(crate) active: PortableActivationArtifact,
    pub(crate) staged: PortableActivationArtifact,
    pub(crate) rollback: PortableActivationArtifact,
    pub(crate) backup: PortableActivationArtifact,
    pub(crate) observed_rollback_file_id: Option<u128>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PortableActivationJournal {
    pub(crate) version: u32,
    pub(crate) checksum: String,
    pub(crate) payload: PortableActivationPayload,
}

impl PortableActivationJournal {
    pub(crate) fn prepared(
        operation_id: String,
        mode: String,
        target_device_key_id: String,
        active: PortableActivationArtifact,
        staged: PortableActivationArtifact,
        rollback: PortableActivationArtifact,
        backup: PortableActivationArtifact,
    ) -> Result<Self, PortableActivationJournalError> {
        validate_uuid_v7(&operation_id)?;
        validate_mode(&mode)?;
        validate_key_id(&target_device_key_id)?;
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let payload = PortableActivationPayload {
            operation_id,
            phase: PortableActivationPhase::Prepared,
            mode,
            created_at: now.clone(),
            updated_at: now,
            target_device_key_id,
            active,
            staged,
            rollback,
            backup,
            observed_rollback_file_id: None,
        };
        Self::from_payload(payload)
    }

    pub(crate) fn advance(
        &self,
        phase: PortableActivationPhase,
        observed_rollback_file_id: Option<u128>,
    ) -> Result<Self, PortableActivationJournalError> {
        let now = Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true);
        let mut payload = self.payload.clone();
        payload.phase = phase;
        payload.updated_at = now;
        payload.observed_rollback_file_id = observed_rollback_file_id;
        Self::from_payload(payload)
    }

    pub(crate) fn from_payload(
        payload: PortableActivationPayload,
    ) -> Result<Self, PortableActivationJournalError> {
        validate_payload(&payload)?;
        let checksum = checksum_payload(&payload)?;
        Ok(Self {
            version: JOURNAL_VERSION,
            checksum,
            payload,
        })
    }

    pub(crate) fn validate(&self) -> Result<(), PortableActivationJournalError> {
        if self.version != JOURNAL_VERSION {
            return Err(PortableActivationJournalError::UnsupportedVersion);
        }
        validate_payload(&self.payload)?;
        let expected = checksum_payload(&self.payload)?;
        if self.checksum != expected {
            return Err(PortableActivationJournalError::ChecksumMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortableActivationJournalError {
    #[error("portable activation journal I/O failed")]
    Io(#[from] std::io::Error),
    #[error("portable activation journal atomic publish failed")]
    Atomic(#[from] AtomicFileError),
    #[error("portable activation journal JSON failed")]
    Json(#[from] serde_json::Error),
    #[error("portable activation journal is too large")]
    TooLarge,
    #[error("portable activation journal checksum mismatch")]
    ChecksumMismatch,
    #[error("portable activation journal version is unsupported")]
    UnsupportedVersion,
    #[error("portable activation journal phase shape is invalid")]
    InvalidPhaseShape,
    #[error("portable activation journal value is invalid")]
    InvalidValue,
}

pub(crate) fn journal_path(config_dir: &Path) -> PathBuf {
    config_dir.join(ACTIVATION_JOURNAL_FILE)
}

pub(crate) fn write_prepared_journal(
    config_dir: &Path,
    journal: &PortableActivationJournal,
) -> Result<(), PortableActivationJournalError> {
    write_journal(config_dir, journal)
}

pub(crate) fn write_journal(
    config_dir: &Path,
    journal: &PortableActivationJournal,
) -> Result<(), PortableActivationJournalError> {
    fs::create_dir_all(config_dir)?;
    journal.validate()?;
    let bytes = serde_json::to_vec_pretty(journal)?;
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(PortableActivationJournalError::TooLarge);
    }
    let target = ApprovedLeaf::approve(config_dir, ACTIVATION_JOURNAL_FILE)?;
    let readback = LocalAtomicFileAdapter.publish_and_readback(&bytes, &target)?;
    let decoded = decode_journal(&readback)?;
    if decoded != *journal {
        return Err(PortableActivationJournalError::ChecksumMismatch);
    }
    Ok(())
}

pub(crate) fn read_journal(
    config_dir: &Path,
) -> Result<Option<PortableActivationJournal>, PortableActivationJournalError> {
    match fs::read(journal_path(config_dir)) {
        Ok(bytes) => decode_journal(&bytes).map(Some),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn remove_journal(config_dir: &Path) -> Result<(), PortableActivationJournalError> {
    let path = journal_path(config_dir);
    match fs::remove_file(&path) {
        Ok(()) => sync_parent(config_dir).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn decode_journal(
    bytes: &[u8],
) -> Result<PortableActivationJournal, PortableActivationJournalError> {
    if bytes.len() > MAX_JOURNAL_BYTES {
        return Err(PortableActivationJournalError::TooLarge);
    }
    let journal: PortableActivationJournal = serde_json::from_slice(bytes)?;
    journal.validate()?;
    Ok(journal)
}

fn checksum_payload(
    payload: &PortableActivationPayload,
) -> Result<String, PortableActivationJournalError> {
    let canonical = serde_json::to_vec(payload)?;
    Ok(format!("{:x}", Sha256::digest(canonical)))
}

fn validate_payload(
    payload: &PortableActivationPayload,
) -> Result<(), PortableActivationJournalError> {
    validate_uuid_v7(&payload.operation_id)?;
    validate_mode(&payload.mode)?;
    validate_key_id(&payload.target_device_key_id)?;
    validate_timestamp(&payload.created_at)?;
    validate_timestamp(&payload.updated_at)?;
    validate_artifact(&payload.active)?;
    validate_artifact(&payload.staged)?;
    validate_artifact(&payload.rollback)?;
    validate_artifact(&payload.backup)?;
    match payload.phase {
        PortableActivationPhase::Prepared | PortableActivationPhase::ActivationStarted => {
            if payload.observed_rollback_file_id.is_some() {
                return Err(PortableActivationJournalError::InvalidPhaseShape);
            }
        }
        PortableActivationPhase::ReplacementCommitted
        | PortableActivationPhase::ActivatedValidated
        | PortableActivationPhase::RollbackStarted
        | PortableActivationPhase::RolledBack
        | PortableActivationPhase::Completed => {
            if payload.observed_rollback_file_id.is_none() {
                return Err(PortableActivationJournalError::InvalidPhaseShape);
            }
        }
        PortableActivationPhase::ManualRecoveryRequired => {}
    }
    Ok(())
}

fn validate_artifact(
    artifact: &PortableActivationArtifact,
) -> Result<(), PortableActivationJournalError> {
    if !artifact.path.is_absolute() || artifact.length == 0 || !is_sha256_hex(&artifact.sha256) {
        return Err(PortableActivationJournalError::InvalidValue);
    }
    Ok(())
}

fn validate_uuid_v7(value: &str) -> Result<(), PortableActivationJournalError> {
    let id =
        uuid::Uuid::parse_str(value).map_err(|_| PortableActivationJournalError::InvalidValue)?;
    if id.get_version_num() != 7 {
        return Err(PortableActivationJournalError::InvalidValue);
    }
    Ok(())
}

fn validate_timestamp(value: &str) -> Result<(), PortableActivationJournalError> {
    DateTime::parse_from_rfc3339(value)
        .map(|_| ())
        .map_err(|_| PortableActivationJournalError::InvalidValue)
}

fn validate_mode(value: &str) -> Result<(), PortableActivationJournalError> {
    match value {
        "restoreIntoEmpty" | "replaceCurrent" => Ok(()),
        _ => Err(PortableActivationJournalError::InvalidValue),
    }
}

fn validate_key_id(value: &str) -> Result<(), PortableActivationJournalError> {
    if value.is_empty()
        || value.len() > 128
        || value.contains("password")
        || value.contains("ciphertext")
        || value.contains("secret")
    {
        return Err(PortableActivationJournalError::InvalidValue);
    }
    Ok(())
}

pub(crate) fn rollback_path_for_active(
    active_path: &Path,
    operation_id: &str,
) -> Result<PathBuf, PortableActivationJournalError> {
    validate_uuid_v7(operation_id)?;
    let parent = active_path
        .parent()
        .ok_or(PortableActivationJournalError::InvalidValue)?;
    let stem = active_path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or(PortableActivationJournalError::InvalidValue)?;
    Ok(parent.join(format!("{stem}.portable-rollback-{operation_id}.sqlite3")))
}

fn is_sha256_hex(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[cfg(test)]
mod tests {
    use sha2::{Digest, Sha256};

    use super::*;

    #[test]
    fn activation_prepare_journal_round_trips_without_secret_material() {
        let root = tempfile::tempdir().expect("tempdir");
        let journal = sample_journal(root.path());

        write_prepared_journal(root.path(), &journal).expect("write journal");
        let raw = std::fs::read_to_string(journal_path(root.path())).expect("raw journal");
        let loaded = read_journal(root.path())
            .expect("read journal")
            .expect("journal");

        assert_eq!(loaded, journal);
        assert_eq!(loaded.payload.phase, PortableActivationPhase::Prepared);
        assert!(!raw.contains("password"));
        assert!(!raw.contains("ciphertext"));
        assert!(!raw.contains("transport"));
        assert!(!raw.contains("sk-"));
    }

    #[test]
    fn activation_prepare_journal_rejects_unknown_duplicate_and_too_large_payloads() {
        let root = tempfile::tempdir().expect("tempdir");
        let journal = sample_journal(root.path());
        let mut value = serde_json::to_value(&journal).expect("json");
        value["unexpected"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&value).expect("bytes");
        assert!(matches!(
            super::decode_journal(&bytes),
            Err(PortableActivationJournalError::Json(_))
        ));

        let duplicate = br#"{"version":1,"version":1,"checksum":"x","payload":{}}"#;
        assert!(matches!(
            super::decode_journal(duplicate),
            Err(PortableActivationJournalError::Json(_))
        ));

        let too_large = vec![b' '; MAX_JOURNAL_BYTES + 1];
        assert!(matches!(
            super::decode_journal(&too_large),
            Err(PortableActivationJournalError::TooLarge)
        ));
    }

    #[test]
    fn activation_prepare_journal_validates_checksum_and_phase_shape() {
        let root = tempfile::tempdir().expect("tempdir");
        let mut journal = sample_journal(root.path());
        journal.checksum = "0".repeat(64);
        assert!(matches!(
            journal.validate(),
            Err(PortableActivationJournalError::ChecksumMismatch)
        ));

        let mut invalid = sample_journal(root.path()).payload;
        invalid.observed_rollback_file_id = Some(42);
        let journal = PortableActivationJournal {
            version: JOURNAL_VERSION,
            checksum: checksum_payload(&invalid).expect("checksum"),
            payload: invalid,
        };
        assert!(matches!(
            journal.validate(),
            Err(PortableActivationJournalError::InvalidPhaseShape)
        ));
    }

    fn sample_journal(root: &Path) -> PortableActivationJournal {
        let active = artifact(root.join("relay-pool-desktop-v2.sqlite3"), b"active");
        let staged = artifact(root.join("staged.sqlite3"), b"staged");
        let rollback = artifact(root.join("rollback.sqlite3"), b"active");
        let backup = artifact(root.join("backup.sqlite3"), b"backup");
        PortableActivationJournal::prepared(
            uuid::Uuid::now_v7().to_string(),
            "replaceCurrent".to_string(),
            "target-device-key".to_string(),
            active,
            staged,
            rollback,
            backup,
        )
        .expect("journal")
    }

    fn artifact(path: PathBuf, bytes: &[u8]) -> PortableActivationArtifact {
        PortableActivationArtifact {
            path,
            volume_serial: Some(1),
            file_id: Some(2),
            length: bytes.len() as u64,
            sha256: format!("{:x}", Sha256::digest(bytes)),
        }
    }
}
