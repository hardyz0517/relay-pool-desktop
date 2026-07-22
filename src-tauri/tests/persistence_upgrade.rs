mod persistence {
    pub(crate) mod error {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/error.rs"
        ));
    }
    pub(crate) mod write_coordinator {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/write_coordinator.rs"
        ));
    }
    pub(crate) mod write_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/write_session.rs"
        ));
    }
    pub(crate) mod settings_compat {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/settings_compat.rs"
        ));
    }
    pub(crate) mod read_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/read_session.rs"
        ));
    }
    pub(crate) mod backup {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/backup.rs"
        ));
    }
    pub(crate) mod schema_compatibility {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/schema_compatibility.rs"
        ));
    }
    pub(crate) mod health_check {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/health_check.rs"
        ));
    }
    pub(crate) mod migrations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/migrations.rs"
        ));
    }
    pub(crate) mod runtime_lifecycle {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime_lifecycle.rs"
        ));
    }
    pub(crate) mod runtime {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime.rs"
        ));
    }
    pub(crate) use crate::legacy_import;
}

#[path = "../src/persistence/legacy_import/mod.rs"]
mod legacy_import;

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use legacy_import::{
    detect_profile, import_profile, source_candidate_identity, validate_import,
    ExpectedImportManifest, UpgradeError,
};
use persistence::{
    migrations::migrator, runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility,
};
use semver::Version;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteConnectOptions, Connection, Executor, Row, SqliteConnection};

#[tokio::test]
async fn every_released_profile_imports_to_the_expected_manifest() {
    let fixtures = released_fixtures();
    let expected_profiles = released_profile_ids_from_manifest();
    let actual_profiles = fixtures
        .iter()
        .map(|fixture| fixture.expected.profile.clone())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        actual_profiles, expected_profiles,
        "fixture directories must exactly match the released-schema manifest"
    );
    assert!(
        !fixtures.is_empty(),
        "released fixtures are a required upgrade gate"
    );
    for fixture in fixtures {
        let before = file_evidence(&fixture.source);
        assert_eq!(
            raw_schema_hash(&fixture.source).await,
            fixture.expected.raw_schema_hash,
            "raw fixture schema provenance drifted"
        );
        assert_eq!(
            sha256_file(&fixture.source),
            fixture.expected.fixture_sha256,
            "fixture bytes drifted"
        );
        let profile = detect_profile(&fixture.source)
            .await
            .expect("known profile");
        assert_eq!(profile.id(), fixture.expected.profile);
        assert_eq!(
            profile.base_schema_hash(),
            fixture.expected.semantic_base_schema_hash
        );
        assert_eq!(
            profile.request_lifecycle_schema_hash(),
            fixture.expected.request_lifecycle_schema_hash.as_deref()
        );
        assert_eq!(
            source_candidate_identity(&fixture.source)
                .expect("source identity")
                .len(),
            64
        );
        let target_path = temp_db_path(profile.id());
        create_v2_target(&target_path).await;
        let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
            .await
            .expect("V2 runtime");

        import_profile(&profile, &fixture.source, &runtime.handle())
            .await
            .expect("released fixture import");
        validate_import(&runtime.handle(), &fixture.expected)
            .await
            .expect("import manifest");
        assert_eq!(runtime.compatibility_decision_code(), "writable");
        assert_eq!(
            runtime.health().await.expect("runtime health").open_mode,
            "writable"
        );
        let mut read = runtime.begin_read().await.expect("import read session");
        let imported_station_count = sqlx::query("SELECT COUNT(*) AS count FROM stations")
            .fetch_one(read.connection())
            .await
            .expect("imported station count")
            .get::<i64, _>("count");
        assert_eq!(
            imported_station_count,
            fixture.expected.table_counts["stations"]
        );
        drop(read);

        let backup_path = target_path.with_extension("verified-backup.sqlite3");
        persistence::backup::create_verified_backup_from_path(&target_path, &backup_path)
            .await
            .expect("verified V2 backup");
        remove_sqlite_set(&backup_path);
        assert_eq!(
            file_evidence(&fixture.source),
            before,
            "source fixture changed during import"
        );
        drop(runtime);
        remove_sqlite_set(&target_path);
    }
}

