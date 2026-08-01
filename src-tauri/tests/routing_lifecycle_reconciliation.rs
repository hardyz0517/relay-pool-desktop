#![allow(dead_code)]

mod persistence {
    pub(crate) mod error {
        #[derive(Debug, thiserror::Error)]
        pub(crate) enum PersistenceError {
            #[error("database operation failed: {0}")]
            DatabaseFailed(String),
        }

        impl From<sqlx::Error> for PersistenceError {
            fn from(error: sqlx::Error) -> Self {
                Self::DatabaseFailed(error.to_string())
            }
        }
    }
}

#[path = "../src/persistence/stores/request_lifecycle_reconciliation.rs"]
mod reconciliation;

use reconciliation::reconcile_startup_interrupted_batch;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row, SqlitePool,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

async fn test_pool() -> SqlitePool {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = tempdir.path().join("relay-pool.sqlite3");
    let options = SqliteConnectOptions::new()
        .filename(path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("pool");
    MIGRATOR.run(&pool).await.expect("migrations");
    pool
}

async fn seed_in_progress_request(pool: &SqlitePool, request_id: &str, started_at_ms: i64) {
    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, status,
            lifecycle_status, created_at
         ) VALUES (?, ?, ?, 'POST', '/v1/chat/completions',
                   'chat_completions', 'in_progress', 'admitted', ?)",
    )
    .bind(request_id)
    .bind(request_id)
    .bind(started_at_ms.to_string())
    .bind(started_at_ms.to_string())
    .execute(pool)
    .await
    .expect("request start");
}

async fn seed_completed_request(pool: &SqlitePool, request_id: &str) {
    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, finished_at, method, path, endpoint, status,
            lifecycle_status, terminal_kind, terminal_at_ms, created_at
         ) VALUES (?, ?, '1', '2', 'POST', '/v1/chat/completions',
                   'chat_completions', 'success', 'completed', 'completed', 2, '1')",
    )
    .bind(request_id)
    .bind(request_id)
    .execute(pool)
    .await
    .expect("completed request");
}

async fn seed_terminal_attempt(pool: &SqlitePool, request_id: &str, ordinal: u16) {
    sqlx::query(
        "INSERT INTO request_attempts (
            request_id, ordinal, station_id, station_key_id, endpoint_revision,
            started_at_ms, terminal_kind, health_effect, output_committed, terminal_at_ms
         ) VALUES (?, ?, 'station-a', 'key-a', 1, 10, 'succeeded', 'success', 1, 20)",
    )
    .bind(request_id)
    .bind(i64::from(ordinal))
    .execute(pool)
    .await
    .expect("attempt");
}

async fn seed_route_decision(pool: &SqlitePool, request_id: &str) {
    sqlx::query(
        "INSERT INTO route_decisions (
            id, request_id, decided_at_ms, ordering_profile,
            selected_station_key_id, selected_station_id, selected_endpoint_revision,
            candidate_count, candidate_detail_count, candidate_detail_truncated,
            rejection_counts_json, snapshot_id, fact_version_vector,
            planner_version, projector_version, runtime_overlay_revision,
            trace_status, created_at_ms, updated_at_ms
         ) VALUES (?, ?, 11, 'priority_first', 'key-a', 'station-a', 1,
                   1, 1, 0, '{}', 'snapshot-a', 'facts-a',
                   'planner-v1', 'projector-v1', 1, 'complete', 11, 11)",
    )
    .bind(format!("decision-{request_id}"))
    .bind(request_id)
    .execute(pool)
    .await
    .expect("route decision");
}

