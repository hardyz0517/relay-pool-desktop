use std::{fs, path::Path, time::Duration};

use relay_pool_desktop_lib::test_support::schema_upgrade::run_schema_upgrade;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};

static MIGRATOR: Migrator = sqlx::migrate!("src/persistence/migrations");

#[test]
fn latest_schema_with_partial_sanitizer_resumes_on_next_startup() {
    let root = tempfile::tempdir().expect("temporary upgrade root");
    let data_dir = root.path().join("data");
    fs::create_dir_all(&data_dir).expect("data directory");
    let database_path = data_dir.join("relay-pool-desktop-v2.sqlite3");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/persistence/schema15/released-schema15.sqlite3");
    fs::copy(fixture, &database_path).expect("copy fixture");

    run_schema_upgrade(
        &data_dir,
        &database_path,
        "schema15-fixture-device-key",
        [0x07; 32],
    )
    .expect("initial schema 15 upgrade");

    tauri::async_runtime::block_on(async {
        let pool = open_pool(&database_path).await;
        sqlx::query(
            r#"
            INSERT INTO request_logs (
                id, request_id, started_at, method, path, endpoint, stream, status,
                lifecycle_status, upstream_base_url, fallback_count, created_at
            ) VALUES ('restart-sanitizer-1', 'restart-sanitizer-1', '1', 'POST',
                '/v1/chat/completions', 'chat', 0, 'success', 'completed',
                'https://user:pass@example.test/v1?token=synthetic', 0, '9999')
            "#,
        )
        .execute(&pool)
        .await
        .expect("seed post-upgrade request log");
        sqlx::query(
            "UPDATE request_log_url_sanitizer_progress SET status = 'running', updated_at = '9999' WHERE id = 'request_logs_upstream_base_url_v1'",
        )
        .execute(&pool)
        .await
        .expect("mark sanitizer running");
        pool.close().await;
    });

    let resumed = run_schema_upgrade(
        &data_dir,
        &database_path,
        "schema15-fixture-device-key",
        [0x07; 32],
    )
    .expect("restart should resume sanitizer");
    let current_schema = MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .expect("migration registry is not empty");
    assert_eq!(resumed.schema_version, current_schema);
    assert_eq!(resumed.open_mode, "writable");
    assert_eq!(
        query_i64(
            &database_path,
            "SELECT COUNT(*) FROM request_logs WHERE upstream_base_url IS NOT NULL"
        ),
        0
    );
    assert_eq!(query_string(&database_path, "SELECT status FROM request_log_url_sanitizer_progress WHERE id = 'request_logs_upstream_base_url_v1'"), "complete");
}

async fn open_pool(path: &Path) -> SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(false)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .foreign_keys(true),
        )
        .await
        .expect("open database")
}

fn query_i64(path: &Path, sql: &str) -> i64 {
    tauri::async_runtime::block_on(async {
        let pool = open_pool(path).await;
        let value = sqlx::query_scalar::<_, i64>(sql)
            .fetch_one(&pool)
            .await
            .expect("scalar query");
        pool.close().await;
        value
    })
}

fn query_string(path: &Path, sql: &str) -> String {
    tauri::async_runtime::block_on(async {
        let pool = open_pool(path).await;
        let value = sqlx::query_scalar::<_, String>(sql)
            .fetch_optional(&pool)
            .await
            .expect("scalar query")
            .unwrap_or_default();
        pool.close().await;
        value
    })
}
