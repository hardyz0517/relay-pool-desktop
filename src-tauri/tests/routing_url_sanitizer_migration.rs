mod services {
    pub(crate) mod time {
        pub(crate) fn now_millis_for_services() -> u128 {
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system time after epoch")
                .as_millis()
        }
    }
}

mod persistence {
    pub(crate) mod error {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/error.rs"
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
    pub(crate) mod schema_registry {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/schema_registry.rs"
        ));
    }
    pub(crate) mod maintenance {
        pub(crate) mod request_log_url_sanitizer {
            include!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/src/persistence/maintenance/request_log_url_sanitizer.rs"
            ));
        }
    }
    pub(crate) mod migrations {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/migrations.rs"
        ));
    }
    pub(crate) mod read_session {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/read_session.rs"
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
    pub(crate) mod health_check {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/health_check.rs"
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
}

use std::{fs, path::Path, time::Duration};

use persistence::{
    maintenance::request_log_url_sanitizer::{
        sanitize_legacy_upstream_url, sanitize_legacy_upstream_url_bytes,
        sanitize_request_log_upstream_urls, LegacyUrlSanitization, RequestLogUrlSanitizerOptions,
    },
    runtime::PersistenceRuntime,
};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    Row, SqlitePool,
};

#[test]
fn sanitizer_primitive_uses_url_parser_and_never_preserves_sensitive_parts() {
    assert_eq!(
        sanitize_legacy_upstream_url(
            "https://user:pass@example.test:8443/v1/chat?api_key=sk-secret#frag"
        ),
        LegacyUrlSanitization::SanitizedOrigin {
            origin: "https://example.test:8443".to_string(),
        }
    );
    assert_eq!(
        sanitize_legacy_upstream_url("https://[::1]:8080/v1/%73afe?token=x"),
        LegacyUrlSanitization::SanitizedOrigin {
            origin: "https://[::1]:8080".to_string(),
        }
    );
    assert_eq!(
        sanitize_legacy_upstream_url("file:///C:/private/token.txt"),
        LegacyUrlSanitization::RedactedNonHttp
    );
    assert_eq!(
        sanitize_legacy_upstream_url("not a url with sk-secret"),
        LegacyUrlSanitization::RedactedUnparseable
    );
    assert_eq!(
        sanitize_legacy_upstream_url_bytes(&[0xff, b'h', b't', b't', b'p']),
        LegacyUrlSanitization::RedactedUnparseable
    );
}

#[tokio::test]
async fn sanitizer_batch_resume_blocks_runtime_ready_until_complete() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("relay-pool-v2.sqlite3");
    persistence::migrations::initialize_v2_database(&path)
        .await
        .expect("initialize schema 18");
    let pool = open_pool(&path).await;
    seed_request_logs(
        &pool,
        &[
            (
                "req-1",
                "https://user:pass@example.test/v1?api_key=sk-secret#frag",
            ),
            ("req-2", "file:///C:/private/token.txt"),
            ("req-3", "not a url with sk-secret"),
        ],
    )
    .await;

    let partial = sanitize_request_log_upstream_urls(
        &pool,
        RequestLogUrlSanitizerOptions {
            batch_size: 2,
            max_batches: Some(1),
        },
    )
    .await
    .expect("partial sanitizer");
    assert!(!partial.complete);
    assert_eq!(non_null_upstream_url_count(&pool).await, 1);
    pool.close().await;

    let open_error = PersistenceRuntime::open_current(&path)
        .await
        .expect_err("incomplete sanitizer must not publish ready runtime");
    assert!(
        open_error
            .to_string()
            .contains("persistence invariant violated"),
        "unexpected error: {open_error:?}"
    );

    let pool = open_pool(&path).await;
    let complete =
        sanitize_request_log_upstream_urls(&pool, RequestLogUrlSanitizerOptions::default())
            .await
            .expect("resume sanitizer");
    assert!(complete.complete);
    assert_eq!(non_null_upstream_url_count(&pool).await, 0);
    assert_eq!(progress_status(&pool).await, "complete");
    pool.close().await;

    let runtime = PersistenceRuntime::open_current(&path)
        .await
        .expect("sanitized runtime opens");
    runtime.close().await.expect("close runtime");
}

