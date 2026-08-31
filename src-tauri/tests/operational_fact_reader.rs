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

#[path = "../src/application/operational_facts/assembler.rs"]
mod operational_fact_assembler;

mod application {
    pub(crate) mod operational_facts {
        pub(crate) use crate::models::operational::OperationalFactReadOptions;
        pub(crate) use crate::operational_fact_assembler::*;
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
                        if database.is_unique_violation()
                            || database.is_foreign_key_violation() =>
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

use application::operational_facts::{
    assemble_operational_fact_bundle, OperationalFactBundle, OperationalFactReadOptions,
};
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

async fn load_bundle(
    store: OperationalFactStore,
    read: &mut TestReadSession,
    options: &OperationalFactReadOptions,
) -> OperationalFactBundle {
    let raw = store.load_raw(read, options).await.expect("raw facts");
    assemble_operational_fact_bundle(raw, options).expect("assembled operational facts")
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
            credit_per_cny REAL NOT NULL DEFAULT 1.0,
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
            schedulable INTEGER NOT NULL DEFAULT 1,
            group_binding_id TEXT,
            group_id_hash TEXT,
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
            supports_chat_completions INTEGER NOT NULL DEFAULT 1,
            supports_responses INTEGER NOT NULL DEFAULT 1,
            supports_stream INTEGER NOT NULL DEFAULT 1,
            supports_tools INTEGER NOT NULL,
            supports_vision INTEGER NOT NULL,
            supports_reasoning INTEGER NOT NULL,
            model_allowlist_json TEXT NOT NULL DEFAULT '[]',
            model_blocklist_json TEXT NOT NULL DEFAULT '[]',
            preferred_models_json TEXT NOT NULL DEFAULT '[]',
            only_use_as_backup INTEGER NOT NULL DEFAULT 0,
            routing_tags_json TEXT NOT NULL DEFAULT '[]',
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
        CREATE TABLE station_group_bindings (
            id TEXT PRIMARY KEY,
            group_id_hash TEXT,
            group_category_override TEXT,
            inferred_group_category TEXT,
            binding_status TEXT,
            effective_rate_multiplier REAL
        );
        CREATE TABLE station_capacity_domains (
            station_id TEXT PRIMARY KEY,
            provider_family TEXT NOT NULL,
            deployment_identity TEXT,
            region_identity TEXT,
            revision INTEGER NOT NULL CHECK (revision > 0)
        );
        CREATE TABLE station_endpoint_health (
            station_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL
        );
        CREATE TABLE routing_health_snapshot (
            station_key_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL,
            consecutive_failures INTEGER NOT NULL,
            success_count INTEGER NOT NULL,
            failure_count INTEGER NOT NULL,
            avg_latency_ms INTEGER,
            last_error_summary TEXT,
            cooldown_until TEXT,
            updated_at TEXT NOT NULL
        );
        CREATE TABLE endpoint_health_snapshot (
            station_id TEXT PRIMARY KEY,
            endpoint_revision INTEGER NOT NULL
        );
        CREATE TABLE routing_policy (
            singleton_key INTEGER PRIMARY KEY CHECK (singleton_key = 1),
            config_json TEXT NOT NULL,
            updated_at_ms INTEGER NOT NULL
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
            created_at TEXT NOT NULL DEFAULT '',
            updated_at TEXT NOT NULL
        );
        CREATE TABLE domain_revisions (
            scope TEXT PRIMARY KEY,
            revision INTEGER NOT NULL CHECK (revision > 0),
            updated_at_ms INTEGER NOT NULL CHECK (updated_at_ms >= 0),
            provenance TEXT NOT NULL
        );
        "#,
    )
    .await
    .expect("schema");

