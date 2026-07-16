use std::{collections::BTreeMap, fs, path::Path};

use rusqlite::{Connection, OpenFlags};

use super::types::{CandidateHealth, CandidateRole, DataStoreCandidate};

const RECOGNIZED_TABLES: [&str; 4] = ["stations", "station_keys", "channel_monitors", "settings"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectedDataStoreCandidate {
    pub candidate: DataStoreCandidate,
    pub contains_relay_pool_schema: bool,
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
        ));
    }

    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "failed to read candidate metadata {}: {error}",
            path.display()
        )
    })?;
    let with_metadata = |health, schema, counts| {
        make_candidate(path, role.clone(), health, schema, counts, Some(&metadata))
    };
    let connection = match Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(with_metadata(
                classify_open_error(&error),
                false,
                BTreeMap::new(),
            ))
        }
    };

    let quick_check = match quick_check_override {
        Some(value) => value,
        None => match run_quick_check(&connection) {
            Ok(value) => value,
            Err(_) => {
                return Ok(with_metadata(
                    CandidateHealth::InvalidSqlite,
                    false,
                    BTreeMap::new(),
                ));
            }
        },
    };
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Ok(with_metadata(
            CandidateHealth::IntegrityFailed,
            false,
            BTreeMap::new(),
        ));
    }

    let tables = recognized_tables(&connection)?;
    let contains_relay_pool_schema = !tables.is_empty();
    let counts = count_tables(&connection, &tables)?;
    Ok(with_metadata(
        CandidateHealth::Healthy,
        contains_relay_pool_schema,
        counts,
    ))
}

fn make_candidate(
    path: &Path,
    role: CandidateRole,
    health: CandidateHealth,
    contains_relay_pool_schema: bool,
    counts: BTreeMap<String, i64>,
    metadata: Option<&fs::Metadata>,
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
    }
}

fn classify_open_error(error: &rusqlite::Error) -> CandidateHealth {
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("permission") || message.contains("access") {
        CandidateHealth::Unreadable
    } else {
        CandidateHealth::InvalidSqlite
    }
}

fn run_quick_check(connection: &Connection) -> Result<String, rusqlite::Error> {
    connection.query_row("PRAGMA quick_check", [], |row| row.get(0))
}

fn recognized_tables(connection: &Connection) -> Result<Vec<&'static str>, String> {
    let mut tables = Vec::new();
    for table in RECOGNIZED_TABLES {
        if table_exists(connection, table)? {
            tables.push(table);
        }
    }
    Ok(tables)
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|exists| exists == 1)
        .map_err(|error| format!("failed to read candidate schema: {error}"))
}

fn count_tables(
    connection: &Connection,
    tables: &[&'static str],
) -> Result<BTreeMap<String, i64>, String> {
    let mut counts = BTreeMap::new();
    for table in tables {
        let count = connection
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .map_err(|error| format!("failed to count candidate table {table}: {error}"))?;
        counts.insert((*table).to_string(), count);
    }
    Ok(counts)
}

#[cfg(test)]
mod tests {
    use super::{inspect_candidate, inspect_with_quick_check, table_exists};
    use crate::services::data_store::types::{CandidateHealth, CandidateRole};
    use rusqlite::Connection;
    use std::{fs, path::PathBuf, time::SystemTime};

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
        let connection = open(&path);
        sql(&connection, "CREATE TABLE unrelated(id TEXT)");
        drop(connection);
        let before = file_facts(&path);

        let inspected = inspect_candidate(&path, CandidateRole::Located).expect("unrelated");
        assert_eq!(file_facts(&path), before);
        assert_eq!(inspected.candidate.health, CandidateHealth::Healthy);
        assert!(!inspected.contains_relay_pool_schema);
        assert!(!table_exists(&open(&path), "settings").expect("table query"));
    }

    #[test]
    fn serialized_summary_excludes_station_urls_keys_cookies_and_secret_values() {
        let path = db_path("redaction");
        let connection = open(&path);
        connection.execute_batch("CREATE TABLE stations(name TEXT, base_url TEXT, cookie TEXT); CREATE TABLE settings(key TEXT, value TEXT); INSERT INTO stations VALUES ('Sensitive Station', 'https://secret.example/v1', 'session-cookie'); INSERT INTO settings VALUES ('local_key', 'sk-sensitive');").expect("fixture");
        drop(connection);

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

    fn protected_db(name: &str, tables: &[(&str, usize)]) -> PathBuf {
        let path = db_path(name);
        let connection = open(&path);
        for (table, rows) in tables {
            create_table(&connection, table);
            for _ in 0..*rows {
                sql(
                    &connection,
                    &format!("INSERT INTO {table}(name) VALUES ('value')"),
                );
            }
        }
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

    fn create_table(connection: &Connection, table: &str) {
        sql(connection, &format!("CREATE TABLE {table}(name TEXT)"));
    }

    fn open(path: &PathBuf) -> Connection {
        Connection::open(path).expect("db")
    }

    fn sql(connection: &Connection, sql: &str) {
        connection.execute(sql, []).expect("sql");
    }

    fn file_facts(path: &PathBuf) -> (u64, Option<SystemTime>) {
        let metadata = fs::metadata(path).expect("metadata");
        (metadata.len(), metadata.modified().ok())
    }
}
