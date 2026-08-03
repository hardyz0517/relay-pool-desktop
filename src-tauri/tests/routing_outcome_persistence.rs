#![allow(dead_code)]

mod persistence {
    pub(crate) mod error {
        #[derive(Debug, thiserror::Error)]
        pub(crate) enum PersistenceError {
            #[error("database operation failed: {0}")]
            DatabaseFailed(String),
            #[error("persistence invariant violated: {0}")]
            InvariantViolation(String),
        }

        impl From<sqlx::Error> for PersistenceError {
            fn from(error: sqlx::Error) -> Self {
                Self::DatabaseFailed(error.to_string())
            }
        }
    }
}

#[path = "../src/persistence/stores/request_outcome_store.rs"]
mod request_outcome_store;
#[path = "../src/persistence/stores/request_cost_write.rs"]
mod request_cost_write;

use request_outcome_store::{AttemptCostWrite, RequestCostAggregateWrite, RequestOutcomeStore};
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

async fn seed_request_with_attempt(pool: &SqlitePool, request_id: &str, ordinals: &[u16]) {
    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, status,
            lifecycle_status, created_at
         ) VALUES (?, ?, '1', 'POST', '/v1/chat/completions',
                   '/v1/chat/completions', 'in_progress', 'admitted', '1')",
    )
    .bind(request_id)
    .bind(request_id)
    .execute(pool)
    .await
    .expect("request start");

    for ordinal in ordinals {
        sqlx::query(
            "INSERT INTO request_attempts (
                request_id, ordinal, station_id, station_key_id, endpoint_revision,
                started_at_ms, terminal_kind, health_effect, output_committed, terminal_at_ms
             ) VALUES (?, ?, 'station-a', 'key-a', 1, 2, 'succeeded', 'success', 1, 3)",
        )
        .bind(request_id)
        .bind(i64::from(*ordinal))
        .execute(pool)
        .await
        .expect("attempt");
    }
}

fn priced_attempt(request_id: &str, ordinal: u16, currency: &str, total: i64) -> AttemptCostWrite {
    AttemptCostWrite {
        request_id: request_id.to_string(),
        ordinal,
        pricing_context_id: format!("pricing-{request_id}-{ordinal}"),
        pricing_basis: "exact_price".to_string(),
        pricing_status_label: "exact".to_string(),
        usage_status: "complete".to_string(),
        input_tokens: Some(10),
        output_tokens: Some(5),
        total_tokens: Some(15),
        cache_creation_tokens: None,
        cache_read_tokens: None,
        cost_status: "priced".to_string(),
        currency: Some(currency.to_string()),
        total_cost_micro: Some(total),
        created_at_ms: 10 + i64::from(ordinal),
    }
}

fn unpriced_attempt(request_id: &str, ordinal: u16) -> AttemptCostWrite {
    AttemptCostWrite {
        request_id: request_id.to_string(),
        ordinal,
        pricing_context_id: format!("pricing-{request_id}-{ordinal}"),
        pricing_basis: "unpriced".to_string(),
        pricing_status_label: "unpriced".to_string(),
        usage_status: "missing_usage".to_string(),
        input_tokens: None,
        output_tokens: None,
        total_tokens: None,
        cache_creation_tokens: None,
        cache_read_tokens: None,
        cost_status: "missing_usage".to_string(),
        currency: None,
        total_cost_micro: None,
        created_at_ms: 10 + i64::from(ordinal),
    }
}

fn aggregate(request_id: &str, status: &str) -> RequestCostAggregateWrite {
    RequestCostAggregateWrite {
        request_id: request_id.to_string(),
        status: status.to_string(),
        totals_by_currency_json: r#"{"USD":300}"#.to_string(),
        compatibility_currency: if status == "complete_single_currency" {
            Some("USD".to_string())
        } else {
            None
        },
        compatibility_total_cost_micro: if status == "complete_single_currency" {
            Some(300)
        } else {
            None
        },
        incomplete_attempts_json: "[]".to_string(),
        written_at_ms: 50,
    }
}

