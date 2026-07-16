use std::path::Path;

use rusqlite::{Connection, OpenFlags};

use super::crypto::{decrypt_secret, EncryptedPayload};

pub fn validate_database_secrets(path: &Path, data_key: &[u8; 32]) -> Result<(), String> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|error| format!("failed to open database for secret validation: {error}"))?;
    if !table_exists(&connection, "secrets")? {
        return Ok(());
    }

    let mut rows = connection
        .prepare("SELECT id, ciphertext, nonce, aad, value_hash FROM secrets ORDER BY id")
        .map_err(|error| format!("failed to prepare secret validation: {error}"))?;
    let rows = rows
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                EncryptedPayload {
                    ciphertext: row.get(1)?,
                    nonce: row.get(2)?,
                    aad: row.get(3)?,
                    value_hash: row.get(4)?,
                },
            ))
        })
        .map_err(|error| format!("failed to read encrypted secrets: {error}"))?;

    for row in rows {
        let (id, payload) = row.map_err(|error| format!("failed to read secret row: {error}"))?;
        decrypt_secret(data_key, &payload).map_err(|_| {
            format!(
                "secret validation failed for row {}",
                sanitized_row_identifier(&id)
            )
        })?;
    }

    Ok(())
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|exists| exists == 1)
        .map_err(|error| format!("failed to inspect secret schema: {error}"))
}

fn sanitized_row_identifier(id: &str) -> String {
    let suffix: String = id
        .chars()
        .rev()
        .take(6)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    format!("***{suffix}")
}

#[cfg(test)]
mod tests {
    use super::validate_database_secrets;
    use crate::services::secrets::crypto::{encrypt_secret, generate_data_key};
    use rusqlite::{params, Connection};
    use std::{fs, path::PathBuf, time::SystemTime};

    #[test]
    fn validation_accepts_missing_or_empty_secret_table() {
        let key = generate_data_key();
        let missing_table = db_path("missing-table");
        Connection::open(&missing_table).expect("db");
        validate_database_secrets(&missing_table, &key).expect("missing table ok");

        let empty_table = db_path("empty-table");
        let connection = Connection::open(&empty_table).expect("db");
        create_secrets_table(&connection);
        drop(connection);
        validate_database_secrets(&empty_table, &key).expect("empty table ok");
    }

    #[test]
    fn validation_decrypts_rows_and_rejects_wrong_key_without_leaking_secret_material() {
        let key = generate_data_key();
        let wrong_key = generate_data_key();
        let path = db_path("encrypted-rows");
        let connection = Connection::open(&path).expect("db");
        create_secrets_table(&connection);
        let payload =
            encrypt_secret(&key, "sk-validation-canary", "station:key:api_key").expect("encrypt");
        connection
            .execute(
                "INSERT INTO secrets (
                    id, scope, owner_id, kind, ciphertext, nonce, aad, masked_value,
                    value_hash, encryption_version, created_at, updated_at
                ) VALUES (?1, 'station_key', 'key-1', 'api_key', ?2, ?3, ?4, 'sk-***', ?5, 1, '1', '1')",
                params![
                    "secret-row-canary",
                    payload.ciphertext,
                    payload.nonce,
                    payload.aad,
                    payload.value_hash
                ],
            )
            .expect("secret row");
        drop(connection);

        validate_database_secrets(&path, &key).expect("right key");
        let error = validate_database_secrets(&path, &wrong_key).expect_err("wrong key");

        assert!(error.contains("secret validation failed"));
        for leaked in [
            "sk-validation-canary",
            "station:key:api_key",
            "secret-row-canary",
        ] {
            assert!(!error.contains(leaked), "leaked {leaked}");
        }
    }

    fn create_secrets_table(connection: &Connection) {
        connection
            .execute_batch(
                "CREATE TABLE secrets (
                    id TEXT PRIMARY KEY,
                    scope TEXT NOT NULL,
                    owner_id TEXT NOT NULL,
                    kind TEXT NOT NULL,
                    ciphertext TEXT NOT NULL,
                    nonce TEXT NOT NULL,
                    aad TEXT NOT NULL,
                    masked_value TEXT NOT NULL,
                    value_hash TEXT NOT NULL,
                    encryption_version INTEGER NOT NULL,
                    created_at TEXT NOT NULL,
                    updated_at TEXT NOT NULL
                )",
            )
            .expect("secrets table");
    }

    fn db_path(name: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("relay-pool-secret-validation-{name}-{unique}"));
        fs::create_dir_all(&root).expect("root");
        root.join("relay-pool-desktop.sqlite3")
    }
}
