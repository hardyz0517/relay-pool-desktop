use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::SqliteConnectOptions, ConnectOptions, Connection, Executor, Row, SqliteConnection,
};

use super::{profiles, DetectedLegacyProfile, LegacySchemaFingerprint, UpgradeError};

pub(crate) const REQUEST_LIFECYCLE_COLUMNS: &[&str] = &[
    "endpoint",
    "terminal_kind",
    "terminal_code",
    "terminal_detail",
    "protocol_completed",
    "delivery_terminal",
    "selected_attempt_ordinal",
    "terminal_at_ms",
];

pub(crate) struct LegacyReadSession {
    connection: SqliteConnection,
    snapshot_dir: PathBuf,
}

impl LegacyReadSession {
    pub(crate) async fn open(path: &Path) -> Result<Self, UpgradeError> {
        if !path.is_file() {
            return Err(UpgradeError::MissingLegacyDatabase);
        }
        let snapshot = SourceSnapshot::capture(path)?;
        let options = SqliteConnectOptions::new()
            .filename(&snapshot.database_path)
            .create_if_missing(false)
            .read_only(true)
            .busy_timeout(Duration::from_secs(5))
            .disable_statement_logging();
        let connection_result = async {
            let mut connection = SqliteConnection::connect_with(&options).await?;
            connection.execute("PRAGMA query_only = ON").await?;
            let integrity: String = sqlx::query_scalar("PRAGMA integrity_check")
                .fetch_one(&mut connection)
                .await?;
            if integrity != "ok" {
                return Err(UpgradeError::LegacyIntegrityFailed);
            }
            Ok(connection)
        }
        .await;
        let connection = match connection_result {
            Ok(connection) => connection,
            Err(error) => {
                let _ = fs::remove_dir_all(&snapshot.directory);
                return Err(error);
            }
        };
        Ok(Self {
            connection,
            snapshot_dir: snapshot.directory,
        })
    }

    pub(crate) fn connection(&mut self) -> &mut SqliteConnection {
        &mut self.connection
    }

    pub(crate) async fn schema_fingerprint(
        &mut self,
    ) -> Result<LegacySchemaFingerprint, UpgradeError> {
        semantic_schema_fingerprint(&mut self.connection).await
    }

    pub(crate) async fn close(self) -> Result<(), UpgradeError> {
        let Self {
            connection,
            snapshot_dir,
        } = self;
        connection.close().await?;
        fs::remove_dir_all(&snapshot_dir)
            .map_err(crate::persistence::error::PersistenceError::from)?;
        Ok(())
    }
}

struct SourceSnapshot {
    directory: PathBuf,
    database_path: PathBuf,
}

impl SourceSnapshot {
    fn capture(source: &Path) -> Result<Self, UpgradeError> {
        let before = source_file_set_evidence(source)?;
        let directory = unique_snapshot_dir();
        fs::create_dir_all(&directory)
            .map_err(crate::persistence::error::PersistenceError::from)?;
        let file_name = source
            .file_name()
            .ok_or(UpgradeError::MissingLegacyDatabase)?;
        let database_path = directory.join(file_name);
        let candidates = [
            (source.to_path_buf(), database_path.clone()),
            (
                sidecar_path(source, "-wal"),
                sidecar_path(&database_path, "-wal"),
            ),
            (
                sidecar_path(source, "-shm"),
                sidecar_path(&database_path, "-shm"),
            ),
        ];
        for (from, to) in candidates {
            if from.is_file() {
                if let Err(error) = fs::copy(&from, &to) {
                    let _ = fs::remove_dir_all(&directory);
                    return Err(crate::persistence::error::PersistenceError::from(error).into());
                }
            }
        }
        let after = match source_file_set_evidence(source) {
            Ok(evidence) => evidence,
            Err(error) => {
                let _ = fs::remove_dir_all(&directory);
                return Err(error);
            }
        };
        if before != after {
            let _ = fs::remove_dir_all(&directory);
            return Err(UpgradeError::LegacySourceChanged);
        }
        Ok(Self {
            directory,
            database_path,
        })
    }
}

#[derive(PartialEq, Eq)]
struct FileEvidence {
    path: PathBuf,
    len: u64,
    modified: SystemTime,
    sha256: Vec<u8>,
}

