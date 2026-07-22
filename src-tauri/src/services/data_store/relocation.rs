use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use crate::persistence::{
    create_verified_backup_from_path, upgrade_recovery_executor::UPGRADE_JOURNAL_FILE,
};

use super::{
    config::{
        create_installation_marker, read_config_v3, write_config_v3, DataDirConfigV3,
        DatabaseGeneration,
    },
    inspect::inspect_candidate,
    types::{CandidateHealth, CandidateRole, DataStoreRelocationIntent},
};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";

pub(crate) fn write_relocation_intent(
    default_data_dir: &Path,
    active_data_dir: &Path,
    pending_data_dir: &Path,
) -> Result<(), String> {
    ensure_no_upgrade_journal(default_data_dir)?;
    let database_generation = configured_generation(default_data_dir)?;
    write_config_v3(
        &default_data_dir.join(DATA_DIR_CONFIG_FILE),
        &DataDirConfigV3 {
            version: 3,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: Some(pending_data_dir.to_path_buf()),
            source_data_dir: Some(active_data_dir.to_path_buf()),
            database_generation,
            updated_at: updated_at(),
        },
    )
}

pub(crate) fn write_active_data_dir_selection(
    default_data_dir: &Path,
    active_data_dir: &Path,
) -> Result<(), String> {
    let database_generation = configured_generation(default_data_dir)?;
    write_config_v3(
        &default_data_dir.join(DATA_DIR_CONFIG_FILE),
        &DataDirConfigV3 {
            version: 3,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: None,
            source_data_dir: None,
            database_generation,
            updated_at: updated_at(),
        },
    )
}

