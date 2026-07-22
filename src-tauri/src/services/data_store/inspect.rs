use std::{collections::BTreeMap, fs, path::Path};

use crate::persistence::{
    inspect_relay_pool_database, upgrade_recovery_executor::read_legacy_tombstone,
    ReadOnlyDatabaseHealth, ReadOnlyDatabaseInspection,
};

use super::types::{CandidateHealth, CandidateRole, DataStoreCandidate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectedDataStoreCandidate {
    pub candidate: DataStoreCandidate,
    pub contains_relay_pool_schema: bool,
    pub is_legacy_tombstone: bool,
}

pub(crate) fn inspect_candidate(
    path: &Path,
    role: CandidateRole,
) -> Result<InspectedDataStoreCandidate, String> {
    inspect_with_quick_check(path, role, None)
}

fn inspect_with_quick_check(
    path: &Path,
    role: CandidateRole,
    quick_check_override: Option<String>,
) -> Result<InspectedDataStoreCandidate, String> {
    if !path.is_file() {
        return Ok(make_candidate(
            path,
            role,
            CandidateHealth::Missing,
            false,
            BTreeMap::new(),
            None,
            false,
        ));
    }

    match read_legacy_tombstone(path) {
        Ok(Some(_)) => {
            let metadata = fs::metadata(path).map_err(|error| {
                format!(
                    "failed to read candidate metadata {}: {error}",
                    path.display()
                )
            })?;
            return Ok(make_candidate(
                path,
                role,
                CandidateHealth::InvalidSqlite,
                false,
                BTreeMap::new(),
                Some(&metadata),
                true,
            ));
        }
        Ok(None) => {}
        Err(_) => {
            return Ok(make_candidate(
                path,
                role,
                CandidateHealth::InvalidSqlite,
                false,
                BTreeMap::new(),
                fs::metadata(path).ok().as_ref(),
                false,
            ));
        }
    }

    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to read candidate metadata {}: {error}",
            path.display()
        )
    })?;
    let with_metadata = |health, schema, counts| {
        make_candidate(
            path,
            role.clone(),
            health,
            schema,
            counts,
            Some(&metadata),
            false,
        )
    };
    if quick_check_override.is_some_and(|value| !value.eq_ignore_ascii_case("ok")) {
        return Ok(with_metadata(
            CandidateHealth::IntegrityFailed,
            false,
            BTreeMap::new(),
        ));
    }

    let inspection: ReadOnlyDatabaseInspection =
        tauri::async_runtime::block_on(inspect_relay_pool_database(path))
            .map_err(|error| format!("failed to inspect candidate database: {error}"))?;
    let health = match inspection.health {
        ReadOnlyDatabaseHealth::Healthy => CandidateHealth::Healthy,
        ReadOnlyDatabaseHealth::IntegrityFailed => CandidateHealth::IntegrityFailed,
        ReadOnlyDatabaseHealth::InvalidSqlite => CandidateHealth::InvalidSqlite,
        ReadOnlyDatabaseHealth::Unreadable => CandidateHealth::Unreadable,
    };
    let contains_relay_pool_schema = !inspection.table_counts.is_empty();
    Ok(with_metadata(
        health,
        contains_relay_pool_schema,
        inspection.table_counts,
    ))
}

fn make_candidate(
    path: &Path,
    role: CandidateRole,
    health: CandidateHealth,
    contains_relay_pool_schema: bool,
    counts: BTreeMap<String, i64>,
    metadata: Option<&fs::Metadata>,
    is_legacy_tombstone: bool,
) -> InspectedDataStoreCandidate {
    let size_bytes = metadata.map(fs::Metadata::len);
    let modified_at = metadata
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|duration| duration.as_secs().to_string());
    InspectedDataStoreCandidate {
        candidate: DataStoreCandidate {
            id: format!("{:?}:{}", role, path.display()),
            role,
            path: path.display().to_string(),
            health,
            schema_compatible: contains_relay_pool_schema,
            size_bytes,
            modified_at,
            counts,
        },
        contains_relay_pool_schema,
        is_legacy_tombstone,
    }
}