    for key in [
        "default_routing_strategy",
        "max_rate_multiplier",
        "default_routing_group_filter",
        "dispatch_algorithm_profile_json",
        "allow_depleted_fallback",
    ] {
        sqlx::query("INSERT INTO settings (key, value, updated_at) VALUES (?1, 'fixture', '1')")
            .bind(key)
            .execute(pool)
            .await
            .expect("insert setting");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES ('setting:' || ?1, 1, 0, 'baseline_snapshot')",
        )
        .bind(key)
        .execute(pool)
        .await
        .expect("insert setting revision");
    }
    sqlx::query(
        "INSERT INTO routing_policy (singleton_key, config_json, updated_at_ms) VALUES (1, ?1, 0)",
    )
    .bind(r#"{"version":1,"reliability_weight":4000,"responsiveness_weight":2500,"cost_weight":2000,"preference_weight":1500,"max_candidates":64,"exploration_share_basis_points":500,"allow_depleted_fallback":false,"affinity_enabled":false,"affinity_ttl_seconds":300}"#)
    .execute(pool)
    .await
    .expect("insert routing policy");
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance) VALUES ('routing_policy', 1, 0, 'baseline_snapshot')",
    )
    .execute(pool)
    .await
    .expect("insert routing policy revision");
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
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('station:' || ?1, 1, 0, 'baseline_snapshot')",
    )
    .bind(&station_id)
    .execute(pool)
    .await
    .expect("insert station revision");
    // Migration 0035 seeds typed account revision baselines for every station.
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('station_account:' || ?1, 1, 0, 'baseline_snapshot')",
    )
    .bind(&station_id)
    .execute(pool)
    .await
    .expect("insert station account revision");
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
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('station_key:' || ?1, 1, 0, 'baseline_snapshot')",
    )
    .bind(&key_id)
    .execute(pool)
    .await
    .expect("insert key revision");
}

#[tokio::test]
async fn planner_prefers_explicit_station_balance_over_stale_key_balance() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    sqlx::query(
        "INSERT INTO balance_snapshots
         (id, station_id, station_key_id, scope, value, currency, status, created_at, updated_at)
         VALUES ('key-depleted', 'station-1', 'key-1', 'station_key', 0, 'USD', 'depleted', '3', '3')",
    )
    .execute(&pool)
    .await
    .expect("key balance");
    sqlx::query(
        "INSERT INTO balance_snapshots
         (id, station_id, station_key_id, scope, value, currency, status, created_at, updated_at)
         VALUES ('station-available', 'station-1', NULL, 'station', 3.61, 'USD', 'normal', '2', '2')",
    )
    .execute(&pool)
    .await
    .expect("station balance");

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let rows = store
        .load_raw(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect("raw facts");

    assert_eq!(rows.candidates.len(), 1);
    assert_eq!(rows.candidates[0].balance_status.as_deref(), Some("normal"));
    assert_eq!(rows.candidates[0].balance_value, Some(3.61));
}

#[tokio::test]
async fn planner_prefers_explicit_key_balance_when_station_status_is_unknown() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    sqlx::query(
        "INSERT INTO balance_snapshots
         (id, station_id, station_key_id, scope, value, currency, status, created_at, updated_at)
         VALUES ('key-available', 'station-1', 'key-1', 'station_key', 2.5, 'USD', 'normal', '2', '2')",
    )
    .execute(&pool)
    .await
    .expect("key balance");
    sqlx::query(
        "INSERT INTO balance_snapshots
         (id, station_id, station_key_id, scope, value, currency, status, created_at, updated_at)
         VALUES ('station-unknown', 'station-1', NULL, 'station', 0, 'USD', 'unknown', '3', '3')",
    )
    .execute(&pool)
    .await
    .expect("station balance");

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let rows = store
        .load_raw(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect("raw facts");

    assert_eq!(rows.candidates[0].balance_status.as_deref(), Some("normal"));
    assert_eq!(rows.candidates[0].balance_value, Some(2.5));
}

#[tokio::test]
async fn planner_uses_latest_station_balance_instead_of_historical_depleted_status() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    sqlx::query(
        "INSERT INTO balance_snapshots
         (id, station_id, station_key_id, scope, value, currency, status, created_at, updated_at)
         VALUES ('station-old-depleted', 'station-1', NULL, 'station', 0, 'USD', 'depleted', '1', '1')",
    )
    .execute(&pool)
    .await
    .expect("historical station balance");
    sqlx::query(
        "INSERT INTO balance_snapshots
         (id, station_id, station_key_id, scope, value, currency, status, created_at, updated_at)
         VALUES ('station-current', 'station-1', NULL, 'station', 4.71, 'USD', 'low', '2', '2')",
    )
    .execute(&pool)
    .await
    .expect("current station balance");

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let rows = store
        .load_raw(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect("raw facts");

    assert_eq!(rows.candidates[0].balance_status.as_deref(), Some("low"));
    assert_eq!(rows.candidates[0].balance_value, Some(4.71));
}

