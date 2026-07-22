use std::{
    fs::{self, OpenOptions},
    path::{Path, PathBuf},
    time::Duration,
};

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    ConnectOptions, Connection, Row, SqlitePool,
};

use crate::persistence::error::PersistenceError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VerifiedBackup {
    pub(crate) final_path: PathBuf,
}

pub(crate) fn temporary_backup_path(final_path: &Path) -> PathBuf {
    let file_name = final_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("relay-pool-backup.sqlite3");
    final_path.with_file_name(format!("{file_name}.tmp"))
}

pub(crate) async fn create_verified_backup_from_path(
    source_path: &Path,
    final_path: &Path,
) -> Result<VerifiedBackup, PersistenceError> {
    if !source_path.is_file() {
        return Err(PersistenceError::MissingDatabase);
    }

    let options = SqliteConnectOptions::new()
        .filename(source_path)
        .read_only(true)
        .create_if_missing(false)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5));
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await?;
    let result = create_verified_backup(&pool, final_path).await;
    pool.close().await;
    result
}

pub(super) async fn create_verified_backup(
    pool: &SqlitePool,
    final_path: &Path,
) -> Result<VerifiedBackup, PersistenceError> {
    let parent = final_path.parent().ok_or(PersistenceError::IoFailed {
        kind: std::io::ErrorKind::InvalidInput,
    })?;
    fs::create_dir_all(parent)?;
    if final_path.exists() {
        return Err(PersistenceError::IoFailed {
            kind: std::io::ErrorKind::AlreadyExists,
        });
    }
    let temporary_path = temporary_backup_path(final_path);
    if temporary_path.exists() {
        return Err(PersistenceError::IoFailed {
            kind: std::io::ErrorKind::AlreadyExists,
        });
    }

    let mut connection = pool.acquire().await?;
    sqlx::query("VACUUM INTO ?1")
        .bind(sqlite_path_literal(&temporary_path)?)
        .execute(&mut *connection)
        .await?;
    drop(connection);

    sync_file(&temporary_path)?;
    validate_read_only_sqlite(&temporary_path).await?;
    fs::rename(&temporary_path, final_path)?;
    sync_parent_directory(parent)?;

    Ok(VerifiedBackup {
        final_path: final_path.to_path_buf(),
    })
}

pub(crate) async fn validate_read_only_sqlite(path: &Path) -> Result<(), PersistenceError> {
    let mut connection = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .connect()
        .await?;
    let validation = async {
        let row = sqlx::query("PRAGMA quick_check")
            .fetch_one(&mut connection)
            .await?;
        let quick_check: String = row.get(0);
        if quick_check != "ok" {
            return Err(PersistenceError::BackupVerificationFailed);
        }
        let foreign_key_violation = sqlx::query("PRAGMA foreign_key_check")
            .fetch_optional(&mut connection)
            .await?;
        if foreign_key_violation.is_some() {
            return Err(PersistenceError::BackupVerificationFailed);
        }
        Ok(())
    }
    .await;
    let close = connection.close().await;
    match validation {
        Ok(()) => close.map_err(Into::into),
        Err(error) => {
            let _ = close;
            Err(error)
        }
    }
}

fn sqlite_path_literal(path: &Path) -> Result<String, PersistenceError> {
    Ok(path
        .to_str()
        .ok_or(PersistenceError::IoFailed {
            kind: std::io::ErrorKind::InvalidData,
        })?
        .replace('\\', "/"))
}

fn sync_file(path: &Path) -> Result<(), PersistenceError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    Ok(())
}

#[cfg(not(windows))]
fn sync_parent_directory(path: &Path) -> Result<(), PersistenceError> {
    OpenOptions::new().read(true).open(path)?.sync_all()?;
    Ok(())
}

#[cfg(windows)]
fn sync_parent_directory(_path: &Path) -> Result<(), PersistenceError> {
    Ok(())
}
