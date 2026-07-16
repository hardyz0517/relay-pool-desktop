use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use rusqlite::{backup::Backup, Connection, OpenFlags};

use super::{
    config::{create_installation_marker, write_config, DataDirConfigV2},
    inspect::inspect_candidate,
    types::{CandidateHealth, CandidateRole, DataStoreRelocationIntent},
};

const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";

pub(crate) fn write_relocation_intent(
    default_data_dir: &Path,
    active_data_dir: &Path,
    pending_data_dir: &Path,
) -> Result<(), String> {
    write_config(
        &default_data_dir.join(DATA_DIR_CONFIG_FILE),
        &DataDirConfigV2 {
            version: 2,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: Some(pending_data_dir.to_path_buf()),
            source_data_dir: Some(active_data_dir.to_path_buf()),
            updated_at: updated_at(),
        },
    )
}

pub(crate) fn write_active_data_dir_selection(
    default_data_dir: &Path,
    active_data_dir: &Path,
) -> Result<(), String> {
    write_config(
        &default_data_dir.join(DATA_DIR_CONFIG_FILE),
        &DataDirConfigV2 {
            version: 2,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: None,
            source_data_dir: None,
            updated_at: updated_at(),
        },
    )
}

pub(crate) fn apply_trusted_relocation(
    default_data_dir: &Path,
    intent: &DataStoreRelocationIntent,
) -> Result<PathBuf, String> {
    let source_db_path = intent.source_data_dir.join(DATABASE_FILE);
    let target_db_path = intent.target_data_dir.join(DATABASE_FILE);
    if !source_db_path.is_file() {
        return Err(format!(
            "source database does not exist: {}",
            source_db_path.display()
        ));
    }
    if target_db_path.exists() {
        return Err(format!(
            "target database already exists: {}",
            target_db_path.display()
        ));
    }

    fs::create_dir_all(&intent.target_data_dir).map_err(|error| {
        format!(
            "failed to create relocation target {}: {error}",
            intent.target_data_dir.display()
        )
    })?;
    let temp_db_path = intent.target_data_dir.join(format!(
        "{DATABASE_FILE}.relocating-{}-{}",
        std::process::id(),
        unique_suffix()
    ));

    if let Err(error) = copy_sqlite_database(&source_db_path, &temp_db_path)
        .and_then(|()| verify_relocated_database(&temp_db_path))
        .and_then(|()| publish_relocated_database(&temp_db_path, &target_db_path))
        .and_then(|()| verify_relocated_database(&target_db_path))
    {
        let _ = fs::remove_file(&temp_db_path);
        return Err(error);
    }

    write_active_data_dir_selection(default_data_dir, &intent.target_data_dir)?;
    create_installation_marker(default_data_dir)?;
    Ok(intent.target_data_dir.clone())
}

