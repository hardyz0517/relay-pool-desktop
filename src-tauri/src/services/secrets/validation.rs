use std::path::Path;

use crate::persistence::{read_encrypted_secrets, StoredEncryptedSecret};

use super::crypto::{decrypt_secret, EncryptedPayload};

pub fn validate_database_secrets(path: &Path, data_key: &[u8; 32]) -> Result<(), String> {
    let records: Vec<StoredEncryptedSecret> =
        tauri::async_runtime::block_on(read_encrypted_secrets(path))
            .map_err(|error| format!("failed to read database for secret validation: {error}"))?;
    for record in records {
        let payload = EncryptedPayload {
            ciphertext: record.ciphertext,
            nonce: record.nonce,
            aad: record.aad,
            value_hash: record.value_hash,
        };
        decrypt_secret(data_key, &payload).map_err(|_| {
            format!(
                "secret validation failed for row {}",
                sanitized_row_identifier(&record.id)
            )
        })?;
    }
    Ok(())
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
    use crate::persistence::runtime::PersistenceRuntime;
    use crate::services::secrets::crypto::{encrypt_secret, generate_data_key};
    use base64::{engine::general_purpose, Engine as _};
    use sqlx::SqliteConnection;
    use std::{
        fs,
        path::{Path, PathBuf},
        time::SystemTime,
    };

    #[test]
    fn validation_accepts_missing_or_empty_secret_table() {
        let key = generate_data_key();
        let missing_table = db_path("missing-table");
        drop(open(&missing_table));
        validate_database_secrets(&missing_table, &key).expect("missing table ok");

        let empty_table = db_path("empty-table");
        initialize_v2(&empty_table);
        validate_database_secrets(&empty_table, &key).expect("empty table ok");
    }

    #[test]
    fn validation_decrypts_rows_and_rejects_wrong_key_without_leaking_secret_material() {
        let key = generate_data_key();
        let wrong_key = generate_data_key();
        let path = db_path("encrypted-rows");
        initialize_v2(&path);
        let payload = encrypt_secret(&key, "sk-validation-canary", "station_key:key-1:api_key")
            .expect("encrypt");
        let mut connection = open_existing(&path);
        tauri::async_runtime::block_on(
            sqlx::query(
                "INSERT INTO secrets (
                    id, scope, owner_id, kind, masked_value, ciphertext, nonce,
                    created_at, updated_at
                ) VALUES (?1, 'station_key', 'key-1', 'api_key', 'sk-***', ?2, ?3, '1', '1')",
            )
            .bind("secret-row-canary")
            .bind(
                general_purpose::STANDARD
                    .decode(payload.ciphertext)
                    .expect("ciphertext"),
            )
            .bind(
                general_purpose::STANDARD
                    .decode(payload.nonce)
                    .expect("nonce"),
            )
            .execute(&mut connection),
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

    #[test]
    fn validation_remains_compatible_with_released_legacy_secret_rows() {
        let key = generate_data_key();
        let path = db_path("legacy-encrypted-row");
        let mut connection = open(&path);
        crate::services::data_store::test_support::execute_batch(
            &mut connection,
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
        );
        let payload =
            encrypt_secret(&key, "sk-legacy-canary", "station_key:key-1:api_key").expect("encrypt");
        tauri::async_runtime::block_on(
            sqlx::query(
                "INSERT INTO secrets (
                    id, scope, owner_id, kind, ciphertext, nonce, aad, masked_value,
                    value_hash, encryption_version, created_at, updated_at
                ) VALUES (?1, 'station_key', 'key-1', 'api_key', ?2, ?3, ?4, 'sk-***', ?5, 1, '1', '1')",
            )
            .bind("legacy-secret-row")
            .bind(payload.ciphertext)
            .bind(payload.nonce)
            .bind(payload.aad)
            .bind(payload.value_hash)
            .execute(&mut connection),
        )
        .expect("legacy secret row");
        drop(connection);

        validate_database_secrets(&path, &key).expect("legacy secret validates");
    }

    fn initialize_v2(path: &Path) {
        let runtime = tauri::async_runtime::block_on(PersistenceRuntime::initialize_new(path))
            .expect("initialize v2 database");
        tauri::async_runtime::block_on(runtime.close()).expect("close persistence runtime");
    }

    fn open(path: &Path) -> SqliteConnection {
        crate::services::data_store::test_support::open_database(path)
    }

    fn open_existing(path: &Path) -> SqliteConnection {
        crate::services::data_store::test_support::open_existing_database(path)
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
