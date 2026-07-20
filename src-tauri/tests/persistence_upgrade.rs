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
    detect_profile, import_profile, validate_import, ExpectedImportManifest, UpgradeError,
};
use persistence::{
    migrations::migrator, runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility,
};
use semver::Version;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqliteConnectOptions, Connection, SqliteConnection};

#[tokio::test]
async fn every_released_profile_imports_to_the_expected_manifest() {
    let fixtures = released_fixtures();
    assert_eq!(
        fixtures.len(),
        6,
        "fixture generation must classify every released schema"
    );
    for fixture in fixtures {
        let before = file_evidence(&fixture.source);
        let profile = detect_profile(&fixture.source)
            .await
            .expect("known profile");
        assert_eq!(profile.id(), fixture.expected.profile);
        let target_path = temp_db_path(profile.id());
        create_v2_target(&target_path).await;
        let runtime = PersistenceRuntime::open(&target_path, binary_v2_schema_7())
            .await
            .expect("V2 runtime");

        import_profile(&profile, &fixture.source, &runtime.handle())
            .await
            .expect("released fixture import");
        validate_import(&runtime.handle(), &fixture.expected)
            .await
            .expect("import manifest");
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
            let expected = serde_json::from_slice(&fs::read(manifest).expect("expected manifest"))
                .expect("valid expected manifest");
            ReleasedFixture { source, expected }
        })
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
        .run(&mut connection)
        .await
        .expect("V2 migrations");
    connection.close().await.expect("close target");
}

fn binary_v2_schema_7() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=8,
        writable_schema: BTreeSet::from([8]),
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
