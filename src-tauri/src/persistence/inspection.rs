use std::{collections::BTreeMap, path::Path};

use base64::{engine::general_purpose, Engine as _};
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row, SqliteConnection};

use crate::persistence::error::PersistenceError;

const RECOVERY_SUMMARY_TABLES: [&str; 4] =
    ["stations", "station_keys", "channel_monitors", "settings"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReadOnlyDatabaseHealth {
    Healthy,
    IntegrityFailed,
    InvalidSqlite,
    Unreadable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReadOnlyDatabaseInspection {
    pub(crate) health: ReadOnlyDatabaseHealth,
    pub(crate) table_counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredEncryptedSecret {
    pub(crate) id: String,
    pub(crate) ciphertext: String,
    pub(crate) nonce: String,
    pub(crate) aad: String,
    pub(crate) value_hash: String,
}

pub(crate) async fn inspect_relay_pool_database(
    path: &Path,
) -> Result<ReadOnlyDatabaseInspection, PersistenceError> {
    let mut connection = match connect_read_only(path).await {
        Ok(connection) => connection,
        Err(error) => {
            return Ok(ReadOnlyDatabaseInspection {
                health: classify_open_error(&error),
                table_counts: BTreeMap::new(),
            });
        }
    };

    let inspection = inspect_open_connection(&mut connection).await;
    let close_result = connection.close().await;
    match inspection {
        Ok(inspection) => {
            close_result?;
            Ok(inspection)
        }
        Err(error) => {
            let _ = close_result;
            Err(error)
        }
    }
}

async fn inspect_open_connection(
    connection: &mut SqliteConnection,
) -> Result<ReadOnlyDatabaseInspection, PersistenceError> {
    let quick_check = match sqlx::query_scalar::<_, String>("PRAGMA quick_check")
        .fetch_one(&mut *connection)
        .await
    {
        Ok(value) => value,
        Err(_) => {
            return Ok(ReadOnlyDatabaseInspection {
                health: ReadOnlyDatabaseHealth::InvalidSqlite,
                table_counts: BTreeMap::new(),
            });
        }
    };
    if !quick_check.eq_ignore_ascii_case("ok") {
        return Ok(ReadOnlyDatabaseInspection {
            health: ReadOnlyDatabaseHealth::IntegrityFailed,
            table_counts: BTreeMap::new(),
        });
    }

    let mut table_counts = BTreeMap::new();
    for table in RECOVERY_SUMMARY_TABLES {
        if table_exists(&mut *connection, table).await? {
            let statement = format!("SELECT COUNT(*) FROM {table}");
            let count = sqlx::query_scalar::<_, i64>(&statement)
                .fetch_one(&mut *connection)
                .await?;
            table_counts.insert(table.to_string(), count);
        }
    }
    Ok(ReadOnlyDatabaseInspection {
        health: ReadOnlyDatabaseHealth::Healthy,
        table_counts,
    })
}

pub(crate) async fn read_encrypted_secrets(
    path: &Path,
) -> Result<Vec<StoredEncryptedSecret>, PersistenceError> {
    let mut connection = connect_read_only(path).await?;
    if !table_exists(&mut connection, "secrets").await? {
        connection.close().await?;
        return Ok(Vec::new());
    }

    let columns = table_columns(&mut connection, "secrets").await?;
    let records = if columns.contains("aad") && columns.contains("value_hash") {
        let rows = sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT id, ciphertext, nonce, aad, value_hash FROM secrets ORDER BY id",
        )
        .fetch_all(&mut connection)
        .await?;
        rows.into_iter()
            .map(
                |(id, ciphertext, nonce, aad, value_hash)| StoredEncryptedSecret {
                    id,
                    ciphertext,
                    nonce,
                    aad,
                    value_hash,
                },
            )
            .collect()
    } else {
        let rows = sqlx::query_as::<_, (String, String, String, String, Vec<u8>, Vec<u8>)>(
            "SELECT id, scope, owner_id, kind, ciphertext, nonce FROM secrets ORDER BY id",
        )
        .fetch_all(&mut connection)
        .await?;
        rows.into_iter()
            .map(
                |(id, scope, owner_id, kind, ciphertext, nonce)| StoredEncryptedSecret {
                    id,
                    ciphertext: general_purpose::STANDARD.encode(ciphertext),
                    nonce: general_purpose::STANDARD.encode(nonce),
                    aad: format!("{scope}:{owner_id}:{kind}"),
                    value_hash: String::new(),
                },
            )
            .collect()
    };
    connection.close().await?;
    Ok(records)
}

async fn connect_read_only(path: &Path) -> Result<SqliteConnection, sqlx::Error> {
    SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .connect()
        .await
}

async fn table_exists(connection: &mut SqliteConnection, table: &str) -> Result<bool, sqlx::Error> {
    sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
    )
    .bind(table)
    .fetch_one(connection)
    .await
    .map(|exists| exists == 1)
}

async fn table_columns(
    connection: &mut SqliteConnection,
    table: &str,
) -> Result<std::collections::BTreeSet<String>, sqlx::Error> {
    let rows = sqlx::query(&format!("PRAGMA table_info({table})"))
        .fetch_all(connection)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| row.get::<String, _>("name"))
        .collect())
}

fn classify_open_error(error: &sqlx::Error) -> ReadOnlyDatabaseHealth {
    if matches!(
        error,
        sqlx::Error::Io(io_error) if io_error.kind() == std::io::ErrorKind::PermissionDenied
    ) {
        return ReadOnlyDatabaseHealth::Unreadable;
    }
    let message = error.to_string().to_ascii_lowercase();
    if message.contains("permission") || message.contains("access") {
        ReadOnlyDatabaseHealth::Unreadable
    } else {
        ReadOnlyDatabaseHealth::InvalidSqlite
    }
}