fn copy_sqlite_database(source_db_path: &Path, temp_db_path: &Path) -> Result<(), String> {
    let source = Connection::open_with_flags(source_db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open relocation source database: {error}"))?;
    let mut target = Connection::open(temp_db_path)
        .map_err(|error| format!("failed to open relocation target database: {error}"))?;
    let backup = Backup::new(&source, &mut target)
        .map_err(|error| format!("failed to start relocation backup: {error}"))?;
    backup
        .run_to_completion(5, Duration::from_millis(250), None)
        .map_err(|error| format!("failed to copy relocation database: {error}"))?;
    Ok(())
}

fn verify_relocated_database(path: &Path) -> Result<(), String> {
    let inspected = inspect_candidate(path, CandidateRole::Pending)?;
    if inspected.candidate.health != CandidateHealth::Healthy
        || !inspected.contains_relay_pool_schema
        || !inspected.candidate.schema_compatible
    {
        return Err(format!(
            "relocated database failed validation with health {:?}",
            inspected.candidate.health
        ));
    }
    Ok(())
}

fn publish_relocated_database(temp_db_path: &Path, target_db_path: &Path) -> Result<(), String> {
    if target_db_path.exists() {
        return Err(format!(
            "target database already exists: {}",
            target_db_path.display()
        ));
    }
    fs::rename(temp_db_path, target_db_path).map_err(|error| {
        format!(
            "failed to publish relocated database {}: {error}",
            target_db_path.display()
        )
    })
}

fn updated_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

fn unique_suffix() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use crate::services::data_store::config::{read_config, DataDirConfigV2};
    use crate::services::data_store::relocation::{
        apply_trusted_relocation, write_relocation_intent,
    };
    use crate::services::data_store::types::DataStoreRelocationIntent;
    use rusqlite::Connection;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::SystemTime,
    };

    const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
    const CONFIG_FILE: &str = "relay-pool-data-dir.json";

    #[test]
    fn relocation_copies_wal_source_consistently_to_missing_target_and_commits_active() {
        let root = temp_root("wal-relocation");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        create_station_db(&source, "source-station");
        enable_wal_and_insert(&source.join(DATABASE_FILE), "wal-station");
        write_relocation_intent(&default_data_dir, &source, &target).expect("intent");

        let relocated = apply_trusted_relocation(
            &default_data_dir,
            &DataStoreRelocationIntent {
                source_data_dir: source.clone(),
                target_data_dir: target.clone(),
            },
        )
        .expect("relocate");

        assert_eq!(relocated, target);
        assert_eq!(station_count(&target.join(DATABASE_FILE)), 2);
        assert_eq!(station_count(&source.join(DATABASE_FILE)), 2);
        let config = read_config(&default_data_dir.join(CONFIG_FILE))
            .expect("read config")
            .expect("config");
        assert_eq!(config.active_data_dir, Some(target));
        assert_eq!(config.pending_data_dir, None);
        assert_eq!(config.source_data_dir, None);
    }

    #[test]
    fn relocation_rejects_populated_or_protected_targets_without_changing_source_or_config() {
        let root = temp_root("target-conflict");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        create_station_db(&source, "source-station");
        create_schema_only_db(&target);
        write_relocation_intent(&default_data_dir, &source, &target).expect("intent");
        let before_config =
            fs::read_to_string(default_data_dir.join(CONFIG_FILE)).expect("config bytes");

        let error = apply_trusted_relocation(
            &default_data_dir,
            &DataStoreRelocationIntent {
                source_data_dir: source.clone(),
                target_data_dir: target.clone(),
            },
        )
        .expect_err("protected target rejected");

        assert!(error.contains("target database already exists"));
        assert_eq!(station_count(&source.join(DATABASE_FILE)), 1);
        assert_eq!(station_count(&target.join(DATABASE_FILE)), 0);
        assert_eq!(
            fs::read_to_string(default_data_dir.join(CONFIG_FILE)).expect("config bytes"),
            before_config
        );
    }

    #[test]
    fn relocation_backup_or_validation_failure_keeps_source_active() {
        let root = temp_root("bad-source");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&source).expect("source dir");
        fs::write(source.join(DATABASE_FILE), b"not sqlite").expect("invalid db");
        write_relocation_intent(&default_data_dir, &source, &target).expect("intent");
        let before_config =
            fs::read_to_string(default_data_dir.join(CONFIG_FILE)).expect("config bytes");

        assert!(apply_trusted_relocation(
            &default_data_dir,
            &DataStoreRelocationIntent {
                source_data_dir: source.clone(),
                target_data_dir: target.clone()
            },
        )
        .is_err());

        assert!(!target.join(DATABASE_FILE).exists());
        assert_eq!(
            fs::read_to_string(default_data_dir.join(CONFIG_FILE)).expect("config bytes"),
            before_config
        );
    }

    #[test]
    fn non_trusted_legacy_config_is_not_relocated() {
        let root = temp_root("legacy-intent");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&default_data_dir).expect("default");
        fs::write(
            default_data_dir.join(CONFIG_FILE),
            format!(
                r#"{{"pendingDataDir":"{}","sourceDataDir":"{}"}}"#,
                slash(&target),
                slash(&source)
            ),
        )
        .expect("legacy config");

        assert!(
            read_config(&default_data_dir.join(CONFIG_FILE))
                .expect("config")
                .expect("present")
                .version
                == 1
        );
        assert!(!target.join(DATABASE_FILE).exists());
    }

    fn create_station_db(data_dir: &Path, station: &str) {
        fs::create_dir_all(data_dir).expect("data dir");
        let db = Connection::open(data_dir.join(DATABASE_FILE)).expect("db");
        db.execute_batch("CREATE TABLE stations(name TEXT); CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);").expect("schema");
        db.execute("INSERT INTO stations VALUES (?)", [station])
            .expect("station");
    }

    fn create_schema_only_db(data_dir: &Path) {
        fs::create_dir_all(data_dir).expect("data dir");
        let db = Connection::open(data_dir.join(DATABASE_FILE)).expect("db");
        db.execute_batch("CREATE TABLE stations(name TEXT); CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);").expect("schema");
    }

    fn enable_wal_and_insert(path: &Path, station: &str) {
        let db = Connection::open(path).expect("db");
        db.pragma_update(None, "journal_mode", "WAL").expect("wal");
        db.execute("INSERT INTO stations VALUES (?)", [station])
            .expect("station");
        assert!(path.with_extension("sqlite3-wal").exists());
    }

    fn station_count(path: &Path) -> i64 {
        Connection::open(path)
            .expect("db")
            .query_row("SELECT COUNT(*) FROM stations", [], |row| row.get(0))
            .expect("count")
    }

    fn temp_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("relay-pool-relocation-{name}-{unique}"))
    }

    fn slash(path: &Path) -> String {
        path.to_string_lossy().replace('\\', "/")
    }

    #[allow(dead_code)]
    fn v2(
        active_data_dir: Option<PathBuf>,
        pending_data_dir: Option<PathBuf>,
        source_data_dir: Option<PathBuf>,
    ) -> DataDirConfigV2 {
        DataDirConfigV2 {
            version: 2,
            active_data_dir,
            pending_data_dir,
            source_data_dir,
            updated_at: "2026-07-17T00:00:00Z".to_string(),
        }
    }
}