#[tokio::test]
async fn semantically_equivalent_legacy_ddl_is_accepted() {
    let path = copy_released_fixture("semantic-legacy-schema");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("semantic fixture");
    connection
        .execute("PRAGMA writable_schema = ON")
        .await
        .expect("enable writable schema");
    let changed = sqlx::query(
        r#"
        UPDATE sqlite_schema
        SET sql = REPLACE(sql, 'CREATE TABLE stations (', 'CREATE TABLE stations (   ')
        WHERE type = 'table' AND name = 'stations'
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("rewrite equivalent DDL")
    .rows_affected();
    assert_eq!(changed, 1);
    connection
        .execute("PRAGMA writable_schema = OFF")
        .await
        .expect("disable writable schema");
    connection.close().await.expect("close semantic fixture");

    let profile = detect_profile(&path)
        .await
        .expect("semantic schema profile");
    assert_eq!(profile.id(), "profile_001");
    remove_sqlite_set(&path);
}

#[tokio::test]
async fn case_only_identifier_changes_are_imported_without_data_loss() {
    let path = copy_released_fixture("case-insensitive-legacy-schema");
    rewrite_stations_id_declaration(&path, "ID TEXT PRIMARY KEY").await;
    let before = file_evidence(&path);
    let expected = released_fixtures()
        .into_iter()
        .next()
        .expect("released fixture")
        .expected;

    let profile = detect_profile(&path)
        .await
        .expect("case-equivalent schema profile");
    let target_path = temp_db_path("case-insensitive-import-target");
    create_v2_target(&target_path).await;
    let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
        .await
        .expect("V2 runtime");
    import_profile(&profile, &path, &runtime.handle())
        .await
        .expect("case-equivalent fixture import");
    validate_import(&runtime.handle(), &expected)
        .await
        .expect("complete imported manifest");

    drop(runtime);
    assert_eq!(file_evidence(&path), before, "source fixture changed");
    remove_sqlite_set(&path);
    remove_sqlite_set(&target_path);
}

#[tokio::test]
async fn quoted_identifier_whitespace_is_not_treated_as_equivalent() {
    let path = copy_released_fixture("whitespace-identifier-schema");
    rewrite_stations_id_declaration(&path, "\" id \" TEXT PRIMARY KEY").await;
    let before = file_evidence(&path);

    assert!(matches!(
        detect_profile(&path).await,
        Err(UpgradeError::UnsupportedLegacySchema)
    ));
    assert_eq!(file_evidence(&path), before, "source fixture changed");
    remove_sqlite_set(&path);
}

#[tokio::test]
async fn legacy_setting_aliases_are_canonicalized_during_import() {
    let source_path = copy_released_fixture("legacy-setting-alias");
    let options = SqliteConnectOptions::new()
        .filename(&source_path)
        .create_if_missing(false);
    let mut source = SqliteConnection::connect_with(&options)
        .await
        .expect("legacy settings fixture");
    sqlx::query(
        r#"
        INSERT OR REPLACE INTO settings (key, value, updated_at)
        VALUES ('tray_behavior', 'minimize-to-tray', 'legacy')
        "#,
    )
    .execute(&mut source)
    .await
    .expect("seed legacy tray behavior");
    source.close().await.expect("close legacy fixture");

    let profile = detect_profile(&source_path)
        .await
        .expect("released profile");
    let target_path = temp_db_path("legacy-setting-alias-target");
    create_v2_target(&target_path).await;
    let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
        .await
        .expect("V2 runtime");
    import_profile(&profile, &source_path, &runtime.handle())
        .await
        .expect("legacy settings import");

    let mut read = runtime.begin_read().await.expect("import read session");
    let imported: String =
        sqlx::query_scalar("SELECT value FROM settings WHERE key = 'tray_behavior'")
            .fetch_one(read.connection())
            .await
            .expect("imported tray behavior");
    assert_eq!(imported, "minimize_to_tray");
    drop(read);
    drop(runtime);
    remove_sqlite_set(&source_path);
    remove_sqlite_set(&target_path);
}

#[tokio::test]
async fn request_lifecycle_capability_imports_attempt_history() {
    let path = copy_released_fixture("request-lifecycle-capability");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("request lifecycle fixture");
    for (column, column_type) in [
        ("endpoint", "TEXT"),
        ("terminal_kind", "TEXT"),
        ("terminal_code", "TEXT"),
        ("terminal_detail", "TEXT"),
        ("protocol_completed", "INTEGER"),
        ("delivery_terminal", "TEXT"),
        ("selected_attempt_ordinal", "INTEGER"),
        ("terminal_at_ms", "INTEGER"),
    ] {
        let sql = format!("ALTER TABLE request_logs ADD COLUMN {column} {column_type}");
        sqlx::query(&sql)
            .execute(&mut connection)
            .await
            .expect("add request lifecycle column");
    }
    connection
        .execute(
            r#"
            CREATE TABLE request_attempts (
                request_id TEXT NOT NULL,
                ordinal INTEGER NOT NULL,
                station_id TEXT NOT NULL,
                station_key_id TEXT NOT NULL,
                endpoint_revision INTEGER NOT NULL,
                started_at_ms INTEGER NOT NULL,
                terminal_kind TEXT NOT NULL,
                failure_kind TEXT,
                failure_blame TEXT,
                retry_disposition TEXT,
                health_effect TEXT NOT NULL,
                health_cooldown_until_ms INTEGER,
                public_code TEXT,
                sanitized_detail TEXT,
                output_committed INTEGER NOT NULL,
                terminal_at_ms INTEGER NOT NULL,
                PRIMARY KEY (request_id, ordinal),
                FOREIGN KEY(request_id) REFERENCES request_logs(id) ON DELETE CASCADE
            );
            CREATE INDEX idx_request_attempts_station_key_terminal
                ON request_attempts(station_key_id, terminal_at_ms DESC);
            "#,
        )
        .await
        .expect("request attempts schema");
    sqlx::query(
        r#"
        INSERT INTO request_logs (
            id, request_id, started_at, finished_at, method, path, endpoint, stream,
            status, station_key_id, station_id, fallback_count, terminal_kind,
            protocol_completed, selected_attempt_ordinal, terminal_at_ms, created_at
        ) VALUES (
            'legacy-request-001', 'upstream-request-001', '1000', '1100', 'POST',
            '/v1/chat/completions', '/v1/chat/completions', 1, 'completed',
            'fixture-station-key-001', 'fixture-station-001', 0, 'success', 1, 0,
            1100, '1000'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("request log fixture");
    sqlx::query(
        r#"
        INSERT INTO request_attempts (
            request_id, ordinal, station_id, station_key_id, endpoint_revision,
            started_at_ms, terminal_kind, failure_kind, failure_blame,
            retry_disposition, health_effect, health_cooldown_until_ms, public_code,
            sanitized_detail, output_committed, terminal_at_ms
        ) VALUES (
            'legacy-request-001', 0, 'fixture-station-001', 'fixture-station-key-001',
            1, 1000, 'upstream_error', 'upstream', 'upstream', 'retryable',
            'failure', 1200, 'fixture_public_code', 'sanitized fixture detail', 1, 1100
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("request attempt fixture");
    connection.close().await.expect("close capability fixture");
    let before = file_evidence(&path);

    let profile = detect_profile(&path)
        .await
        .expect("request lifecycle profile");
    let target_path = temp_db_path("request-lifecycle-target");
    create_v2_target(&target_path).await;
    let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
        .await
        .expect("V2 runtime");
    import_profile(&profile, &path, &runtime.handle())
        .await
        .expect("request lifecycle import");
    let mut read = runtime.begin_read().await.expect("request lifecycle read");
    let imported = sqlx::query(
        r#"
        SELECT request_id, ordinal, station_id, station_key_id, endpoint_revision,
               started_at_ms, terminal_kind, failure_kind, failure_blame,
               retry_disposition, health_effect, health_cooldown_until_ms, public_code,
               sanitized_detail, output_committed, terminal_at_ms
        FROM request_attempts
        "#,
    )
    .fetch_one(read.connection())
    .await
    .expect("imported request attempt");
    assert_eq!(
        imported.get::<String, _>("request_id"),
        "legacy-request-001"
    );
    assert_eq!(imported.get::<i64, _>("ordinal"), 0);
    assert_eq!(
        imported.get::<String, _>("station_id"),
        "fixture-station-001"
    );
    assert_eq!(
        imported.get::<String, _>("station_key_id"),
        "fixture-station-key-001"
    );
    assert_eq!(imported.get::<i64, _>("endpoint_revision"), 1);
    assert_eq!(imported.get::<i64, _>("started_at_ms"), 1000);
    assert_eq!(imported.get::<String, _>("terminal_kind"), "upstream_error");
    assert_eq!(imported.get::<String, _>("failure_kind"), "upstream");
    assert_eq!(imported.get::<String, _>("failure_blame"), "upstream");
    assert_eq!(imported.get::<String, _>("retry_disposition"), "retryable");
    assert_eq!(imported.get::<String, _>("health_effect"), "failure");
    assert_eq!(imported.get::<i64, _>("health_cooldown_until_ms"), 1200);
    assert_eq!(
        imported.get::<String, _>("public_code"),
        "fixture_public_code"
    );
    assert_eq!(
        imported.get::<String, _>("sanitized_detail"),
        "sanitized fixture detail"
    );
    assert_eq!(imported.get::<i64, _>("output_committed"), 1);
    assert_eq!(imported.get::<i64, _>("terminal_at_ms"), 1100);
    let imported_request_id: String =
        sqlx::query_scalar("SELECT request_id FROM request_logs WHERE id = 'legacy-request-001'")
            .fetch_one(read.connection())
            .await
            .expect("preserved upstream request id");
    assert_eq!(imported_request_id, "upstream-request-001");
    drop(read);
    drop(runtime);
    assert_eq!(file_evidence(&path), before, "capability source changed");
    remove_sqlite_set(&path);
    remove_sqlite_set(&target_path);
}

#[tokio::test]
async fn partial_request_lifecycle_capability_is_rejected() {
    let path = copy_released_fixture("partial-request-lifecycle-capability");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("partial capability fixture");
    connection
        .execute("ALTER TABLE request_logs ADD COLUMN endpoint TEXT")
        .await
        .expect("partial capability marker");
    connection.close().await.expect("close partial fixture");
    let before = file_evidence(&path);

    assert!(matches!(
        detect_profile(&path).await,
        Err(UpgradeError::UnsupportedLegacySchema)
    ));
    assert_eq!(file_evidence(&path), before);
    remove_sqlite_set(&path);
}

#[tokio::test]
async fn orphaned_legacy_secrets_are_not_carried_into_v2() {
    let path = copy_released_fixture("orphaned-legacy-secret");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("orphan secret fixture");
    sqlx::query(
        r#"
        INSERT INTO secrets (
            id, scope, owner_id, kind, ciphertext, nonce, aad, masked_value,
            value_hash, encryption_version, created_at, updated_at
        ) VALUES (
            'orphan-secret-001', 'station_key', 'deleted-key-001', 'api_key',
            'synthetic-ciphertext', 'synthetic-nonce',
            'station_key:deleted-key-001:api_key', '****', 'synthetic-hash', 1,
            '1000', '1000'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("orphan secret row");
    connection.close().await.expect("close orphan fixture");

    let profile = detect_profile(&path).await.expect("known schema");
    let target_path = temp_db_path("orphaned-legacy-secret-target");
    create_v2_target(&target_path).await;
    let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
        .await
        .expect("V2 runtime");
    import_profile(&profile, &path, &runtime.handle())
        .await
        .expect("orphan secret import");
    let mut read = runtime.begin_read().await.expect("secret count read");
    let secret_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM secrets")
        .fetch_one(read.connection())
        .await
        .expect("secret count");
    assert_eq!(secret_count, 0);
    drop(read);
    drop(runtime);
    remove_sqlite_set(&path);
    remove_sqlite_set(&target_path);
}

#[tokio::test]
async fn request_logs_with_deleted_keys_keep_history_with_a_null_reference() {
    let path = copy_released_fixture("request-log-deleted-key");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("request log fixture");
    sqlx::query(
        r#"
        INSERT INTO request_logs (
            id, started_at, method, path, stream, status, station_key_id, station_id,
            fallback_count, created_at
        ) VALUES (
            'legacy-log-deleted-key', '1000', 'POST', '/v1/chat/completions', 1,
            'success', 'deleted-key-001', 'fixture-station-001', 0, '1000'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("request log row");
    connection.close().await.expect("close request log fixture");

    let profile = detect_profile(&path).await.expect("known schema");
    let target_path = temp_db_path("request-log-deleted-key-target");
    create_v2_target(&target_path).await;
    let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
        .await
        .expect("V2 runtime");
    import_profile(&profile, &path, &runtime.handle())
        .await
        .expect("request log import");
    let mut read = runtime.begin_read().await.expect("request log read");
    let imported = sqlx::query(
        "SELECT station_key_id, station_id FROM request_logs WHERE id = 'legacy-log-deleted-key'",
    )
    .fetch_one(read.connection())
    .await
    .expect("imported request log");
    assert_eq!(imported.get::<Option<String>, _>("station_key_id"), None);
    assert_eq!(
        imported.get::<String, _>("station_id"),
        "fixture-station-001"
    );
    drop(read);
    drop(runtime);
    remove_sqlite_set(&path);
    remove_sqlite_set(&target_path);
}

#[tokio::test]
async fn change_events_with_deleted_owners_keep_history_with_null_references() {
    let path = copy_released_fixture("change-event-deleted-owner");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("change event fixture");
    sqlx::query(
        r#"
        INSERT INTO change_events (
            id, severity, event_type, status, title, message, object_type, object_id,
            station_id, station_key_id, dedupe_key, source, detected_at, created_at, updated_at
        ) VALUES (
            'legacy-change-deleted-owner', 'warning', 'fixture', 'unread', 'Fixture',
            'Synthetic fixture event', 'station', 'deleted-station-001',
            'deleted-station-001', 'deleted-key-001', 'fixture-deleted-owner', 'fixture',
            '1000', '1000', '1000'
        )
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("change event row");
    connection
        .close()
        .await
        .expect("close change event fixture");

    let profile = detect_profile(&path).await.expect("known schema");
    let target_path = temp_db_path("change-event-deleted-owner-target");
    create_v2_target(&target_path).await;
    let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_8())
        .await
        .expect("V2 runtime");
    import_profile(&profile, &path, &runtime.handle())
        .await
        .expect("change event import");
    let mut read = runtime.begin_read().await.expect("change event read");
    let imported = sqlx::query(
        "SELECT station_id, station_key_id FROM change_events WHERE id = 'legacy-change-deleted-owner'",
    )
    .fetch_one(read.connection())
    .await
    .expect("imported change event");
    assert_eq!(imported.get::<Option<String>, _>("station_id"), None);
    assert_eq!(imported.get::<Option<String>, _>("station_key_id"), None);
    drop(read);
    drop(runtime);
    remove_sqlite_set(&path);
    remove_sqlite_set(&target_path);
}

#[test]
fn legacy_secret_transform_contract_keeps_material_and_ciphertext_typed() {
    use legacy_import::{
        ImportPhase, ImportedEncryptedSecret, LegacySecretBytes, LegacySecretMaterial,
        LegacySecretTransformer,
    };

    struct RejectingTransformer;

    impl LegacySecretTransformer for RejectingTransformer {
        fn transform(
            &self,
            profile_id: &str,
            material: LegacySecretMaterial,
        ) -> Result<ImportedEncryptedSecret, UpgradeError> {
            assert_eq!(profile_id, "v0.3.1");
            match material {
                LegacySecretMaterial::Plaintext {
                    scope,
                    owner_id,
                    kind,
                    value,
                } => {
                    assert_eq!(
                        (scope.as_str(), owner_id.as_str(), kind.as_str()),
                        ("station", "s1", "api_key")
                    );
                    assert_eq!(value.as_bytes(), b"secret");
                }
                LegacySecretMaterial::EncryptedV1 { .. } => panic!("unexpected encrypted material"),
            }
            Err(UpgradeError::SecretTransformationFailed)
        }
    }

    let material = LegacySecretMaterial::Plaintext {
        scope: "station".to_string(),
        owner_id: "s1".to_string(),
        kind: "api_key".to_string(),
        value: LegacySecretBytes::new(b"secret".to_vec()),
    };
    assert!(matches!(
        RejectingTransformer.transform("v0.3.1", material),
        Err(UpgradeError::SecretTransformationFailed)
    ));

    let encrypted = LegacySecretMaterial::EncryptedV1 {
        scope: "station".to_string(),
        owner_id: "s1".to_string(),
        kind: "session".to_string(),
        ciphertext: LegacySecretBytes::new(vec![1, 2]),
        nonce: LegacySecretBytes::new(vec![3, 4]),
        aad: "aad".to_string(),
    };
    let LegacySecretMaterial::EncryptedV1 {
        scope,
        owner_id,
        kind,
        ciphertext,
        nonce,
        aad,
    } = encrypted
    else {
        unreachable!()
    };
    assert_eq!(
        (
            scope,
            owner_id,
            kind,
            ciphertext.as_bytes().to_vec(),
            nonce.as_bytes().to_vec(),
            aad,
        ),
        (
            "station".to_string(),
            "s1".to_string(),
            "session".to_string(),
            vec![1, 2],
            vec![3, 4],
            "aad".to_string(),
        )
    );

    let imported = ImportedEncryptedSecret {
        id: "secret-1".to_string(),
        scope: "station".to_string(),
        owner_id: "s1".to_string(),
        kind: "api_key".to_string(),
        masked_value: "sk-***".to_string(),
        ciphertext: vec![1],
        nonce: vec![2],
        created_at: "2026-07-22T00:00:00Z".to_string(),
        updated_at: "2026-07-22T00:00:00Z".to_string(),
    };
    assert_eq!(
        (
            imported.id.as_str(),
            imported.scope.as_str(),
            imported.owner_id.as_str(),
            imported.kind.as_str(),
            imported.masked_value.as_str(),
            imported.ciphertext.as_slice(),
            imported.nonce.as_slice(),
            imported.created_at.as_str(),
            imported.updated_at.as_str(),
        ),
        (
            "secret-1",
            "station",
            "s1",
            "api_key",
            "sk-***",
            [1].as_slice(),
            [2].as_slice(),
            "2026-07-22T00:00:00Z",
            "2026-07-22T00:00:00Z",
        )
    );
    assert_eq!(ImportPhase::Pricing, ImportPhase::Pricing);
    let _ = legacy_import::import_profile_with_secrets_and_phase_hook;
}

#[tokio::test]
async fn unknown_future_schema_fails_without_touching_source() {
    let path = temp_db_path("unknown-legacy-schema");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("unknown fixture");
    sqlx::query("CREATE TABLE future_only(id TEXT PRIMARY KEY, payload BLOB NOT NULL)")
        .execute(&mut connection)
        .await
        .expect("future schema");
    connection.close().await.expect("close fixture");
    let before = file_evidence(&path);

    assert!(matches!(
        detect_profile(&path).await,
        Err(UpgradeError::UnsupportedLegacySchema)
    ));
    assert_eq!(file_evidence(&path), before);
    remove_sqlite_set(&path);
}

#[derive(Debug)]
struct ReleasedFixture {
    source: PathBuf,
    expected: ExpectedImportManifest,
}

fn released_fixtures() -> Vec<ReleasedFixture> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/persistence_upgrade/fixtures");
    let mut fixture_dirs = fs::read_dir(root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false))
        .collect::<Vec<_>>();
    fixture_dirs.sort_by_key(|entry| entry.file_name());
    fixture_dirs
        .into_iter()
        .map(|entry| {
            let source = entry.path().join("source.sqlite3");
            let manifest = entry.path().join("expected_manifest.json");
            let bytes = fs::read(manifest).expect("expected manifest");
            let bytes = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]).unwrap_or(&bytes);
            let expected = serde_json::from_slice(bytes).expect("valid expected manifest");
            ReleasedFixture { source, expected }
        })
        .collect()
}

fn copy_released_fixture(label: &str) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/persistence_upgrade/fixtures/profile_001/source.sqlite3");
    let destination = temp_db_path(label);
    fs::copy(source, &destination).expect("copy released fixture");
    destination
}

async fn rewrite_stations_id_declaration(path: &Path, replacement: &str) {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("legacy fixture");
    connection
        .execute("PRAGMA writable_schema = ON")
        .await
        .expect("enable writable schema");
    sqlx::query(
        r#"
        UPDATE sqlite_schema
        SET sql = REPLACE(sql, 'id TEXT PRIMARY KEY', ?1)
        WHERE type = 'table' AND name = 'stations'
        "#,
    )
    .bind(replacement)
    .execute(&mut connection)
    .await
    .expect("rewrite station identifier");
    let rewritten: String = sqlx::query_scalar(
        "SELECT sql FROM sqlite_schema WHERE type = 'table' AND name = 'stations'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("rewritten station schema");
    assert!(
        rewritten.contains(replacement),
        "fixture schema did not contain the expected station identifier declaration"
    );
    connection
        .execute("PRAGMA writable_schema = OFF")
        .await
        .expect("disable writable schema");
    connection.close().await.expect("close rewritten fixture");
}

fn released_profile_ids_from_manifest() -> BTreeSet<String> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../docs/superpowers/audits/persistence-v2-released-schema-manifest.json");
    let raw = fs::read(path).expect("released schema manifest");
    serde_json::from_slice::<serde_json::Value>(&raw).expect("valid released schema manifest")
        ["profiles"]
        .as_object()
        .expect("manifest profiles object")
        .keys()
        .cloned()
        .collect()
}

async fn create_v2_target(path: &Path) {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("target connection");
    migrator()
        .run_direct(&mut connection)
        .await
        .expect("V2 migrations");
    connection.close().await.expect("close target");
}

fn binary_v2_schema_8() -> BinaryCompatibility {
    let schema_version = persistence::migrations::current_schema_version();
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=schema_version,
        writable_schema: BTreeSet::from([schema_version]),
    }
}

fn file_evidence(path: &Path) -> Vec<(String, u64, u128, String)> {
    [path.to_path_buf(), wal_path(path), shm_path(path)]
        .into_iter()
        .filter(|candidate| candidate.is_file())
        .map(|candidate| {
            let metadata = fs::metadata(&candidate).expect("fixture metadata");
            let modified = metadata
                .modified()
                .expect("fixture mtime")
                .duration_since(UNIX_EPOCH)
                .expect("fixture mtime after epoch")
                .as_nanos();
            let digest = Sha256::digest(fs::read(&candidate).expect("fixture bytes"));
            (
                candidate.file_name().unwrap().to_string_lossy().to_string(),
                metadata.len(),
                modified,
                digest.iter().map(|byte| format!("{byte:02x}")).collect(),
            )
        })
        .collect()
}

fn sha256_file(path: &Path) -> String {
    Sha256::digest(fs::read(path).expect("fixture bytes"))
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

async fn raw_schema_hash(path: &Path) -> String {
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(false)
        .read_only(true);
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .expect("fixture provenance connection");
    let rows = sqlx::query(
        r#"
        SELECT type, name, tbl_name, COALESCE(sql, '') AS sql
        FROM sqlite_schema
        WHERE name NOT LIKE 'sqlite_%'
        ORDER BY type ASC, name ASC, tbl_name ASC
        "#,
    )
    .fetch_all(&mut connection)
    .await
    .expect("fixture raw schema");
    connection.close().await.expect("close fixture provenance");
    let mut digest = Sha256::new();
    for (index, row) in rows.iter().enumerate() {
        if index > 0 {
            digest.update(b"\n");
        }
        for (field_index, field) in ["type", "name", "tbl_name", "sql"].iter().enumerate() {
            if field_index > 0 {
                digest.update([0x1f]);
            }
            digest.update(row.get::<String, _>(*field).as_bytes());
        }
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn temp_db_path(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock")
        .as_nanos();
    std::env::temp_dir().join(format!("relay-pool-{label}-{nonce}.sqlite3"))
}

fn wal_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-wal", path.display()))
}

fn shm_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}-shm", path.display()))
}

fn remove_sqlite_set(path: &Path) {
    for candidate in [path.to_path_buf(), wal_path(path), shm_path(path)] {
        let _ = fs::remove_file(candidate);
    }
}
