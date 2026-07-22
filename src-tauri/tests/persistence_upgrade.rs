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
use sqlx::{sqlite::SqliteConnectOptions, Connection, Row, SqliteConnection};

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
        let profile = detect_profile(&fixture.source)
            .await
            .expect("known profile");
        assert_eq!(profile.id(), fixture.expected.profile);
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