fn source_file_set_evidence(source: &Path) -> Result<Vec<FileEvidence>, UpgradeError> {
    [
        source.to_path_buf(),
        sidecar_path(source, "-wal"),
        sidecar_path(source, "-shm"),
    ]
    .into_iter()
    .filter(|path| path.is_file())
    .map(|path| {
        let metadata =
            fs::metadata(&path).map_err(crate::persistence::error::PersistenceError::from)?;
        Ok(FileEvidence {
            sha256: sha256_file(&path)?,
            path,
            len: metadata.len(),
            modified: metadata
                .modified()
                .map_err(crate::persistence::error::PersistenceError::from)?,
        })
    })
    .collect()
}

fn sha256_file(path: &Path) -> Result<Vec<u8>, UpgradeError> {
    let mut file =
        fs::File::open(path).map_err(crate::persistence::error::PersistenceError::from)?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(crate::persistence::error::PersistenceError::from)?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest.finalize().to_vec())
}

fn sidecar_path(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.display()))
}

fn unique_snapshot_dir() -> PathBuf {
    static NEXT_SNAPSHOT: AtomicU64 = AtomicU64::new(0);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "relay-pool-legacy-read-{}-{timestamp}-{}",
        std::process::id(),
        NEXT_SNAPSHOT.fetch_add(1, Ordering::Relaxed)
    ))
}

pub(crate) async fn detect_profile(
    source_path: &Path,
) -> Result<DetectedLegacyProfile, UpgradeError> {
    let mut session = LegacyReadSession::open(source_path).await?;
    let fingerprint = session.schema_fingerprint().await?;
    let profile = profiles::by_fingerprint(&fingerprint);
    session.close().await?;
    profile.ok_or(UpgradeError::UnsupportedLegacySchema)
}

