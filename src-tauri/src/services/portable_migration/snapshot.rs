use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};

use chrono::{SecondsFormat, Utc};
use tokio_util::sync::CancellationToken;

use crate::persistence::{create_verified_backup_from_path, error::PersistenceError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortableExportSnapshot {
    pub(crate) source_path: PathBuf,
    pub(crate) snapshot_path: PathBuf,
    pub(crate) created_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum PortableSnapshotError {
    #[error("portable migration snapshot was cancelled")]
    Cancelled,
    #[error("portable migration snapshot destination already exists")]
    DestinationExists,
    #[error("portable migration snapshot parent is invalid")]
    InvalidParent,
    #[error("portable migration direct file copy is unsafe for live WAL databases")]
    UnsafeDirectCopy,
    #[error("portable migration snapshot I/O failed")]
    Io,
    #[error("portable migration snapshot verification failed")]
    Persistence,
}

impl From<PersistenceError> for PortableSnapshotError {
    fn from(_: PersistenceError) -> Self {
        Self::Persistence
    }
}

pub(crate) async fn create_consistent_snapshot(
    source_path: &Path,
    snapshot_path: &Path,
    cancellation: Option<&CancellationToken>,
) -> Result<PortableExportSnapshot, PortableSnapshotError> {
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        return Err(PortableSnapshotError::Cancelled);
    }
    if snapshot_path.exists() {
        return Err(PortableSnapshotError::DestinationExists);
    }
    let parent = snapshot_path
        .parent()
        .ok_or(PortableSnapshotError::InvalidParent)?;
    fs::create_dir_all(parent).map_err(|_| PortableSnapshotError::Io)?;

    let backup = create_verified_backup_from_path(source_path, snapshot_path).await?;
    if cancellation.is_some_and(CancellationToken::is_cancelled) {
        remove_snapshot_file(&backup.final_path)?;
        return Err(PortableSnapshotError::Cancelled);
    }

    Ok(PortableExportSnapshot {
        source_path: source_path.to_path_buf(),
        snapshot_path: backup.final_path,
        created_at: Utc::now().to_rfc3339_opts(SecondsFormat::Millis, true),
    })
}

pub(crate) fn reject_direct_sqlite_copy_source(
    source_path: &Path,
) -> Result<(), PortableSnapshotError> {
    for suffix in ["wal", "shm"] {
        let sidecar = sqlite_sidecar_path(source_path, suffix)?;
        if sidecar
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
        {
            return Err(PortableSnapshotError::UnsafeDirectCopy);
        }
    }
    Ok(())
}

pub(crate) fn remove_snapshot_file(path: &Path) -> Result<(), PortableSnapshotError> {
    if path.exists() {
        fs::remove_file(path).map_err(|_| PortableSnapshotError::Io)?;
    }
    Ok(())
}

fn sqlite_sidecar_path(path: &Path, suffix: &str) -> Result<PathBuf, PortableSnapshotError> {
    let file_name = path
        .file_name()
        .ok_or(PortableSnapshotError::InvalidParent)?;
    let mut sidecar_name = OsString::from(file_name);
    sidecar_name.push(format!("-{suffix}"));
    Ok(path.with_file_name(sidecar_name))
}

#[cfg(test)]
mod tests {
    use sqlx::{Connection, Row, SqliteConnection};

    use super::*;

    #[tokio::test]
    async fn online_snapshot_is_a_consistency_boundary_for_later_writes() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        let snapshot = directory.path().join("snapshot.sqlite3");
        let runtime = crate::persistence::runtime::PersistenceRuntime::initialize_new(&source)
            .await
            .expect("runtime");

        runtime
            .handle()
            .write(|session| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE settings SET value = '15' WHERE key = 'collector_interval_minutes'",
                    )
                    .execute(session.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("first write");

        let created = create_consistent_snapshot(&source, &snapshot, None)
            .await
            .expect("snapshot");

        runtime
            .handle()
            .write(|session| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE settings SET value = '60' WHERE key = 'collector_interval_minutes'",
                    )
                    .execute(session.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("second write");

        let mut connection = SqliteConnection::connect(&format!(
            "sqlite:{}?mode=ro",
            created.snapshot_path.display()
        ))
        .await
        .expect("open snapshot");
        let value: String =
            sqlx::query("SELECT value FROM settings WHERE key = 'collector_interval_minutes'")
                .fetch_one(&mut connection)
                .await
                .expect("setting")
                .get(0);
        connection.close().await.expect("close");
        assert_eq!(value, "15");
    }

    #[test]
    fn direct_copy_guard_rejects_live_wal_sidecars() {
        let directory = tempfile::tempdir().expect("tempdir");
        let source = directory.path().join("source.sqlite3");
        std::fs::write(&source, b"not used").expect("source");
        std::fs::write(
            sqlite_sidecar_path(&source, "wal").expect("wal path"),
            b"live wal",
        )
        .expect("wal");

        assert_eq!(
            reject_direct_sqlite_copy_source(&source).unwrap_err(),
            PortableSnapshotError::UnsafeDirectCopy
        );
    }
}