#[cfg(test)]
mod tests {
    use super::{inspect_candidate, inspect_with_quick_check};
    use crate::persistence::{
        upgrade_journal::UpgradeAttemptId, upgrade_recovery_executor::replace_legacy_with_tombstone,
    };
    use crate::services::data_store::types::{CandidateHealth, CandidateRole};
    use sqlx::SqliteConnection;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::SystemTime,
    };

    #[test]
    fn missing_invalid_and_integrity_failed_are_classified_without_creating_paths() {
        let missing = temp_root("missing")
            .join("absent")
            .join("relay-pool-desktop.sqlite3");
        let inspected = inspect_candidate(&missing, CandidateRole::Located).expect("missing");
        assert_eq!(inspected.candidate.health, CandidateHealth::Missing);
        assert!(!missing.exists() && !missing.parent().expect("parent").exists());

        let invalid = db_path("invalid-header");
        fs::write(&invalid, b"not sqlite").expect("invalid db");
        let inspected = inspect_candidate(&invalid, CandidateRole::Located).expect("invalid");
        assert_eq!(inspected.candidate.health, CandidateHealth::InvalidSqlite);
        assert!(!inspected.contains_relay_pool_schema);

        let corrupt = protected_db("quick-check", &[("stations", 0)]);
        let inspected = inspect_with_quick_check(
            &corrupt,
            CandidateRole::Located,
            Some("database disk image is malformed".to_string()),
        )
        .expect("quick check");
        assert_eq!(inspected.candidate.health, CandidateHealth::IntegrityFailed);
    }

    #[test]
    fn protected_tables_return_allowlisted_counts_even_when_empty() {
        let populated = protected_db("healthy", &[("stations", 1), ("station_keys", 1)]);
        let empty = protected_db("empty-protected", &[("stations", 0)]);
        let populated = inspect_candidate(&populated, CandidateRole::Active).expect("populated");
        let empty = inspect_candidate(&empty, CandidateRole::Default).expect("empty");

        for inspected in [&populated, &empty] {
            assert_eq!(inspected.candidate.health, CandidateHealth::Healthy);
            assert!(inspected.contains_relay_pool_schema);
        }
        assert_eq!(populated.candidate.counts.get("stations"), Some(&1));
        assert_eq!(populated.candidate.counts.get("station_keys"), Some(&1));
        assert_eq!(empty.candidate.counts.get("stations"), Some(&0));
    }

    #[test]
    fn read_only_inspection_does_not_initialize_schema_or_touch_metadata() {
        let path = db_path("read-only");
        let mut connection = open(&path);
        sql(&mut connection, "CREATE TABLE unrelated(id TEXT)");
        crate::services::data_store::test_support::close_database(connection);
        let before = file_facts(&path);

        let inspected = inspect_candidate(&path, CandidateRole::Located).expect("unrelated");
        assert_eq!(file_facts(&path), before);
        assert_eq!(inspected.candidate.health, CandidateHealth::Healthy);
        assert!(!inspected.contains_relay_pool_schema);
        let settings_exists = crate::services::data_store::test_support::query_i64(
            &path,
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'settings')",
        );
        assert_eq!(settings_exists, 0);
    }

    #[test]
    fn serialized_summary_excludes_station_urls_keys_cookies_and_secret_values() {
        let path = db_path("redaction");
        let mut connection = open(&path);
        sql(&mut connection, "CREATE TABLE stations(name TEXT, base_url TEXT, cookie TEXT); CREATE TABLE settings(key TEXT, value TEXT); INSERT INTO stations VALUES ('Sensitive Station', 'https://secret.example/v1', 'session-cookie'); INSERT INTO settings VALUES ('local_key', 'sk-sensitive');");
        crate::services::data_store::test_support::close_database(connection);

        let inspected = inspect_candidate(&path, CandidateRole::Located).expect("redaction");
        let serialized = serde_json::to_string(&inspected.candidate).expect("serialize");
        for secret in [
            "Sensitive Station",
            "secret.example",
            "session-cookie",
            "sk-sensitive",
        ] {
            assert!(!serialized.contains(secret), "leaked {secret}");
        }
    }

    #[test]
    fn valid_tombstone_is_recognized_before_sqlite_open_without_exposing_attempt() {
        let path = db_path("legacy-tombstone");
        fs::write(&path, b"SQLite format 3\0legacy").expect("legacy");
        let attempt =
            UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000001").expect("attempt");
        replace_legacy_with_tombstone(&path, &attempt).expect("tombstone");

        let inspected = inspect_candidate(&path, CandidateRole::Active).expect("inspect");

        assert!(inspected.is_legacy_tombstone);
        assert_eq!(inspected.candidate.health, CandidateHealth::InvalidSqlite);
        assert!(!inspected.contains_relay_pool_schema);
        let summary = serde_json::to_string(&inspected.candidate).expect("summary");
        assert!(!summary.contains(attempt.as_str()));
    }

    fn protected_db(name: &str, tables: &[(&str, usize)]) -> PathBuf {
        let path = db_path(name);
        let mut connection = open(&path);
        for (table, rows) in tables {
            create_table(&mut connection, table);
            for _ in 0..*rows {
                sql(
                    &mut connection,
                    &format!("INSERT INTO {table}(name) VALUES ('value')"),
                );
            }
        }
        crate::services::data_store::test_support::close_database(connection);
        path
    }

    fn db_path(name: &str) -> PathBuf {
        let root = temp_root(name);
        fs::create_dir_all(&root).expect("root");
        root.join("relay-pool-desktop.sqlite3")
    }

    fn temp_root(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("relay-pool-inspect-{name}-{unique}"))
    }

    fn create_table(connection: &mut SqliteConnection, table: &str) {
        sql(connection, &format!("CREATE TABLE {table}(name TEXT)"));
    }

    fn open(path: &Path) -> SqliteConnection {
        crate::services::data_store::test_support::open_database(path)
    }

    fn sql(connection: &mut SqliteConnection, sql: &str) {
        crate::services::data_store::test_support::execute_batch(connection, sql);
    }

    fn file_facts(path: &Path) -> (u64, Option<SystemTime>) {
        let metadata = fs::metadata(path).expect("metadata");
        (metadata.len(), metadata.modified().ok())
    }
}