#[tokio::test]
async fn attempt_cost_snapshot_is_inserted_once_and_replay_must_match() {
    let pool = test_pool().await;
    seed_request_with_attempt(&pool, "req-cost-replay", &[0]).await;
    let store = RequestOutcomeStore;
    let mut connection = pool.acquire().await.expect("connection");
    let cost = priced_attempt("req-cost-replay", 0, "USD", 100);

    let first = store
        .insert_attempt_cost(&mut connection, &cost)
        .await
        .expect("first cost");
    let duplicate = store
        .insert_attempt_cost(&mut connection, &cost)
        .await
        .expect("duplicate cost");
    assert!(first.inserted);
    assert!(!duplicate.inserted);

    let mut changed = cost.clone();
    changed.total_cost_micro = Some(101);
    let error = store
        .insert_attempt_cost(&mut connection, &changed)
        .await
        .expect_err("changed replay");
    assert!(error
        .to_string()
        .contains("duplicate attempt cost does not match"));
}

#[tokio::test]
async fn request_aggregate_fails_until_every_started_attempt_has_durable_cost() {
    let pool = test_pool().await;
    seed_request_with_attempt(&pool, "req-cost-missing", &[0, 1]).await;
    let store = RequestOutcomeStore;
    let mut connection = pool.acquire().await.expect("connection");
    store
        .insert_attempt_cost(
            &mut connection,
            &priced_attempt("req-cost-missing", 0, "USD", 100),
        )
        .await
        .expect("first cost");

    let error = store
        .insert_request_cost_aggregate(
            &mut connection,
            &aggregate("req-cost-missing", "complete_single_currency"),
        )
        .await
        .expect_err("missing second cost");
    assert!(error
        .to_string()
        .contains("requires all durable attempt costs"));
}

#[tokio::test]
async fn request_aggregate_projects_single_currency_compatibility_fields() {
    let pool = test_pool().await;
    seed_request_with_attempt(&pool, "req-cost-single", &[0, 1]).await;
    let store = RequestOutcomeStore;
    let mut connection = pool.acquire().await.expect("connection");
    store
        .insert_attempt_cost(
            &mut connection,
            &priced_attempt("req-cost-single", 0, "USD", 100),
        )
        .await
        .expect("first cost");
    store
        .insert_attempt_cost(
            &mut connection,
            &priced_attempt("req-cost-single", 1, "USD", 200),
        )
        .await
        .expect("second cost");
    let ack = store
        .insert_request_cost_aggregate(
            &mut connection,
            &aggregate("req-cost-single", "complete_single_currency"),
        )
        .await
        .expect("aggregate");
    assert!(ack.inserted);

    let row = sqlx::query(
        "SELECT cost_status, cost_currency, estimated_total_cost
         FROM request_logs WHERE request_id = ?",
    )
    .bind("req-cost-single")
    .fetch_one(&mut *connection)
    .await
    .expect("request log projection");
    assert_eq!(row.get::<String, _>(0), "complete_single_currency");
    assert_eq!(row.get::<String, _>(1), "USD");
    assert_eq!(row.get::<f64, _>(2), 0.0003);
}

#[tokio::test]
async fn mixed_or_incomplete_aggregate_does_not_forge_single_currency_projection() {
    let pool = test_pool().await;
    seed_request_with_attempt(&pool, "req-cost-mixed", &[0, 1]).await;
    let store = RequestOutcomeStore;
    let mut connection = pool.acquire().await.expect("connection");
    store
        .insert_attempt_cost(
            &mut connection,
            &priced_attempt("req-cost-mixed", 0, "USD", 100),
        )
        .await
        .expect("first cost");
    store
        .insert_attempt_cost(&mut connection, &unpriced_attempt("req-cost-mixed", 1))
        .await
        .expect("gap cost");
    store
        .insert_request_cost_aggregate(
            &mut connection,
            &RequestCostAggregateWrite {
                request_id: "req-cost-mixed".to_string(),
                status: "incomplete".to_string(),
                totals_by_currency_json: r#"{"USD":100}"#.to_string(),
                compatibility_currency: None,
                compatibility_total_cost_micro: None,
                incomplete_attempts_json:
                    r#"[{"attempt":"req-cost-mixed:1","status":"missing_usage"}]"#.to_string(),
                written_at_ms: 60,
            },
        )
        .await
        .expect("aggregate");

    let row = sqlx::query(
        "SELECT cost_status, cost_currency, estimated_total_cost
         FROM request_logs WHERE request_id = ?",
    )
    .bind("req-cost-mixed")
    .fetch_one(&mut *connection)
    .await
    .expect("request log projection");
    assert_eq!(row.get::<String, _>(0), "incomplete");
    assert_eq!(row.get::<Option<String>, _>(1), None);
    assert_eq!(row.get::<Option<f64>, _>(2), None);
}
