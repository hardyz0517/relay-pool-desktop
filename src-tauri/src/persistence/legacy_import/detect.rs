use std::{
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

use super::{profiles, DetectedLegacyProfile, UpgradeError};

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

    pub(crate) async fn schema_hash(&mut self) -> Result<String, UpgradeError> {
        canonical_schema_hash(&mut self.connection).await
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
    let schema_hash = session.schema_hash().await?;
    let profile = profiles::by_schema_hash(&schema_hash);
    session.close().await?;
    profile.ok_or(UpgradeError::UnsupportedLegacySchema)
}

async fn canonical_schema_hash(connection: &mut SqliteConnection) -> Result<String, UpgradeError> {
    let rows = sqlx::query(
        r#"
        SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
        ORDER BY type ASC, name ASC, tbl_name ASC
        "#,
    )
    .fetch_all(connection)
    .await?;

    let mut digest = Sha256::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            digest.update(b"\n");
        }
        for (field_index, field) in ["type", "name", "tbl_name", "sql"].iter().enumerate() {
            if field_index > 0 {
                digest.update([0x1f]);
            }
            let value: String = row.try_get(*field)?;
            digest.update(value.as_bytes());
        }
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