pub(crate) fn source_candidate_identity(source_path: &Path) -> Result<String, UpgradeError> {
    let evidence = source_file_set_evidence(source_path)?;
    let mut digest = Sha256::new();
    digest.update((evidence.len() as u64).to_le_bytes());
    for (index, file) in evidence.iter().enumerate() {
        digest.update((index as u64).to_le_bytes());
        digest.update(file.len.to_le_bytes());
        digest.update((file.sha256.len() as u64).to_le_bytes());
        digest.update(&file.sha256);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

async fn semantic_schema_fingerprint(
    connection: &mut SqliteConnection,
) -> Result<LegacySchemaFingerprint, UpgradeError> {
    let rows = sqlx::query(
        r#"
        SELECT type, name, tbl_name
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
          AND type IN ('table', 'view', 'trigger')
        ORDER BY type ASC, name ASC, tbl_name ASC
        "#,
    )
    .fetch_all(&mut *connection)
    .await?;

    let mut base_records = Vec::new();
    let mut capability_records = Vec::new();
    let mut request_attempts_present = false;
    let mut lifecycle_columns_present = BTreeSet::new();

    for row in rows {
        let object_type: String = row.try_get("type")?;
        let name: String = row.try_get("name")?;
        let table_name: String = row.try_get("tbl_name")?;
        let normalized_name = normalize_identifier(&name);
        let normalized_table = normalize_identifier(&table_name);
        let is_attempts_table = object_type == "table" && normalized_name == "request_attempts";
        if is_attempts_table {
            request_attempts_present = true;
        }

        let object_record = record(
            "object",
            &[&object_type, &normalized_name, &normalized_table],
        );
        if is_attempts_table {
            capability_records.push(object_record);
        } else {
            base_records.push(object_record);
        }

        if object_type != "table" {
            continue;
        }

        let columns = sqlx::query(
            r#"
            SELECT name, type, "notnull" AS is_not_null, pk, hidden
            FROM pragma_table_xinfo(?1)
            "#,
        )
        .bind(&name)
        .fetch_all(&mut *connection)
        .await?;
        for column in columns {
            let column_name: String = column.try_get("name")?;
            let normalized_column = normalize_identifier(&column_name);
            let column_type: String = column.try_get("type")?;
            let normalized_type = normalize_declared_type(&column_type);
            let is_not_null: i64 = column.try_get("is_not_null")?;
            let primary_key_position: i64 = column.try_get("pk")?;
            let hidden: i64 = column.try_get("hidden")?;
            let column_record = record(
                "column",
                &[
                    &normalized_name,
                    &normalized_column,
                    &normalized_type,
                    &is_not_null.to_string(),
                    &primary_key_position.to_string(),
                    &hidden.to_string(),
                ],
            );
            if is_attempts_table
                || (normalized_name == "request_logs"
                    && REQUEST_LIFECYCLE_COLUMNS.contains(&normalized_column.as_str()))
            {
                if normalized_name == "request_logs" {
                    lifecycle_columns_present.insert(normalized_column);
                }
                capability_records.push(column_record);
            } else {
                base_records.push(column_record);
            }
        }

        let foreign_keys = sqlx::query(
            r#"
            SELECT "table" AS referenced_table, "from" AS source_column,
                   "to" AS referenced_column, on_update, on_delete, "match" AS match_kind
            FROM pragma_foreign_key_list(?1)
            "#,
        )
        .bind(&name)
        .fetch_all(&mut *connection)
        .await?;
        for foreign_key in foreign_keys {
            let referenced_table: String = foreign_key.try_get("referenced_table")?;
            let source_column: Option<String> = foreign_key.try_get("source_column")?;
            let referenced_column: Option<String> = foreign_key.try_get("referenced_column")?;
            let on_update: String = foreign_key.try_get("on_update")?;
            let on_delete: String = foreign_key.try_get("on_delete")?;
            let match_kind: String = foreign_key.try_get("match_kind")?;
            let foreign_key_record = record(
                "foreign_key",
                &[
                    &normalized_name,
                    &normalize_identifier(&referenced_table),
                    &normalize_identifier(source_column.as_deref().unwrap_or("")),
                    &normalize_identifier(referenced_column.as_deref().unwrap_or("")),
                    &on_update.to_ascii_uppercase(),
                    &on_delete.to_ascii_uppercase(),
                    &match_kind.to_ascii_uppercase(),
                ],
            );
            if is_attempts_table {
                capability_records.push(foreign_key_record);
            } else {
                base_records.push(foreign_key_record);
            }
        }

        collect_unique_constraints(
            connection,
            &name,
            &normalized_name,
            is_attempts_table,
            &mut base_records,
            &mut capability_records,
        )
        .await?;
    }

    let has_capability_markers = request_attempts_present || !lifecycle_columns_present.is_empty();
    let request_lifecycle_hash = has_capability_markers.then(|| hash_records(capability_records));
    Ok(LegacySchemaFingerprint {
        base_hash: hash_records(base_records),
        request_lifecycle_hash,
    })
}

async fn collect_unique_constraints(
    connection: &mut SqliteConnection,
    table_name: &str,
    normalized_table: &str,
    is_attempts_table: bool,
    base_records: &mut Vec<String>,
    capability_records: &mut Vec<String>,
) -> Result<(), UpgradeError> {
    let indexes = sqlx::query(
        r#"
        SELECT name, "unique" AS is_unique, origin, partial
        FROM pragma_index_list(?1)
        "#,
    )
    .bind(table_name)
    .fetch_all(&mut *connection)
    .await?;
    for index in indexes {
        let is_unique: i64 = index.try_get("is_unique")?;
        let origin: String = index.try_get("origin")?;
        if is_unique == 0 || origin == "pk" {
            continue;
        }
        let index_name: String = index.try_get("name")?;
        let partial: i64 = index.try_get("partial")?;
        let columns = sqlx::query(
            r#"
            SELECT seqno, COALESCE(name, '') AS name, "desc" AS is_desc,
                   COALESCE(coll, '') AS collation, "key" AS is_key
            FROM pragma_index_xinfo(?1)
            WHERE "key" = 1
            ORDER BY seqno ASC
            "#,
        )
        .bind(index_name)
        .fetch_all(&mut *connection)
        .await?;
        let mut fields = vec![normalized_table.to_string(), origin, partial.to_string()];
        for column in columns {
            let name: String = column.try_get("name")?;
            let is_desc: i64 = column.try_get("is_desc")?;
            let collation: String = column.try_get("collation")?;
            let is_key: i64 = column.try_get("is_key")?;
            fields.push(format!(
                "{}:{}:{}:{}",
                normalize_identifier(&name),
                is_desc,
                collation.to_ascii_uppercase(),
                is_key
            ));
        }
        let borrowed = fields.iter().map(String::as_str).collect::<Vec<_>>();
        let constraint_record = record("unique", &borrowed);
        if is_attempts_table {
            capability_records.push(constraint_record);
        } else {
            base_records.push(constraint_record);
        }
    }
    Ok(())
}

fn normalize_identifier(value: &str) -> String {
    value.to_ascii_lowercase()
}

fn normalize_declared_type(value: &str) -> String {
    value
        .split_ascii_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_uppercase()
}

fn record(kind: &str, fields: &[&str]) -> String {
    std::iter::once(kind)
        .chain(fields.iter().copied())
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

fn hash_records(mut records: Vec<String>) -> String {
    records.sort_unstable();
    let mut digest = Sha256::new();
    for (index, item) in records.iter().enumerate() {
        if index > 0 {
            digest.update(b"\n");
        }
        digest.update(item.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
