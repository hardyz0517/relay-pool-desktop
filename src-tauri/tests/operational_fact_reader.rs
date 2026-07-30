#![allow(dead_code)]

use std::str::FromStr;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Executor, Row, Sqlite, SqliteConnection, SqlitePool, Transaction,
};

#[path = "../src/models/operational/mod.rs"]
mod operational_model;

mod models {
    pub(crate) mod operational {
        pub(crate) use crate::operational_model::*;
    }
}

#[path = "../src/application/operational_facts/mod.rs"]
mod app_operational_facts;

mod application {
    pub(crate) mod operational_facts {
        pub(crate) use crate::app_operational_facts::assembler;
        pub(crate) use crate::app_operational_facts::*;
    }
}

mod persistence {
    pub(crate) mod error {
        #[derive(Debug, thiserror::Error)]
        pub(crate) enum PersistenceError {
            #[error("record not found")]
            NotFound,
            #[error("constraint violation")]
            ConstraintViolation,
            #[error("database operation failed")]
            DatabaseFailed,
        }

        impl From<sqlx::Error> for PersistenceError {
            fn from(error: sqlx::Error) -> Self {
                match error {
                    sqlx::Error::RowNotFound => Self::NotFound,
                    sqlx::Error::Database(database)
                        if database.is_unique_violation() || database.is_foreign_key_violation() =>
                    {
                        Self::ConstraintViolation
                    }
                    _ => Self::DatabaseFailed,
                }
            }
        }
    }

    pub(crate) use crate::TestReadSession as ReadSession;
}

#[path = "../src/persistence/stores/operational_facts/queries.rs"]
mod operational_fact_queries;

use application::operational_facts::OperationalFactReadOptions;
use operational_fact_queries::{OperationalFactQueryError, OperationalFactStore};

pub(crate) struct TestReadSession {
    transaction: Option<Transaction<'static, Sqlite>>,
}

impl TestReadSession {
    async fn begin(pool: &SqlitePool) -> Self {
        Self {
            transaction: Some(pool.begin().await.expect("begin read transaction")),
        }
    }

    pub(crate) fn connection(&mut self) -> &mut SqliteConnection {
        let transaction = self.transaction.as_mut().expect("open transaction");
        &mut *transaction
    }
}

