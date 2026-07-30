use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::{SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::services::data_store::atomic_file::{
    sync_parent, ApprovedLeaf, AtomicFileError, AtomicJournalPort, LocalAtomicFileAdapter,
};

const JOURNAL_FILE: &str = "device-key-bootstrap-journal.json";

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DeviceKeyJournalPhase {
    Planned,
    KeyCreated,
    DatabaseValidated,
    ActiveCommitted,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct DeviceKeyJournal {
    pub(crate) version: u32,
    pub(crate) phase: DeviceKeyJournalPhase,
    pub(crate) key_id: String,
    pub(crate) candidate_identity: String,
    pub(crate) updated_at: String,
}

impl DeviceKeyJournal {
    pub(crate) fn new(
        phase: DeviceKeyJournalPhase,
        key_id: String,
        candidate_identity: String,
    ) -> Self {
        Self {
            version: 1,
            phase,
            key_id,
            candidate_identity,
            updated_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
        }
    }

    pub(crate) fn advance(&self, phase: DeviceKeyJournalPhase) -> Self {
        Self::new(phase, self.key_id.clone(), self.candidate_identity.clone())
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum DeviceKeyJournalError {
    #[error("device key journal I/O failed")]
    Io(#[from] std::io::Error),
    #[error("device key journal atomic publish failed")]
    Atomic(#[from] AtomicFileError),
    #[error("device key journal JSON failed")]
    Json(#[from] serde_json::Error),
}

pub(crate) fn journal_path(config_dir: &Path) -> PathBuf {
    config_dir.join(JOURNAL_FILE)
}

pub(crate) fn write_journal(
    config_dir: &Path,
    journal: &DeviceKeyJournal,
) -> Result<(), DeviceKeyJournalError> {
    fs::create_dir_all(config_dir)?;
    let bytes = serde_json::to_vec_pretty(journal)?;
    let target = ApprovedLeaf::approve(config_dir, JOURNAL_FILE)?;
    let readback = LocalAtomicFileAdapter.publish_and_readback(&bytes, &target)?;
    let decoded: DeviceKeyJournal = serde_json::from_slice(&readback)?;
    if decoded != *journal {
        return Err(DeviceKeyJournalError::Json(serde_json::Error::io(
            std::io::Error::new(std::io::ErrorKind::InvalidData, "journal readback mismatch"),
        )));
    }
    Ok(())
}

pub(crate) fn read_journal(
    config_dir: &Path,
) -> Result<Option<DeviceKeyJournal>, DeviceKeyJournalError> {
    match fs::read(journal_path(config_dir)) {
        Ok(bytes) => serde_json::from_slice(&bytes).map(Some).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

pub(crate) fn remove_journal(config_dir: &Path) -> Result<(), DeviceKeyJournalError> {
    let path = journal_path(config_dir);
    match fs::remove_file(&path) {
        Ok(()) => sync_parent(config_dir).map_err(Into::into),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        journal_path, read_journal, remove_journal, write_journal, DeviceKeyJournal,
        DeviceKeyJournalPhase,
    };

    #[test]
    fn journal_write_round_trips_exact_phase_without_key_material() {
        let root = tempfile::tempdir().expect("tempdir");
        let journal = DeviceKeyJournal::new(
            DeviceKeyJournalPhase::Planned,
            "019fad3d-631e-76d0-8659-ac335efde02d".to_string(),
            "first-run:C:/RelayPool".to_string(),
        );

        write_journal(root.path(), &journal).expect("write journal");
        let raw = std::fs::read_to_string(journal_path(root.path())).expect("raw journal");
        let loaded = read_journal(root.path())
            .expect("read journal")
            .expect("journal");

        assert_eq!(loaded.phase, DeviceKeyJournalPhase::Planned);
        assert_eq!(loaded.key_id, journal.key_id);
        assert!(!raw.contains("local-data-key"));
        assert!(!raw.contains("ciphertext"));
        assert!(!raw.contains("password"));
    }

    #[test]
    fn journal_cleanup_removes_file_idempotently() {
        let root = tempfile::tempdir().expect("tempdir");
        let journal = DeviceKeyJournal::new(
            DeviceKeyJournalPhase::ActiveCommitted,
            "019fad3d-631e-76d0-8659-ac335efde02d".to_string(),
            "first-run:C:/RelayPool".to_string(),
        );
        write_journal(root.path(), &journal).expect("write journal");

        remove_journal(root.path()).expect("remove journal");
        remove_journal(root.path()).expect("remove again");

        assert!(!journal_path(root.path()).exists());
    }
}