pub(crate) fn apply_trusted_relocation(
    default_data_dir: &Path,
    intent: &DataStoreRelocationIntent,
) -> Result<PathBuf, String> {
    ensure_no_upgrade_journal(default_data_dir)?;
    let database_file = configured_generation(default_data_dir)?.database_file();
    let source_db_path = intent.source_data_dir.join(database_file);
    let target_db_path = intent.target_data_dir.join(database_file);
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
        "{database_file}.relocating-{}-{}",
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

fn configured_generation(default_data_dir: &Path) -> Result<DatabaseGeneration, String> {
    Ok(
        read_config_v3(&default_data_dir.join(DATA_DIR_CONFIG_FILE))?
            .map(|config| config.database_generation)
            .unwrap_or(DatabaseGeneration::One),
    )
}

fn ensure_no_upgrade_journal(default_data_dir: &Path) -> Result<(), String> {
    if default_data_dir.join(UPGRADE_JOURNAL_FILE).exists() {
        return Err(
            "data relocation is unavailable while a persistence upgrade is in progress".to_string(),
        );
    }
    Ok(())
}

fn copy_sqlite_database(source_db_path: &Path, temp_db_path: &Path) -> Result<(), String> {
    tauri::async_runtime::block_on(create_verified_backup_from_path(
        source_db_path,
        temp_db_path,
    ))
    .map(|_| ())
    .map_err(|error| format!("failed to create verified relocation database: {error}"))
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
    use crate::services::data_store::config::{
        read_config, read_config_v3, write_config_v3, DataDirConfigV3, DatabaseGeneration,
    };
    use crate::services::data_store::relocation::{
        apply_trusted_relocation, write_relocation_intent,
    };
    use crate::services::data_store::types::DataStoreRelocationIntent;
    use sqlx::SqliteConnection;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::SystemTime,
    };

    const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
    const DATABASE_FILE_V2: &str = "relay-pool-desktop-v2.sqlite3";
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
    fn generation_two_relocation_preserves_authoritative_generation() {
        let root = temp_root("v2-generation-relocation");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        create_station_db_file(&source.join(DATABASE_FILE_V2), "v2-station");
        write_config_v3(
            &default_data_dir.join(CONFIG_FILE),
            &DataDirConfigV3 {
                version: 3,
                active_data_dir: Some(source.clone()),
                pending_data_dir: None,
                source_data_dir: None,
                database_generation: DatabaseGeneration::Two,
                updated_at: "2026-07-21T00:00:00Z".to_string(),
            },
        )
        .expect("generation-two config");
        write_relocation_intent(&default_data_dir, &source, &target).expect("intent");

        apply_trusted_relocation(
            &default_data_dir,
            &DataStoreRelocationIntent {
                source_data_dir: source.clone(),
                target_data_dir: target.clone(),
            },
        )
        .expect("relocate generation two");

        assert_eq!(station_count(&target.join(DATABASE_FILE_V2)), 1);
        assert!(!target.join(DATABASE_FILE).exists());
        let config = read_config_v3(&default_data_dir.join(CONFIG_FILE))
            .expect("read V3 config")
            .expect("config");
        assert_eq!(config.database_generation, DatabaseGeneration::Two);
        assert_eq!(config.active_data_dir, Some(target));
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

    #[test]
    fn upgrade_journal_blocks_new_relocation_intent_before_config_write() {
        let root = temp_root("journal-blocks-intent");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        fs::create_dir_all(&default_data_dir).expect("default");
        let journal = default_data_dir
            .join(crate::persistence::upgrade_recovery_executor::UPGRADE_JOURNAL_FILE);
        fs::write(&journal, b"protected-upgrade-journal").expect("journal");

        let error = write_relocation_intent(&default_data_dir, &source, &target)
            .expect_err("journal must block relocation intent");

        assert!(error.contains("upgrade is in progress"));
        assert!(!default_data_dir.join(CONFIG_FILE).exists());
        assert_eq!(
            fs::read(&journal).expect("journal unchanged"),
            b"protected-upgrade-journal"
        );
    }

    #[test]
    fn upgrade_journal_blocks_relocation_before_source_target_or_config_changes() {
        let root = temp_root("journal-blocks-apply");
        let default_data_dir = root.join("default");
        let source = root.join("source");
        let target = root.join("target");
        create_station_db(&source, "source-station");
        write_relocation_intent(&default_data_dir, &source, &target).expect("intent");
        let config = default_data_dir.join(CONFIG_FILE);
        let journal = default_data_dir
            .join(crate::persistence::upgrade_recovery_executor::UPGRADE_JOURNAL_FILE);
        fs::write(&journal, b"protected-upgrade-journal").expect("journal");
        let before_source = fs::read(source.join(DATABASE_FILE)).expect("source bytes");
        let before_config = fs::read(&config).expect("config bytes");

        let error = apply_trusted_relocation(
            &default_data_dir,
            &DataStoreRelocationIntent {
                source_data_dir: source.clone(),
                target_data_dir: target.clone(),
            },
        )
        .expect_err("journal must block relocation");

        assert!(error.contains("upgrade is in progress"));
        assert_eq!(
            fs::read(source.join(DATABASE_FILE)).expect("source"),
            before_source
        );
        assert_eq!(fs::read(&config).expect("config"), before_config);
        assert_eq!(
            fs::read(&journal).expect("journal unchanged"),
            b"protected-upgrade-journal"
        );
        assert!(!target.exists());
    }

    fn create_station_db(data_dir: &Path, station: &str) {
        fs::create_dir_all(data_dir).expect("data dir");
        create_station_db_file(&data_dir.join(DATABASE_FILE), station);
    }

    fn create_station_db_file(path: &Path, station: &str) {
        fs::create_dir_all(path.parent().expect("database parent")).expect("data dir");
        let mut db = open(path);
        execute_batch(&mut db, "CREATE TABLE stations(name TEXT); CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);");
        execute_with_text(&mut db, "INSERT INTO stations VALUES (?1)", station);
    }

    fn create_schema_only_db(data_dir: &Path) {
        fs::create_dir_all(data_dir).expect("data dir");
        let mut db = open(&data_dir.join(DATABASE_FILE));
        execute_batch(&mut db, "CREATE TABLE stations(name TEXT); CREATE TABLE settings(key TEXT PRIMARY KEY, value TEXT);");
    }

    fn enable_wal_and_insert(path: &Path, station: &str) {
        let mut db = open(path);
        execute_with_text(&mut db, "INSERT INTO stations VALUES (?1)", station);
        assert!(path.with_extension("sqlite3-wal").exists());
    }

    fn station_count(path: &Path) -> i64 {
        crate::services::data_store::test_support::query_i64(path, "SELECT COUNT(*) FROM stations")
    }

    fn open(path: &Path) -> SqliteConnection {
        crate::services::data_store::test_support::open_database(path)
    }

    fn execute_batch(connection: &mut SqliteConnection, statements: &str) {
        crate::services::data_store::test_support::execute_batch(connection, statements);
    }

    fn execute_with_text(connection: &mut SqliteConnection, statement: &str, value: &str) {
        crate::services::data_store::test_support::execute_with_text(connection, statement, value);
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
}
