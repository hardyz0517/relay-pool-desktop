//! Durable helpers for the one remaining pre-secret-baseline transition.
//!
//! This is deliberately scoped to the schema-15 -> encrypted-secret baseline
//! transition. Generation selection and recovery are no longer part of this
//! module or of normal startup.

use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

use sha2::{Digest, Sha256};

use super::upgrade_journal::{BaselineConversionJournal, Sha256Digest};
use crate::services::data_store::atomic_file::{
    create_new_file, replace_existing_file, sync_file, sync_parent, unique_sibling, AtomicFileError,
};

pub(crate) const BASELINE_CONVERSION_JOURNAL_FILE: &str = "persistence-upgrade-journal.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PersistenceJournalKind {
    Missing,
    BaselineConversion,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedPersistenceJournal {
    pub(crate) kind: PersistenceJournalKind,
    pub(crate) baseline: Option<BaselineConversionJournal>,
}

pub(crate) fn observe_persistence_journal(journal_path: &Path) -> ObservedPersistenceJournal {
    let bytes = match fs::read(journal_path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return ObservedPersistenceJournal {
                kind: PersistenceJournalKind::Missing,
                baseline: None,
            }
        }
        Err(_) => {
            return ObservedPersistenceJournal {
                kind: PersistenceJournalKind::Invalid,
                baseline: None,
            }
        }
    };
    match BaselineConversionJournal::from_json(&bytes) {
        Ok(journal) => ObservedPersistenceJournal {
            kind: PersistenceJournalKind::BaselineConversion,
            baseline: Some(journal),
        },
        Err(_) => ObservedPersistenceJournal {
            kind: PersistenceJournalKind::Invalid,
            baseline: None,
        },
    }
}

pub(crate) fn write_baseline_conversion_journal_atomically(
    journal_path: &Path,
    journal: &BaselineConversionJournal,
) -> Result<(), String> {
    let bytes = journal
        .to_canonical_json()
        .map_err(|error| format!("baseline conversion journal is invalid: {error}"))?;
    let parent = journal_path
        .parent()
        .ok_or_else(|| "baseline conversion journal has no parent directory".to_string())?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("failed to create baseline journal directory: {error}"))?;
    let temporary = unique_sibling(journal_path, "journal");
    let result = (|| {
        let mut file = create_new_file(&temporary)
            .map_err(|error| format!("failed to create baseline journal staging file: {error}"))?;
        file.write_all(&bytes)
            .map_err(|error| format!("failed to write baseline journal staging file: {error}"))?;
        drop(file);
        sync_file(&temporary)
            .map_err(|error| format!("failed to sync baseline journal staging file: {error}"))?;
        let publish = if journal_path.exists() {
            replace_existing_file(&temporary, journal_path)
        } else {
            fs::rename(&temporary, journal_path).map_err(AtomicFileError::Io)
        };
        publish.map_err(|error| format!("failed to publish baseline journal: {error}"))?;
        sync_parent(parent)
            .map_err(|error| format!("failed to sync baseline journal directory: {error}"))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

pub(crate) fn remove_file_and_sync_parent(path: &Path) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "baseline journal has no parent directory".to_string())?;
    match fs::remove_file(path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(format!("failed to remove baseline journal: {error}")),
    }
    sync_parent(parent)
        .map_err(|error| format!("failed to sync baseline journal directory: {error}"))
}

pub(crate) fn sha256_file(path: &Path) -> Result<Sha256Digest, String> {
    let mut file = fs::File::open(path)
        .map_err(|error| format!("failed to read baseline artifact: {error}"))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("failed to read baseline artifact: {error}"))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Sha256Digest::parse(&format!("{:x}", hasher.finalize()))
        .map_err(|error| format!("failed to calculate baseline artifact digest: {error}"))
}
