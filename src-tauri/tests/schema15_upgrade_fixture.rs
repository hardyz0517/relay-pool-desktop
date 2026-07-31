use std::{fs, path::Path};

use sha2::{Digest, Sha256, Sha384};
use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection};

#[test]
fn schema15_fixture_manifest_matches_frozen_database_and_migrations() {
    let manifest = schema15_manifest();
    let fixture_path = manifest_path(&manifest["fixture"]);

    assert!(
        fixture_path.is_file(),
        "schema15 fixture database must be committed"
    );
    assert_eq!(
        sha256_file(&fixture_path),
        manifest["fixture_sha256"]
            .as_str()
            .expect("fixture_sha256 string")
    );
    for suffix in ["-wal", "-shm"] {
        assert!(
            !Path::new(&format!("{}{}", fixture_path.display(), suffix)).exists(),
            "schema15 fixture must not commit SQLite sidecar {suffix}"
        );
    }

    let migrations = manifest["migration_contract"]["migrations"]
        .as_array()
        .expect("migration list");
    assert_eq!(migrations.len(), 15);
    for (index, migration) in migrations.iter().enumerate() {
        let version = migration["version"].as_i64().expect("migration version");
        assert_eq!(version, i64::try_from(index + 1).expect("version index"));
        let path = manifest_path(&migration["path"]);
        assert_eq!(
            sha256_file(&path),
            migration["sha256"].as_str().expect("migration sha256"),
            "released schema15 migration file drifted: {}",
            path.display()
        );
    }
}

#[tokio::test]
async fn schema15_fixture_database_is_a_released_baseline_not_dynamic_latest() {
    let manifest = schema15_manifest();
    let fixture_path = manifest_path(&manifest["fixture"]);
    let mut connection = read_only_connection(&fixture_path).await;

    let compatibility_schema: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("compatibility schema");
    assert_eq!(compatibility_schema, 15);

    let migration_versions = sqlx::query("SELECT version FROM _sqlx_migrations ORDER BY version")
        .fetch_all(&mut connection)
        .await
        .expect("migration ledger")
        .into_iter()
        .map(|row| row.get::<i64, _>("version"))
        .collect::<Vec<_>>();
    assert_eq!(migration_versions, (1_i64..=15).collect::<Vec<_>>());

    let migrations = manifest["migration_contract"]["migrations"]
        .as_array()
        .expect("migration list");
    for migration in migrations {
        let version = migration["version"].as_i64().expect("migration version");
        let path = manifest_path(&migration["path"]);
        let expected_checksum = sha384_file(&path);
        let actual_checksum: Vec<u8> =
            sqlx::query_scalar("SELECT checksum FROM _sqlx_migrations WHERE version = ?1")
                .bind(version)
                .fetch_one(&mut connection)
                .await
                .expect("migration checksum");
        assert_eq!(
            hex_lower(&actual_checksum),
            expected_checksum,
            "schema15 SQLx ledger checksum must match raw migration bytes: {}",
            path.display()
        );
    }

    let legacy_secret_columns: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM pragma_table_info('secrets')
        WHERE name IN ('key_id', 'encryption_version', 'value_hash')
        "#,
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy secret columns");
    assert_eq!(
        legacy_secret_columns, 0,
        "schema15 fixture must stay pre encrypted-secret baseline"
    );

    let legacy_local_key: String =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'local_key'")
            .fetch_one(&mut connection)
            .await
            .expect("legacy local key");
    assert_eq!(legacy_local_key, "schema15-fixture-local-key");

    let encrypted_legacy_secret_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM secrets
        WHERE scope = 'station_credentials'
          AND owner_id = 'fixture-station-001'
          AND kind = 'cookie'
        "#,
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy encrypted secret");
    assert_eq!(encrypted_legacy_secret_count, 1);

    connection.close().await.expect("close fixture connection");
}

fn schema15_manifest() -> serde_json::Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/persistence/schema15/manifest.json");
    serde_json::from_slice(&fs::read(path).expect("schema15 fixture manifest"))
        .expect("valid schema15 fixture manifest")
}

fn manifest_path(value: &serde_json::Value) -> std::path::PathBuf {
    let relative = value.as_str().expect("manifest path string");
    assert!(
        !relative.contains("..") && !Path::new(relative).is_absolute(),
        "manifest path must stay repository-relative"
    );
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("repo root")
        .join(relative)
}

fn sha256_file(path: &Path) -> String {
    Sha256::digest(fs::read(path).expect("file bytes"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn sha384_file(path: &Path) -> String {
    Sha384::digest(fs::read(path).expect("file bytes"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

async fn read_only_connection(path: &Path) -> SqliteConnection {
    SqliteConnection::connect_with(
        &SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .read_only(true),
    )
    .await
    .expect("read-only fixture connection")
}
