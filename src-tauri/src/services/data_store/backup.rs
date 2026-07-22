use std::{
    fs,
    path::{Path, PathBuf},
    time::SystemTime,
};

use crate::persistence::create_verified_backup_from_path;

use super::{
    inspect::inspect_candidate,
    types::{CandidateHealth, CandidateRole},
};

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
    let source_file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "source database path has no valid file name".to_string())?;
    if source_file_name != "relay-pool-desktop.sqlite3"
        && source_file_name != "relay-pool-desktop-v2.sqlite3"
    {
        return Err("source database file name is not owned by Relay Pool".to_string());
    }

    let backups_root = default_app_data.join("backups");
    fs::create_dir_all(&backups_root).map_err(|error| {
        format!(
            "failed to create backup root {}: {error}",
            backups_root.display()
        )
    })?;
    let backup_dir = create_unique_backup_dir(&backups_root)?;
    let final_backup_path = backup_dir.join(source_file_name);

    #[cfg(test)]
    if matches!(failure, Some(BackupFailure::DestinationOpen)) {
        return Err("injected destination-open failure".to_string());
    }

    write_sqlite_backup(source_path, &final_backup_path)?;
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
    tauri::async_runtime::block_on(create_verified_backup_from_path(
        source_path,
        temp_backup_path,
    ))
    .map(|_| ())
    .map_err(|error| format!("failed to create verified sqlite backup: {error}"))
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
    use crate::services::data_store::{
        config::{write_config_v3, DataDirConfigV3, DatabaseGeneration},
        inspect::inspect_candidate,
        inspect_startup,
        types::{CandidateHealth, CandidateRole},
    };
    use sqlx::SqliteConnection;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::SystemTime,
    };

    #[test]
    fn wal_mode_backup_contains_committed_rows_and_passes_quick_check() {
        let root = temp_root("wal-backup");
        let source = root.join("relay-pool-desktop.sqlite3");
        let backup_root = root.join("app-data");
        let mut connection = open(&source);
        execute_batch(
            &mut connection,
            "CREATE TABLE stations(name TEXT); INSERT INTO stations VALUES ('committed')",
        );
        assert!(source.with_extension("sqlite3-wal").exists());

        let backup = backup_selected_database(&source, &backup_root).expect("backup");
        let inspected =
            inspect_candidate(&backup.backup_path, CandidateRole::Backup).expect("inspect backup");

        assert_eq!(inspected.candidate.health, CandidateHealth::Healthy);
        assert_eq!(inspected.candidate.counts.get("stations"), Some(&1));
        assert!(backup.backup_path.starts_with(backup_root.join("backups")));
        crate::services::data_store::test_support::close_database(connection);
    }

    #[test]
    fn injected_backup_failures_leave_source_unchanged() {
        let root = temp_root("backup-failures");
        let source = root.join("relay-pool-desktop.sqlite3");
        let backup_root = root.join("app-data");
        let mut connection = open(&source);
        execute_batch(
            &mut connection,
            "CREATE TABLE stations(name TEXT); INSERT INTO stations VALUES ('original')",
        );
        crate::services::data_store::test_support::close_database(connection);
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

    #[test]
    fn generation_two_backup_keeps_its_discoverable_file_name() {
        let root = temp_root("v2-backup-name");
        let source = root.join("relay-pool-desktop-v2.sqlite3");
        let backup_root = root.join("app-data");
        let mut connection = open(&source);
        execute_batch(
            &mut connection,
            "CREATE TABLE stations(name TEXT); INSERT INTO stations VALUES ('v2')",
        );
        crate::services::data_store::test_support::close_database(connection);

        let backup = backup_selected_database(&source, &backup_root).expect("v2 backup");

        assert_eq!(
            backup
                .backup_path
                .file_name()
                .and_then(|name| name.to_str()),
            Some("relay-pool-desktop-v2.sqlite3")
        );
        assert!(backup.backup_path.is_file());
        write_config_v3(
            &backup_root.join("relay-pool-data-dir.json"),
            &DataDirConfigV3 {
                version: 3,
                active_data_dir: Some(root.clone()),
                pending_data_dir: None,
                source_data_dir: None,
                database_generation: DatabaseGeneration::Two,
                updated_at: "2026-07-21T00:00:00Z".to_string(),
            },
        )
        .expect("generation-two config");
        let startup = inspect_startup(&backup_root).expect("discover backup");
        assert!(startup.candidates.iter().any(|candidate| {
            candidate.role == CandidateRole::Backup
                && candidate.path == backup.backup_path.display().to_string()
        }));
    }

    fn source_facts(path: &Path) -> (u64, i64) {
        let size = fs::metadata(path).expect("metadata").len();
        let count = crate::services::data_store::test_support::query_i64(
            path,
            "SELECT COUNT(*) FROM stations",
        );
        (size, count)
    }

    fn open(path: &Path) -> SqliteConnection {
        crate::services::data_store::test_support::open_database(path)
    }

    fn execute_batch(connection: &mut SqliteConnection, statements: &str) {
        crate::services::data_store::test_support::execute_batch(connection, statements);
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
