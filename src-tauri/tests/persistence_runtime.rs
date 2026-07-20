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

    pub(crate) mod runtime {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/runtime.rs"
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
}

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use persistence::{
    error::PersistenceError,
    runtime::PersistenceRuntime,
    schema_compatibility::{BinaryCompatibility, OpenMode, SchemaCompatibility},
};
use semver::Version;
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[tokio::test]
async fn writable_open_requires_compatible_schema_metadata() {
    let db = V2Fixture::create().await;
    db.set_compatibility(SchemaCompatibility {
        database_generation: 2,
        schema_version: 1,
        min_reader_app_version: Version::new(0, 4, 0),
        min_writer_app_version: Version::new(0, 4, 0),
        updated_by_migration: 1,
    })
    .await;

    let error = PersistenceRuntime::open(db.path(), binary_031())
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        PersistenceError::IncompatibleSchema {
            writable: false,
            ..
        }
    ));
    assert_eq!(db.write_probe_count().await, 0);
}

#[tokio::test]
async fn missing_database_is_not_created_by_normal_open() {
    let db = temp_db_path("missing");

    let error = PersistenceRuntime::open(&db, binary_031())
        .await
        .unwrap_err();

    assert!(matches!(error, PersistenceError::MissingDatabase));
    assert!(!db.exists());
}

#[tokio::test]
async fn generation_mismatch_fails_before_health_write() {
    let db = V2Fixture::create().await;
    let mut binary = binary_031();
    binary.database_generation = 3;

    let error = PersistenceRuntime::open(db.path(), binary)
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        PersistenceError::IncompatibleSchema {
            writable: false,
            ..
        }
    ));
    assert_eq!(db.write_probe_count().await, 0);
}

#[tokio::test]
async fn readable_but_not_writable_opens_in_inspection_only_mode() {
    let db = V2Fixture::create().await;
    db.set_compatibility(SchemaCompatibility {
        database_generation: 2,
        schema_version: 4,
        min_reader_app_version: Version::new(0, 3, 1),
        min_writer_app_version: Version::new(0, 4, 0),
        updated_by_migration: 4,
    })
    .await;

    let runtime = PersistenceRuntime::open(db.path(), binary_031())
        .await
        .expect("inspection runtime");

    assert_eq!(runtime.open_mode(), OpenMode::InspectionOnly);
    assert_eq!(runtime.compatibility().schema_version, 4);
    assert_eq!(
        runtime.health().await.expect("health").open_mode,
        "inspection_only"
    );
    assert_eq!(db.write_probe_count().await, 0);
}

#[tokio::test]
async fn unknown_future_schema_fails_before_health_write() {
    let db = V2Fixture::create().await;
    db.set_compatibility(SchemaCompatibility {
        database_generation: 2,
        schema_version: 99,
        min_reader_app_version: Version::new(0, 3, 1),
        min_writer_app_version: Version::new(0, 3, 1),
        updated_by_migration: 99,
    })
    .await;

    let error = PersistenceRuntime::open(db.path(), binary_031())
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        PersistenceError::IncompatibleSchema {
            writable: false,
            ..
        }
    ));
    assert_eq!(db.write_probe_count().await, 0);
}

#[tokio::test]
async fn metadata_sqlx_mismatch_fails_before_health_write() {
    let db = V2Fixture::create().await;
    db.set_compatibility(SchemaCompatibility {
        database_generation: 2,
        schema_version: 2,
        min_reader_app_version: Version::new(0, 3, 1),
        min_writer_app_version: Version::new(0, 3, 1),
        updated_by_migration: 2,
    })
    .await;

    let error = PersistenceRuntime::open(db.path(), binary_for_schema(1))
        .await
        .unwrap_err();

    assert!(matches!(
        error,
        PersistenceError::IncompatibleSchema {
            writable: false,
            ..
        }
    ));
    assert_eq!(db.write_probe_count().await, 0);
}

#[tokio::test]
async fn valid_writable_open_records_health_without_exposing_pool() {
    let db = V2Fixture::create().await;

    let runtime = PersistenceRuntime::open(db.path(), binary_031())
        .await
        .expect("writable runtime");

    assert_eq!(runtime.open_mode(), OpenMode::Writable);
    assert_eq!(runtime.compatibility_decision_code(), "writable");
    assert_eq!(runtime.compatibility().database_generation, 2);
    assert_eq!(
        runtime.health().await.expect("health").open_mode,
        "writable"
    );
    assert_eq!(db.write_probe_count().await, 1);
}

fn binary_031() -> BinaryCompatibility {
    binary_for_schema(5)
}

fn binary_for_schema(schema: i64) -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=schema,
        writable_schema: BTreeSet::from([schema]),
    }
}

struct V2Fixture {
    path: PathBuf,
}

impl V2Fixture {
    async fn create() -> Self {
        let path = temp_db_path("v2");
        let mut connection = SqliteConnectOptions::new()
            .filename(&path)
            .create_if_missing(true)
            .connect()
            .await
            .expect("connect fixture");
        persistence::migrations::migrator()
            .run(&mut connection)
            .await
            .expect("migrate fixture");
        connection.close().await.expect("close fixture");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    async fn set_compatibility(&self, compatibility: SchemaCompatibility) {
        let mut connection = SqliteConnectOptions::new()
            .filename(&self.path)
            .create_if_missing(false)
            .connect()
            .await
            .expect("connect fixture");
        sqlx::query(
            r#"
            UPDATE persistence_schema_compatibility
            SET database_generation = ?1,
                schema_version = ?2,
                min_reader_app_version = ?3,
                min_writer_app_version = ?4,
                updated_by_migration = ?5
            WHERE singleton_key = 1
            "#,
        )
        .bind(compatibility.database_generation)
        .bind(compatibility.schema_version)
        .bind(compatibility.min_reader_app_version.to_string())
        .bind(compatibility.min_writer_app_version.to_string())
        .bind(compatibility.updated_by_migration)
        .execute(&mut connection)
        .await
        .expect("set compatibility");
        connection.close().await.expect("close fixture");
    }

    async fn write_probe_count(&self) -> i64 {
        let mut connection = SqliteConnectOptions::new()
            .filename(&self.path)
            .create_if_missing(false)
            .connect()
            .await
            .expect("connect fixture");
        let row = sqlx::query(
            r#"
            SELECT write_probe_count
            FROM persistence_runtime_health
            WHERE singleton_key = 1
            "#,
        )
        .fetch_one(&mut connection)
        .await
        .expect("probe count");
        let count = row.get::<i64, _>("write_probe_count");
        connection.close().await.expect("close fixture");
        count
    }
}

fn temp_db_path(name: &str) -> PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("relay-pool-persistence-runtime-{name}-{id}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("clean stale fixture dir");
    }
    std::fs::create_dir_all(&root).expect("fixture dir");
    root.join("relay-pool-v2.sqlite3")
}