#[tokio::test]
async fn schema17_upgrade_runs_sanitizer_before_runtime_can_open() {
    let root = tempfile::tempdir().expect("tempdir");
    let path = root.path().join("relay-pool-v2.sqlite3");
    initialize_database_through(&path, 17).await;
    let pool = open_pool(&path).await;
    seed_request_logs(
        &pool,
        &[
            (
                "req-upgrade-1",
                "https://user:pass@example.test/v1?token=sk-task25-canary",
            ),
            ("req-upgrade-2", "notaurl"),
        ],
    )
    .await;
    seed_request_log_bytes(&pool, "req-upgrade-3", &[0xff, b's', b'k', b'-', b't']).await;
    pool.close().await;

    let backup = persistence::migrations::upgrade_existing_v2_database(&path)
        .await
        .expect("upgrade and sanitize")
        .expect("schema upgrade backup");
    assert!(backup.is_file());

    let pool = open_pool(&path).await;
    assert_eq!(
        persistence::migrations::applied_schema_version(&pool)
            .await
            .unwrap(),
        persistence::migrations::current_schema_version()
    );
    assert_eq!(non_null_upstream_url_count(&pool).await, 0);
    assert_eq!(progress_status(&pool).await, "complete");
    let redacted_unparseable: i64 = sqlx::query_scalar(
        "SELECT redacted_unparseable_count FROM request_log_url_sanitizer_progress WHERE id = 'request_logs_upstream_base_url_v1'",
    )
    .fetch_one(&pool)
    .await
    .expect("unparseable count");
    assert_eq!(
        redacted_unparseable, 0,
        "pre-18 scrub runs before schema 18 progress exists so upgrade backups cannot retain raw URLs"
    );
    pool.close().await;

    assert_no_file_contains(
        &[
            path.as_path(),
            backup.as_path(),
            &path.with_extension("sqlite3-wal"),
            &path.with_extension("sqlite3-shm"),
        ],
        b"sk-task25-canary",
    );

    let runtime = PersistenceRuntime::open_current(&path)
        .await
        .expect("upgraded sanitized runtime opens");
    runtime.close().await.expect("close runtime");
}

async fn initialize_database_through(path: &Path, target_version: i64) {
    let pool = open_create_pool(path).await;
    let partial = persistence::migrations::migrator_through(target_version)
        .expect("target schema is registered");
    partial.run(&pool).await.expect("partial migrations");
    if target_version == 17 {
        mark_secret_baseline_completed_for_fixture(&pool).await;
    }
    pool.close().await;
}

async fn mark_secret_baseline_completed_for_fixture(pool: &SqlitePool) {
    sqlx::query(
        r#"
        UPDATE persistence_schema_compatibility
        SET schema_version = 17,
            min_reader_app_version = '0.3.1',
            min_writer_app_version = '0.3.1',
            updated_by_migration = 17,
            updated_at = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
        WHERE singleton_key = 1
        "#,
    )
    .execute(pool)
    .await
    .expect("mark test fixture secret baseline complete");
}

async fn open_create_pool(path: &Path) -> SqlitePool {
    let options = sqlite_options(path, true);
    SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .expect("create pool")
}

async fn open_pool(path: &Path) -> SqlitePool {
    let options = sqlite_options(path, false);
    SqlitePoolOptions::new()
        .max_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .connect_with(options)
        .await
        .expect("open pool")
}

fn sqlite_options(path: &Path, create: bool) -> SqliteConnectOptions {
    SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(create)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Full)
        .foreign_keys(true)
        .busy_timeout(Duration::from_secs(5))
}

async fn seed_request_logs(pool: &SqlitePool, rows: &[(&str, &str)]) {
    for (index, (request_id, upstream_base_url)) in rows.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO request_logs (
                id, request_id, started_at, method, path, endpoint, stream, status,
                lifecycle_status, upstream_base_url, fallback_count, created_at
            ) VALUES (?, ?, '1', 'POST', '/v1/chat/completions', 'chat', 0, 'success',
                'completed', ?, 0, ?)
            "#,
        )
        .bind(*request_id)
        .bind(*request_id)
        .bind(*upstream_base_url)
        .bind(format!("{:04}", index))
        .execute(pool)
        .await
        .expect("insert request log");
    }
}

async fn seed_request_log_bytes(pool: &SqlitePool, request_id: &str, upstream_base_url: &[u8]) {
    sqlx::query(
        r#"
        INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, stream, status,
            lifecycle_status, upstream_base_url, fallback_count, created_at
        ) VALUES (?, ?, '1', 'POST', '/v1/chat/completions', 'chat', 0, 'success',
            'completed', ?, 0, ?)
        "#,
    )
    .bind(request_id)
    .bind(request_id)
    .bind(upstream_base_url)
    .bind("bytes")
    .execute(pool)
    .await
    .expect("insert byte request log");
}

async fn non_null_upstream_url_count(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM request_logs WHERE upstream_base_url IS NOT NULL")
        .fetch_one(pool)
        .await
        .expect("count upstream urls")
}

async fn progress_status(pool: &SqlitePool) -> String {
    sqlx::query("SELECT status FROM request_log_url_sanitizer_progress WHERE id = ?")
        .bind("request_logs_upstream_base_url_v1")
        .fetch_one(pool)
        .await
        .expect("progress row")
        .get("status")
}

fn assert_no_file_contains(paths: &[&Path], needle: &[u8]) {
    for path in paths {
        if !path.exists() {
            continue;
        }
        let bytes =
            fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
        assert!(
            !bytes.windows(needle.len()).any(|window| window == needle),
            "{} still contains forbidden canary",
            path.display()
        );
    }
}
