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

#[test]
fn schema15_fixture_upgrades_through_production_startup_route_and_restarts() {
    let manifest = schema15_manifest();
    let fixture_path = manifest_path(&manifest["fixture"]);
    let root = tempfile::tempdir().expect("temporary upgrade root");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&data_dir).expect("data directory");
    let database_path = data_dir.join("relay-pool-desktop-v2.sqlite3");
    fs::copy(&fixture_path, &database_path).expect("copy frozen fixture");
    let source_digest = sha256_file(&fixture_path);
    assert_eq!(
        sha256_file(&database_path),
        source_digest,
        "upgrade must start from an exact frozen fixture copy"
    );

    let first = relay_pool_desktop_lib::test_support::schema_upgrade::run_schema_upgrade(
        &data_dir,
        &database_path,
        "schema15-fixture-device-key",
        [0x07; 32],
    )
    .expect("schema 15 production startup upgrade");
    assert_eq!(first.schema_version, 74);
    assert_eq!(first.open_mode, "writable");
    assert!(first.plan_step_count > 0);
    assert!(first.restart_ready);

    let source_after = sha256_file(&fixture_path);
    assert_eq!(
        source_after, source_digest,
        "frozen fixture must remain unchanged"
    );
    assert_ne!(
        sha256_file(&database_path),
        source_digest,
        "upgrade must mutate only the copied database"
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1"
        ),
        74
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT MAX(version) FROM _sqlx_migrations WHERE success = 1"
        ),
        74
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM request_logs WHERE upstream_base_url IS NOT NULL"
        ),
        0
    );
    assert_eq!(query_string(&database_path, "SELECT status FROM request_log_url_sanitizer_progress WHERE id = 'request_logs_upstream_base_url_v1'"), "complete");
    assert_eq!(
        query_string(
            &database_path,
            "SELECT value FROM settings WHERE key = '__secret_format_version'"
        ),
        "1"
    );
    assert_eq!(
        query_string(
            &database_path,
            "SELECT value FROM settings WHERE key = '__active_key_id'"
        ),
        "schema15-fixture-device-key"
    );
    // The baseline conversion must move every legacy plaintext credential into
    // an encrypted row and leave no plaintext copies behind.  The frozen
    // manifest records the expected cardinality so this assertion also catches
    // accidental duplicate conversion rows.
    let expected = &manifest["expected_after_upgrade"];
    assert_eq!(
        query_i64(&database_path, "SELECT COUNT(*) FROM secrets"),
        expected["secret_count"].as_i64().expect("secret_count"),
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM secrets WHERE key_id IS NULL OR encryption_version IS NULL OR encryption_version <> 1",
        ),
        0,
        "all upgraded secrets must carry current encryption metadata",
    );
    assert_eq!(
        query_string(
            &database_path,
            "SELECT value FROM settings WHERE key = 'local_key'"
        ),
        expected["settings_local_key_value"]
            .as_str()
            .expect("settings_local_key_value"),
    );
    assert_eq!(
        query_string(
            &database_path,
            "SELECT api_key FROM stations WHERE id = 'fixture-station-001'",
        ),
        expected["legacy_station_api_key_value"]
            .as_str()
            .expect("legacy_station_api_key_value"),
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM stations WHERE TRIM(COALESCE(api_key, '')) <> ''",
        ),
        0,
        "station plaintext API keys must be cleared after conversion",
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM stations WHERE id = 'fixture-station-001' AND api_key_secret_id IS NOT NULL",
        ),
        1,
        "station API key must be referenced by an encrypted secret",
    );
    assert_eq!(
        query_string(
            &database_path,
            "SELECT api_key FROM station_keys WHERE id = 'fixture-station-key-001'",
        ),
        expected["legacy_station_key_api_key_value"]
            .as_str()
            .expect("legacy_station_key_api_key_value"),
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM station_keys WHERE TRIM(COALESCE(api_key, '')) <> ''",
        ),
        0,
        "station-key plaintext API keys must be cleared after conversion",
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM station_keys WHERE id = 'fixture-station-key-001' AND api_key_secret_id IS NOT NULL",
        ),
        1,
        "station-key API key must be referenced by an encrypted secret",
    );
    assert_eq!(
        query_string(
            &database_path,
            "SELECT login_password FROM station_credentials WHERE station_id = 'fixture-station-001'",
        ),
        expected["legacy_login_password_value"]
            .as_str()
            .expect("legacy_login_password_value"),
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM station_credentials WHERE TRIM(COALESCE(login_password, '')) <> ''",
        ),
        0,
        "legacy login passwords must be cleared after conversion",
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM station_credentials WHERE station_id = 'fixture-station-001' AND login_password_secret_id IS NOT NULL",
        ),
        1,
        "login password must be referenced by an encrypted secret",
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM app_secret_bindings WHERE binding_scope = 'settings' AND binding_owner_id = 'local_key' AND binding_kind = 'local_access_key'",
        ),
        expected["local_access_key_binding_count"]
            .as_i64()
            .expect("local_access_key_binding_count"),
    );
    assert_eq!(query_string(&database_path, "PRAGMA quick_check"), "ok");
    assert!(query_string(&database_path, "PRAGMA foreign_key_check").is_empty());
    let staged_policy_count = query_i64(
        &database_path,
        "SELECT COUNT(*) FROM routing_policy_v3_staged WHERE scope = 'active' AND status = 'staged'",
    );
    assert!(
        staged_policy_count >= 1,
        "routing policy v3 staging must materialize the active policy"
    );
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM routing_policy_v3_migration_audit WHERE scope = 'active' AND migration_status = 'staged'",
        ),
        staged_policy_count,
        "every staged policy must have an append-only migration audit row",
    );
    let backup_entries = fs::read_dir(data_dir.join("backups"))
        .expect("read upgrade backup directory")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.starts_with("relay-pool-v2-schema-") && name.ends_with(".sqlite3")
        })
        .collect::<Vec<_>>();
    let backup_count = backup_entries.len();
    assert!(
        backup_count >= 1,
        "schema15 upgrade must retain a verified backup"
    );
    for entry in backup_entries {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        assert!(
            !Path::new(&format!("{}-wal", path.display())).exists()
                && !Path::new(&format!("{}-shm", path.display())).exists(),
            "verified backup must not leave SQLite sidecars: {name}"
        );
        assert_eq!(
            query_string(&path, "PRAGMA quick_check"),
            "ok",
            "verified backup must pass SQLite quick_check: {name}"
        );
        let manifest_path = path.with_file_name(format!("{name}.backup-manifest.json"));
        assert!(
            manifest_path.is_file(),
            "verified backup must have a durable identity manifest: {name}"
        );
        let backup_manifest: serde_json::Value =
            serde_json::from_slice(&fs::read(&manifest_path).expect("backup manifest bytes"))
                .expect("valid backup manifest");
        assert_eq!(
            backup_manifest["manifestVersion"].as_i64(),
            Some(1),
            "backup manifest version must be supported"
        );
        assert_eq!(
            backup_manifest["backupFileName"].as_str(),
            Some(name.as_str()),
            "backup manifest must bind to its sibling file"
        );
        let source_schema = backup_manifest["sourceSchema"]
            .as_i64()
            .expect("backup manifest sourceSchema");
        let target_schema = backup_manifest["targetSchema"]
            .as_i64()
            .expect("backup manifest targetSchema");
        assert!(
            source_schema < target_schema,
            "backup manifest must describe a forward schema transition"
        );
    }

    let second = relay_pool_desktop_lib::test_support::schema_upgrade::run_schema_upgrade(
        &data_dir,
        &database_path,
        "schema15-fixture-device-key",
        [0x07; 32],
    )
    .expect("second startup must be idempotent and writable");
    assert_eq!(second.schema_version, 74);
    assert_eq!(second.open_mode, "writable");
    assert!(second.restart_ready);
    assert_eq!(
        query_i64(&database_path, "SELECT COUNT(*) FROM secrets"),
        expected["secret_count"].as_i64().expect("secret_count"),
        "idempotent restart must not duplicate converted secrets",
    );
    assert_eq!(
        query_i64(&database_path, "SELECT COUNT(*) FROM app_secret_bindings"),
        expected["local_access_key_binding_count"]
            .as_i64()
            .expect("local_access_key_binding_count"),
    );
    let backup_count_after_restart = fs::read_dir(data_dir.join("backups"))
        .expect("read backups after restart")
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.starts_with("relay-pool-v2-schema-") && name.ends_with(".sqlite3")
        })
        .count();
    assert_eq!(
        backup_count_after_restart, backup_count,
        "idempotent restart must not create an unnecessary schema backup"
    );
    assert_eq!(
        query_string(
            &database_path,
            "SELECT status FROM request_log_url_sanitizer_progress WHERE id = 'request_logs_upstream_base_url_v1'",
        ),
        "complete",
    );
    assert!(!data_dir.join("persistence-upgrade-journal.json").exists());
}

fn query_i64(path: &Path, sql: &str) -> i64 {
    tauri::async_runtime::block_on(async {
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .read_only(true),
        )
        .await
        .expect("query connection");
        let value = sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&mut connection)
            .await
            .expect("scalar query");
        connection.close().await.expect("close query connection");
        value
    })
}

fn query_string(path: &Path, sql: &str) -> String {
    tauri::async_runtime::block_on(async {
        let mut connection = SqliteConnection::connect_with(
            &SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .read_only(true),
        )
        .await
        .expect("query connection");
        let value = sqlx::query_scalar::<_, String>(sql)
            .fetch_optional(&mut connection)
            .await
            .expect("scalar query")
            .unwrap_or_default();
        connection.close().await.expect("close query connection");
        value
    })
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