async fn test_pool() -> SqlitePool {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let db_path = tempdir.path().join("operational-facts.sqlite");
    let options = SqliteConnectOptions::from_str(&format!("sqlite://{}", db_path.display()))
        .expect("sqlite options")
        .create_if_missing(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(4)
        .connect_with(options)
        .await
        .expect("sqlite pool");
    // Keep the directory alive for the process lifetime of this short test.
    std::mem::forget(tempdir);
    pool.execute("PRAGMA journal_mode = WAL")
        .await
        .expect("wal mode");
    pool.execute("PRAGMA busy_timeout = 1000")
        .await
        .expect("busy timeout");
    create_schema(&pool).await;
    pool
}

async fn create_schema(pool: &SqlitePool) {
    pool.execute(
        r#"
        CREATE TABLE settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE stations (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            api_base_url TEXT NOT NULL,
            endpoint_revision INTEGER NOT NULL,
            enabled INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE station_keys (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            api_key TEXT NOT NULL DEFAULT '',
            api_key_secret_id TEXT,
            enabled INTEGER NOT NULL,
            priority INTEGER NOT NULL,
            routing_order INTEGER,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE model_aliases (
            id TEXT PRIMARY KEY,
            client_model TEXT NOT NULL,
            upstream_model TEXT NOT NULL,
            enabled INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE station_key_capabilities (
            station_key_id TEXT PRIMARY KEY,
            supports_tools INTEGER NOT NULL,
            supports_vision INTEGER NOT NULL,
            supports_reasoning INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE station_key_health (
            station_key_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL,
            consecutive_failures INTEGER NOT NULL,
            success_count INTEGER NOT NULL,
            failure_count INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE station_endpoint_health (
            station_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL
        );
        CREATE TABLE balance_snapshots (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            scope TEXT NOT NULL,
            value REAL,
            currency TEXT NOT NULL,
            low_balance_threshold REAL,
            status TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE pricing_rules (
            id TEXT PRIMARY KEY,
            station_id TEXT NOT NULL,
            station_key_id TEXT,
            model TEXT NOT NULL,
            input_price REAL,
            output_price REAL,
            fixed_price REAL,
            rate_multiplier REAL,
            currency TEXT NOT NULL,
            unit TEXT NOT NULL,
            confidence REAL NOT NULL,
            enabled INTEGER NOT NULL,
            updated_at TEXT NOT NULL
        );
        "#,
    )
    .await
    .expect("schema");

    for key in [
        "default_routing_strategy",
        "max_rate_multiplier",
        "default_routing_group_filter",
        "scheduler_advanced_settings_json",
        "allow_depleted_fallback",
    ] {
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES (?1, 'fixture', '1')")
            .bind(key)
            .execute(pool)
            .await
            .expect("insert setting");
    }
}

async fn insert_candidate(pool: &SqlitePool, index: usize) {
    let station_id = format!("station-{index}");
    let key_id = format!("key-{index}");
    sqlx::query(
        "INSERT INTO stations (id, name, api_base_url, endpoint_revision, enabled, updated_at)
         VALUES (?1, ?2, 'https://api.example.com/v1/chat?debug=leak-canary', 1, 1, '1')",
    )
    .bind(&station_id)
    .bind(format!("Station {index}"))
    .execute(pool)
    .await
    .expect("insert station");
    sqlx::query(
        "INSERT INTO station_keys
         (id, station_id, api_key, api_key_secret_id, enabled, priority, routing_order, created_at, updated_at)
         VALUES (?1, ?2, 'key-leak-canary', NULL, 1, ?3, ?3, ?3, '1')",
    )
    .bind(&key_id)
    .bind(&station_id)
    .bind(index as i64)
    .execute(pool)
    .await
    .expect("insert key");
}

#[tokio::test]
async fn sqlite_read_transaction_keeps_one_snapshot_across_batched_selects() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    let mut transaction = pool.begin().await.expect("begin read");
    let first = sqlx::query("SELECT updated_at FROM stations WHERE id = 'station-1'")
        .fetch_one(&mut *transaction)
        .await
        .expect("first read")
        .get::<String, _>("updated_at");

    sqlx::query("UPDATE stations SET updated_at = '2' WHERE id = 'station-1'")
        .execute(&pool)
        .await
        .expect("concurrent writer");

    let second = sqlx::query("SELECT updated_at FROM stations WHERE id = 'station-1'")
        .fetch_one(&mut *transaction)
        .await
        .expect("second read")
        .get::<String, _>("updated_at");

    assert_eq!(first, "1");
    assert_eq!(second, "1");
}

#[tokio::test]
async fn autocommit_reads_can_mix_generations_between_batches() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    let first = sqlx::query("SELECT updated_at FROM stations WHERE id = 'station-1'")
        .fetch_one(&pool)
        .await
        .expect("first read")
        .get::<String, _>("updated_at");
    sqlx::query("UPDATE stations SET updated_at = '2' WHERE id = 'station-1'")
        .execute(&pool)
        .await
        .expect("writer");
    let second = sqlx::query("SELECT updated_at FROM stations WHERE id = 'station-1'")
        .fetch_one(&pool)
        .await
        .expect("second read")
        .get::<String, _>("updated_at");

    assert_eq!(first, "1");
    assert_eq!(second, "2");
}

#[tokio::test]
async fn fixed_query_count_does_not_scale_with_candidate_count() {
    let pool = test_pool().await;
    for index in 0..100 {
        insert_candidate(&pool, index).await;
    }
    sqlx::query(
        "INSERT INTO model_aliases (id, client_model, upstream_model, enabled, updated_at)
         VALUES ('alias-1', 'gpt-4.1', 'upstream-gpt-4.1', 1, '1')",
    )
    .execute(&pool)
    .await
    .expect("alias");

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let bundle = store
        .load_bundle(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect("bundle");

    assert_eq!(bundle.candidates().len(), 100);
    assert_eq!(bundle.query_count(), 9);
    assert!(!bundle.loaded_full_model_catalog());
}

#[tokio::test]
async fn single_model_shape_does_not_load_full_model_catalog() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;
    for index in 0..20 {
        sqlx::query(
            "INSERT INTO model_aliases (id, client_model, upstream_model, enabled, updated_at)
             VALUES (?1, ?2, ?3, 1, '1')",
        )
        .bind(format!("alias-{index}"))
        .bind(format!("client-{index}"))
        .bind(format!("upstream-{index}"))
        .execute(&pool)
        .await
        .expect("alias");
    }
    sqlx::query(
        "INSERT INTO model_aliases (id, client_model, upstream_model, enabled, updated_at)
         VALUES ('alias-target', 'gpt-4.1', 'upstream-gpt-4.1', 1, '1')",
    )
    .execute(&pool)
    .await
    .expect("target alias");

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let request_bundle = store
        .load_bundle(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect("request bundle");

    assert_eq!(request_bundle.model_aliases().len(), 1);
    assert!(!request_bundle.loaded_full_model_catalog());

    let mut catalog_read = TestReadSession::begin(&pool).await;
    let catalog_bundle = store
        .load_bundle(
            &mut catalog_read,
            &OperationalFactReadOptions::for_model_catalog(),
        )
        .await
        .expect("catalog bundle");
    assert_eq!(catalog_bundle.model_aliases().len(), 21);
    assert!(catalog_bundle.loaded_full_model_catalog());
}

#[tokio::test]
async fn reader_does_not_return_secret_raw_json_or_full_endpoint_url() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let bundle = store
        .load_bundle(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect("bundle");
    let debug = format!("{bundle:?}");

    assert!(bundle.candidates()[0].credential().available());
    assert_eq!(
        bundle.candidates()[0]
            .endpoint()
            .sanitized_origin()
            .as_str(),
        "https://api.example.com"
    );
    assert!(!debug.contains("key-leak-canary"));
    assert!(!debug.contains("debug=leak-canary"));
    assert!(!debug.contains("/v1/chat"));
    assert!(!debug.contains("collector_json"));
}

#[tokio::test]
async fn candidate_limit_failure_is_typed_and_not_silent_sql_truncation() {
    let pool = test_pool().await;
    for index in 0..3 {
        insert_candidate(&pool, index).await;
    }

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let error = store
        .load_bundle(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1").with_candidate_limit(2),
        )
        .await
        .expect_err("candidate limit");

    assert!(matches!(
        error,
        OperationalFactQueryError::CandidateLimitExceeded {
            actual: 3,
            limit: 2
        }
    ));
}