async fn insert_alias_revision(pool: &SqlitePool, id: &str) {
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('model_alias:' || ?1, 1, 0, 'baseline_snapshot')",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("insert alias revision");
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
    insert_alias_revision(&pool, "alias-1").await;

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let bundle = load_bundle(
        store,
        &mut read,
        &OperationalFactReadOptions::for_request_model("gpt-4.1"),
    )
    .await;

    assert_eq!(bundle.candidates().len(), 100);
    assert_eq!(bundle.query_count(), 3);
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
        insert_alias_revision(&pool, &format!("alias-{index}")).await;
    }
    sqlx::query(
        "INSERT INTO model_aliases (id, client_model, upstream_model, enabled, updated_at)
         VALUES ('alias-target', 'gpt-4.1', 'upstream-gpt-4.1', 1, '1')",
    )
    .execute(&pool)
    .await
    .expect("target alias");
    insert_alias_revision(&pool, "alias-target").await;

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let request_bundle = load_bundle(
        store,
        &mut read,
        &OperationalFactReadOptions::for_request_model("gpt-4.1"),
    )
    .await;

    assert_eq!(request_bundle.model_aliases().len(), 1);
    assert!(!request_bundle.loaded_full_model_catalog());

    let mut catalog_read = TestReadSession::begin(&pool).await;
    let catalog_bundle = load_bundle(
        store,
        &mut catalog_read,
        &OperationalFactReadOptions::for_model_catalog(),
    )
    .await;
    assert_eq!(catalog_bundle.model_aliases().len(), 21);
    assert!(catalog_bundle.loaded_full_model_catalog());
}

#[tokio::test]
async fn reader_does_not_return_secret_raw_json_or_full_endpoint_url() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let bundle = load_bundle(
        store,
        &mut read,
        &OperationalFactReadOptions::for_request_model("gpt-4.1"),
    )
    .await;
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
async fn candidate_source_remains_complete_before_request_capability_filtering() {
    let pool = test_pool().await;
    for index in 0..3 {
        insert_candidate(&pool, index).await;
    }

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let rows = store
        .load_raw(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1").with_candidate_limit(2),
        )
        .await
        .expect("configured candidate rows load");

    // The source must not apply the routing cap. Model/capability filtering is
    // request-specific and belongs to PlanningSnapshotBuilder.
    assert_eq!(rows.candidates.len(), 3);
    assert_eq!(rows.candidates[0].station_key_id, "key-0");
    assert_eq!(rows.candidates[1].station_key_id, "key-1");
    assert_eq!(rows.candidates[2].station_key_id, "key-2");

    let bundle = assemble_operational_fact_bundle(
        rows,
        &OperationalFactReadOptions::for_request_model("gpt-4.1").with_candidate_limit(2),
    )
    .expect("fact assembly must remain complete before capability filtering");
    assert_eq!(bundle.candidates().len(), 3);
}

#[tokio::test]
async fn credentialless_keys_remain_visible_for_terminal_candidate_counts() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;
    // A credentialless configured key must remain visible so planning can
    // distinguish "configured but unavailable" from "no configured key".
    sqlx::query(
        "INSERT INTO station_keys
         (id, station_id, api_key, api_key_secret_id, enabled, priority, routing_order, created_at, updated_at)
         VALUES ('key-empty', 'station-1', '', NULL, 1, -1, -1, '-1', '1')",
    )
    .execute(&pool)
    .await
    .expect("insert credentialless key");
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('station_key:key-empty', 1, 0, 'baseline_snapshot')",
    )
    .execute(&pool)
    .await
    .expect("insert credentialless key revision");

    let store = OperationalFactStore;
    let mut read = TestReadSession::begin(&pool).await;
    let rows = store
        .load_raw(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1").with_candidate_limit(1),
        )
        .await
        .expect("configured candidate rows load");

    assert_eq!(rows.candidates.len(), 2);
    assert_eq!(rows.candidates[0].station_key_id, "key-empty");
    assert!(!rows.candidates[0].credential_available);
    assert_eq!(rows.candidates[1].station_key_id, "key-1");
    assert!(rows.candidates[1].credential_available);
}

#[tokio::test]
async fn missing_domain_revision_fails_closed_without_timestamp_fallback() {
    let pool = test_pool().await;
    insert_candidate(&pool, 1).await;
    sqlx::query("DELETE FROM domain_revisions WHERE scope = 'station_key:key-1'")
        .execute(&pool)
        .await
        .expect("remove revision");

    let mut read = TestReadSession::begin(&pool).await;
    let error = OperationalFactStore
        .load_raw(
            &mut read,
            &OperationalFactReadOptions::for_request_model("gpt-4.1"),
        )
        .await
        .expect_err("missing revision must not become one");

    assert!(matches!(
        error,
        OperationalFactQueryError::RevisionUnavailable { scope }
        if scope == "station_key:key-1"
    ));
}
