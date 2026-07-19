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
    time::Duration,
};

use persistence::{
    backup::temporary_backup_path, error::PersistenceError, read_session::ReadSession,
    runtime::PersistenceRuntime, schema_compatibility::BinaryCompatibility,
    write_session::WriteSession,
};
use semver::Version;
use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[tokio::test]
async fn cancelled_uncommitted_write_rolls_back_and_releases_permit() {
    let runtime = V2Fixture::create().await.open().await;
    let store = TestStore;
    let write_runtime = runtime.clone();
    let task = tokio::spawn(async move {
        let mut write = write_runtime.begin_write().await.expect("begin write");
        TestStore
            .insert(&mut write, "cancelled")
            .await
            .expect("insert");
        std::future::pending::<()>().await;
    });

    tokio::time::sleep(Duration::from_millis(25)).await;
    task.abort();
    task.await.expect_err("aborted");

    assert_eq!(store.row_count(&runtime).await, 0);
    runtime
        .write(|_| Box::pin(async { Ok(()) }))
        .await
        .expect("permit released");
}

#[tokio::test]
async fn read_session_keeps_one_snapshot_across_two_queries() {
    let runtime = V2Fixture::create().await.open().await;
    let store = TestStore;
    store.replace(&runtime, "before").await.expect("seed");

    let mut read = runtime.begin_read().await.expect("begin read");
    let before = store.value(&mut read).await.expect("initial value");
    store.replace(&runtime, "new").await.expect("replace");
    let same_snapshot = store.value(&mut read).await.expect("snapshot value");

    assert_eq!(same_snapshot, before);
    let mut fresh = runtime.begin_read().await.expect("fresh read");
    assert_eq!(store.value(&mut fresh).await.expect("fresh value"), "new");
}

#[tokio::test]
async fn verified_backup_captures_committed_wal_data_and_reopens_read_only() {
    let fixture = V2Fixture::create().await;
    let runtime = fixture.open().await;
    TestStore
        .replace(&runtime, "wal-committed")
        .await
        .expect("write wal data");
    let backup_path = fixture.path.with_file_name("verified-backup.sqlite3");

    let backup = runtime
        .handle()
        .backup_to(&backup_path)
        .await
        .expect("verified backup");

    assert_eq!(backup.final_path, backup_path);
    assert_eq!(
        read_value_from_file(&backup.final_path).await,
        "wal-committed"
    );
}

#[tokio::test]
async fn interrupted_temporary_backup_output_never_becomes_final_candidate() {
    let fixture = V2Fixture::create().await;
    let runtime = fixture.open().await;
    let backup_path = fixture.path.with_file_name("blocked-backup.sqlite3");
    let temp_path = temporary_backup_path(&backup_path);
    std::fs::write(&temp_path, b"interrupted").expect("pre-existing temp");

    let error = runtime.handle().backup_to(&backup_path).await.unwrap_err();

    assert!(matches!(error, PersistenceError::IoFailed { .. }));
    assert!(!backup_path.exists());
    assert_eq!(
        std::fs::read(&temp_path).expect("temp preserved"),
        b"interrupted"
    );
}

struct TestStore;

impl TestStore {
    async fn insert(&self, write: &mut WriteSession, value: &str) -> Result<(), PersistenceError> {
        sqlx::query("INSERT INTO persistence_session_test (value) VALUES (?1)")
            .bind(value)
            .execute(write.connection())
            .await?;
        Ok(())
    }

    async fn replace(
        &self,
        runtime: &PersistenceRuntime,
        value: &str,
    ) -> Result<(), PersistenceError> {
        let value = value.to_owned();
        runtime
            .write(move |write| {
                Box::pin(async move {
                    sqlx::query("DELETE FROM persistence_session_test")
                        .execute(write.connection())
                        .await?;
                    sqlx::query("INSERT INTO persistence_session_test (value) VALUES (?1)")
                        .bind(value)
                        .execute(write.connection())
                        .await?;
                    Ok(())
                })
            })
            .await
    }

    async fn value(&self, read: &mut ReadSession) -> Result<String, PersistenceError> {
        let row = sqlx::query("SELECT value FROM persistence_session_test ORDER BY id LIMIT 1")
            .fetch_one(read.connection())
            .await?;
        Ok(row.get("value"))
    }

    async fn row_count(&self, runtime: &PersistenceRuntime) -> i64 {
        let mut read = runtime.begin_read().await.expect("read session");
        let row = sqlx::query("SELECT COUNT(*) AS count FROM persistence_session_test")
            .fetch_one(read.connection())
            .await
            .expect("count rows");
        row.get("count")
    }
}

#[derive(Clone)]
struct V2Fixture {
    path: PathBuf,
}

impl V2Fixture {
    async fn create() -> Self {
        let path = temp_db_path("sessions");
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
        sqlx::query(
            r#"
            CREATE TABLE persistence_session_test (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                value TEXT NOT NULL
            )
            "#,
        )
        .execute(&mut connection)
        .await
        .expect("create test table");
        connection.close().await.expect("close fixture");
        Self { path }
    }

    async fn open(&self) -> PersistenceRuntime {
        PersistenceRuntime::open(&self.path, binary_031())
            .await
            .expect("open runtime")
    }
}

fn binary_031() -> BinaryCompatibility {
    BinaryCompatibility {
        app_version: Version::new(0, 3, 1),
        database_generation: 2,
        readable_schema: 1..=1,
        writable_schema: BTreeSet::from([1]),
    }
}

async fn read_value_from_file(path: &Path) -> String {
    let mut connection = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .connect()
        .await
        .expect("open backup");
    let row = sqlx::query("SELECT value FROM persistence_session_test ORDER BY id LIMIT 1")
        .fetch_one(&mut connection)
        .await
        .expect("backup row");
    let value = row.get("value");
    connection.close().await.expect("close backup");
    value
}

fn temp_db_path(name: &str) -> PathBuf {
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("relay-pool-persistence-{name}-{id}"));
    if root.exists() {
        std::fs::remove_dir_all(&root).expect("clean stale fixture dir");
    }
    std::fs::create_dir_all(&root).expect("fixture dir");
    root.join("relay-pool-v2.sqlite3")
}