#[tokio::test]
async fn startup_reconciliation_marks_in_progress_requests_trace_incomplete_without_guessing_attempts(
) {
    let pool = test_pool().await;
    seed_in_progress_request(&pool, "req-reconcile", 1_000).await;
    seed_terminal_attempt(&pool, "req-reconcile", 0).await;
    seed_route_decision(&pool, "req-reconcile").await;
    seed_completed_request(&pool, "req-complete").await;

    let mut connection = pool.acquire().await.expect("connection");
    let first = reconcile_startup_interrupted_batch(&mut connection, 5_000, 16)
        .await
        .expect("first reconciliation");

    assert!(!first.has_more);
    assert_eq!(first.report.batches_completed, 1);
    assert_eq!(first.report.requests_interrupted, 1);
    assert_eq!(first.report.attempt_cost_gaps_inserted, 1);
    assert_eq!(first.report.decisions_marked_trace_incomplete, 1);

    let request = sqlx::query(
        "SELECT status, lifecycle_status, terminal_kind, terminal_code,
                terminal_detail, protocol_completed, delivery_terminal,
                terminal_at_ms, duration_ms
         FROM request_logs WHERE request_id = 'req-reconcile'",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("request row");
    assert_eq!(request.get::<String, _>(0), "interrupted");
    assert_eq!(request.get::<String, _>(1), "interrupted");
    assert_eq!(request.get::<String, _>(2), "interrupted");
    assert_eq!(request.get::<String, _>(3), "startup_interrupted");
    assert!(request.get::<String, _>(4).contains("trace_incomplete"));
    assert_eq!(request.get::<i64, _>(5), 0);
    assert_eq!(request.get::<String, _>(6), "NotStarted");
    assert_eq!(request.get::<i64, _>(7), 5_000);
    assert_eq!(request.get::<i64, _>(8), 4_000);

    let complete = sqlx::query("SELECT status FROM request_logs WHERE request_id = 'req-complete'")
        .fetch_one(&mut *connection)
        .await
        .expect("completed row");
    assert_eq!(complete.get::<String, _>(0), "success");

    let decision =
        sqlx::query("SELECT trace_status FROM route_decisions WHERE request_id = 'req-reconcile'")
            .fetch_one(&mut *connection)
            .await
            .expect("decision row");
    assert_eq!(decision.get::<String, _>(0), "trace_incomplete");

    let cost = sqlx::query(
        "SELECT ordinal, pricing_context_id, cost_status, total_cost_micro
         FROM routing_attempt_costs WHERE request_id = 'req-reconcile'",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("cost gap");
    assert_eq!(cost.get::<i64, _>(0), 0);
    assert_eq!(cost.get::<String, _>(1), "trace_incomplete");
    assert_eq!(cost.get::<String, _>(2), "missing_usage");
    assert_eq!(cost.get::<Option<i64>, _>(3), None);

    let rerun = reconcile_startup_interrupted_batch(&mut connection, 6_000, 16)
        .await
        .expect("rerun reconciliation");
    assert_eq!(rerun.report.requests_interrupted, 0);
    assert_eq!(rerun.report.attempt_cost_gaps_inserted, 0);
    assert_eq!(rerun.report.decisions_marked_trace_incomplete, 0);
    let cost_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM routing_attempt_costs WHERE request_id = ?")
            .bind("req-reconcile")
            .fetch_one(&mut *connection)
            .await
            .expect("cost count");
    assert_eq!(cost_count, 1);
}

#[tokio::test]
async fn startup_reconciliation_uses_bounded_batches_and_durable_progress() {
    let pool = test_pool().await;
    for request_id in ["req-batch-1", "req-batch-2", "req-batch-3"] {
        seed_in_progress_request(&pool, request_id, 1_000).await;
    }

    let mut connection = pool.acquire().await.expect("connection");
    let first = reconcile_startup_interrupted_batch(&mut connection, 5_000, 2)
        .await
        .expect("first batch");
    assert!(first.has_more);
    assert_eq!(first.report.requests_interrupted, 2);

    let second = reconcile_startup_interrupted_batch(&mut connection, 5_100, 2)
        .await
        .expect("second batch");
    assert!(!second.has_more);
    assert_eq!(second.report.requests_interrupted, 1);

    let progress = sqlx::query(
        "SELECT batches_completed, requests_interrupted, completed
         FROM routing_lifecycle_reconciliation_progress WHERE singleton_key = 1",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("progress row");
    assert_eq!(progress.get::<i64, _>(0), 2);
    assert_eq!(progress.get::<i64, _>(1), 3);
    assert_eq!(progress.get::<i64, _>(2), 1);

    let done = reconcile_startup_interrupted_batch(&mut connection, 5_200, 2)
        .await
        .expect("completion batch");
    assert_eq!(done.report.requests_interrupted, 0);
    let completed: i64 = sqlx::query_scalar(
        "SELECT completed FROM routing_lifecycle_reconciliation_progress WHERE singleton_key = 1",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("completed flag");
    assert_eq!(completed, 1);
}
