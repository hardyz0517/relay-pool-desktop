use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use rusqlite::{backup::Backup, Connection, OpenFlags};

use super::{
    inspect::inspect_candidate,
    types::{CandidateHealth, CandidateRole},
};

const BACKUP_FILE_NAME: &str = "relay-pool-desktop.sqlite3";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BackupResult {
    pub backup_path: PathBuf,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupFailure {
    InsufficientSpacePreflight,
    DestinationOpen,
}

pub(crate) fn backup_selected_database(
    source_path: &Path,
    default_app_data: &Path,
) -> Result<BackupResult, String> {
    #[cfg(not(test))]
    return backup_selected_database_inner(source_path, default_app_data);
    #[cfg(test)]
    return backup_selected_database_inner(source_path, default_app_data, None);
}

#[cfg(test)]
pub(crate) fn backup_selected_database_with_failure_for_test(
    source_path: &Path,
    default_app_data: &Path,
    failure: BackupFailure,
) -> Result<BackupResult, String> {
    backup_selected_database_inner(source_path, default_app_data, Some(failure))
}

fn backup_selected_database_inner(
    source_path: &Path,
    default_app_data: &Path,
    #[cfg(test)] failure: Option<BackupFailure>,
) -> Result<BackupResult, String> {
    if !source_path.is_file() {
        return Err(format!(
            "source database does not exist: {}",
            source_path.display()
        ));
    }
    #[cfg(test)]
    if matches!(failure, Some(BackupFailure::InsufficientSpacePreflight)) {
        return Err("injected insufficient-space preflight failure".to_string());
    }

    let backups_root = default_app_data.join("backups");
    fs::create_dir_all(&backups_root).map_err(|error| {
        format!(
            "failed to create backup root {}: {error}",
            backups_root.display()
        )
    })?;
    let backup_dir = create_unique_backup_dir(&backups_root)?;
    let temp_backup_path = backup_dir.join(format!("{BACKUP_FILE_NAME}.tmp"));
    let final_backup_path = backup_dir.join(BACKUP_FILE_NAME);

    #[cfg(test)]
    if matches!(failure, Some(BackupFailure::DestinationOpen)) {
        return Err("injected destination-open failure".to_string());
    }

    write_sqlite_backup(source_path, &temp_backup_path)?;
    verify_backup(&temp_backup_path)?;
    fs::rename(&temp_backup_path, &final_backup_path).map_err(|error| {
        format!(
            "failed to publish backup {}: {error}",
            final_backup_path.display()
        )
    })?;
    verify_backup(&final_backup_path)?;

    Ok(BackupResult {
        backup_path: final_backup_path,
    })
}

fn create_unique_backup_dir(backups_root: &Path) -> Result<PathBuf, String> {
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_err(|error| format!("system clock is before UNIX_EPOCH: {error}"))?
        .as_millis();
    for attempt in 0..1000u16 {
        let backup_dir = backups_root.join(format!("{timestamp}-{attempt:03}"));
        match fs::create_dir(&backup_dir) {
            Ok(()) => return Ok(backup_dir),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => {
                return Err(format!(
                    "failed to create backup directory {}: {error}",
                    backup_dir.display()
                ))
            }
        }
    }
    Err("failed to allocate a unique backup directory".to_string())
}

fn write_sqlite_backup(source_path: &Path, temp_backup_path: &Path) -> Result<(), String> {
    let source = Connection::open_with_flags(source_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open source database for backup: {error}"))?;
    let mut destination = Connection::open(temp_backup_path)
        .map_err(|error| format!("failed to open backup destination: {error}"))?;
    let backup = Backup::new(&source, &mut destination)
        .map_err(|error| format!("failed to start sqlite backup: {error}"))?;
    backup
        .run_to_completion(5, Duration::from_millis(250), None)
        .map_err(|error| format!("failed to copy sqlite backup: {error}"))?;
    Ok(())
}

fn verify_backup(path: &Path) -> Result<(), String> {
    let inspected = inspect_candidate(path, CandidateRole::Backup)?;
    if inspected.candidate.health != CandidateHealth::Healthy {
        return Err(format!(
            "backup failed validation with health {:?}",
            inspected.candidate.health
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        backup_selected_database, backup_selected_database_with_failure_for_test, BackupFailure,
    };
    use crate::services::data_store::inspect::inspect_candidate;
    use crate::services::data_store::types::{CandidateHealth, CandidateRole};
    use rusqlite::Connection;
    use std::{fs, path::PathBuf, time::SystemTime};

    #[test]
    fn wal_mode_backup_contains_committed_rows_and_passes_quick_check() {
        let root = temp_root("wal-backup");
        let source = root.join("source.sqlite3");
        let backup_root = root.join("app-data");
        let connection = Connection::open(&source).expect("source");
        connection
            .pragma_update(None, "journal_mode", "WAL")
            .expect("wal");
        connection
            .execute("CREATE TABLE stations(name TEXT)", [])
            .expect("table");
        connection
            .execute("INSERT INTO stations VALUES ('committed')", [])
            .expect("row");
        assert!(source.with_extension("sqlite3-wal").exists());
        drop(connection);

        let backup = backup_selected_database(&source, &backup_root).expect("backup");
        let inspected =
            inspect_candidate(&backup.backup_path, CandidateRole::Backup).expect("inspect backup");

        assert_eq!(inspected.candidate.health, CandidateHealth::Healthy);
        assert_eq!(inspected.candidate.counts.get("stations"), Some(&1));
        assert!(backup.backup_path.starts_with(backup_root.join("backups")));
    }

    #[test]
    fn injected_backup_failures_leave_source_unchanged() {
        let root = temp_root("backup-failures");
        let source = root.join("source.sqlite3");
        let backup_root = root.join("app-data");
        let connection = Connection::open(&source).expect("source");
        connection
            .execute("CREATE TABLE stations(name TEXT)", [])
            .expect("table");
        connection
            .execute("INSERT INTO stations VALUES ('original')", [])
            .expect("row");
        drop(connection);
        let before = source_facts(&source);

        for failure in [
            BackupFailure::InsufficientSpacePreflight,
            BackupFailure::DestinationOpen,
        ] {
            let error =
                backup_selected_database_with_failure_for_test(&source, &backup_root, failure)
                    .expect_err("injected failure");
            assert!(error.contains("injected"));
            assert_eq!(source_facts(&source), before);
        }
    }

    fn source_facts(path: &PathBuf) -> (u64, i64) {
        let size = fs::metadata(path).expect("metadata").len();
        let count = Connection::open(path)
            .expect("source")
            .query_row("SELECT COUNT(*) FROM stations", [], |row| row.get(0))
            .expect("count");
        (size, count)
    }

    fn temp_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("relay-pool-backup-{name}-{unique}"));
        fs::create_dir_all(&root).expect("root");
        root
    }
}
