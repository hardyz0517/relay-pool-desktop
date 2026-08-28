#[path = "../src/persistence/stores/monitoring/budgets.rs"]
pub mod budgets;
#[path = "../src/persistence/stores/monitoring/executions.rs"]
pub mod executions;
#[path = "../src/persistence/stores/health_observation_store.rs"]
pub mod health_observation_store;
#[path = "../src/application/health_transitions.rs"]
pub mod health_transitions;
#[path = "../src/models/health.rs"]
pub mod model_health;
#[path = "../src/persistence/error.rs"]
pub mod persistence_error;
#[path = "../src/persistence/stores/monitoring/retention.rs"]
pub mod retention;

mod models {
    pub(crate) mod health {
        pub(crate) use crate::model_health::*;
    }
}

mod persistence {
    pub(crate) struct WriteSession {
        connection: *mut sqlx::SqliteConnection,
    }

    impl WriteSession {
        pub(crate) fn new(connection: &mut sqlx::SqliteConnection) -> Self {
            Self { connection }
        }

        pub(crate) fn connection(&mut self) -> &mut sqlx::SqliteConnection {
            // SAFETY: test-local wrapper is created from one mutable connection
            // borrow and is used synchronously by the included application code.
            unsafe { &mut *self.connection }
        }
    }

    pub(crate) mod error {
        pub(crate) use crate::persistence_error::*;
    }

    pub(crate) mod stores {
        pub(crate) mod health_observation_store {
            pub(crate) use crate::health_observation_store::*;
        }

        pub(crate) mod monitoring {
            pub(crate) mod retention {
                pub(crate) use crate::retention::*;
            }
        }
    }
}

use budgets::MonitoringBudgetRepository;
use executions::{
    FinalizeTargetRow, MonitoringExecutionRepository, NewAttemptRow, NewExecutionRow,
};
use health_transitions::HealthTransitionService;
use model_health::{
    HealthObservation, HealthObservationOutcome, HealthObservationSource, HealthWritebackMode,
    TrafficEquivalence,
};
use persistence::error::PersistenceError;
use retention::MonitoringRetentionRepository;
use sqlx::{Connection, Row, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

async fn record_observation(
    service: &HealthTransitionService,
    connection: &mut SqliteConnection,
    observation: HealthObservation,
) -> Result<health_transitions::HealthTransitionAck, PersistenceError> {
    let mut write = persistence::WriteSession::new(connection);
    service.record_observation(&mut write, observation).await
}

#[tokio::test]
async fn transaction_rollback_covers_attempt_target_health_execution_rollup_and_budget_phases() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    let retention = MonitoringRetentionRepository;
    let budgets = MonitoringBudgetRepository;
    let health = HealthTransitionService::new();

    let mut tx = connection.begin().await.expect("begin fault tx");
    executions
        .insert_execution(&mut tx, &execution("execution-rollback", "running", 1))
        .await
        .expect("execution append");
    executions
        .append_attempt(&mut tx, &attempt("attempt-rollback", "execution-rollback"))
        .await
        .expect("attempt append");
    executions
        .finalize_target(
            &mut tx,
            &target(
                "target-rollback",
                "execution-rollback",
                "attempt-rollback",
                "available",
            ),
        )
        .await
        .expect("target finalization");
    record_observation(
        &health,
        &mut *tx,
        observation("observation-rollback", Some("target-rollback"), 1_120),
    )
    .await
    .expect("health observation");
    executions
        .finalize_execution_and_advance_schedule(
            &mut tx,
            "execution-rollback",
            "monitor-1",
            1_120,
            Some(2_000),
        )
        .await
        .expect("execution finalization");
    retention
        .mark_dirty_range(
            &mut tx,
            "dirty-rollback",
            "monitor-1",
            Some("key-1"),
            1_000,
            1_121,
            "fault_matrix_rollback",
            1_120,
        )
        .await
        .expect("rollup dirty range");
    assert!(budgets
        .reserve_attempts(
            &mut tx,
            "budget-rollback",
            "monitor-1",
            Some("key-1"),
            0,
            86_400_000,
            1,
            10,
            1_120,
        )
        .await
        .expect("budget reservation"));
    tx.rollback().await.expect("rollback fault tx");

    assert_eq!(
        count(&mut connection, "channel_monitor_executions").await,
        0
    );
    assert_eq!(count(&mut connection, "channel_monitor_attempts").await, 0);
    assert_eq!(
        count(&mut connection, "channel_monitor_target_results").await,
        0
    );
    assert_eq!(
        count(&mut connection, "station_key_health_observations").await,
        0
    );
    assert_eq!(
        count(&mut connection, "channel_monitor_rollup_dirty_ranges").await,
        0
    );
    assert_eq!(
        count(&mut connection, "channel_monitor_probe_budget_usage").await,
        0
    );
    assert_eq!(next_due_at_ms(&mut connection).await, 1_000);
}

#[tokio::test]
async fn commit_outcome_unknown_replay_is_idempotent_for_budget_and_health_observation() {
    let mut connection = ready_connection().await;
    let budgets = MonitoringBudgetRepository;
    let health = HealthTransitionService::new();

    for _ in 0..2 {
        let mut tx = connection.begin().await.expect("begin replay tx");
        assert!(budgets
            .reserve_attempts(
                &mut tx,
                "budget-replay",
                "monitor-1",
                Some("key-1"),
                0,
                86_400_000,
                1,
                10,
                1_000,
            )
            .await
            .expect("budget replay"));
        record_observation(
            &health,
            &mut *tx,
            observation("observation-replay", None, 1_000),
        )
        .await
        .expect("health replay");
        tx.commit().await.expect("commit replay tx");
    }

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT attempt_count FROM channel_monitor_probe_budget_usage WHERE id = 'budget-replay'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("budget count"),
        1
    );
    assert_eq!(
        count(&mut connection, "station_key_health_observations").await,
        1
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT success_count FROM routing_health_snapshot WHERE station_key_id = 'key-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("health success count"),
        1
    );
}

#[tokio::test]
async fn permanent_target_finalization_fault_leaves_no_partial_target_or_execution_summary() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    executions
        .insert_execution(
            &mut connection,
            &execution("execution-invalid", "running", 1),
        )
        .await
        .expect("execution");
    executions
        .append_attempt(
            &mut connection,
            &attempt("attempt-invalid", "execution-invalid"),
        )
        .await
        .expect("attempt");

    let invalid = FinalizeTargetRow {
        attempt_count: 2,
        ..target(
            "target-invalid",
            "execution-invalid",
            "attempt-invalid",
            "available",
        )
    };
    assert!(matches!(
        executions.finalize_target(&mut connection, &invalid).await,
        Err(PersistenceError::ConstraintViolation)
    ));
    assert_eq!(
        count(&mut connection, "channel_monitor_target_results").await,
        0
    );
    assert!(matches!(
        executions
            .finalize_execution_and_advance_schedule(
                &mut connection,
                "execution-invalid",
                "monitor-1",
                1_120,
                Some(2_000)
            )
            .await,
        Err(PersistenceError::ConstraintViolation)
    ));
    assert_eq!(
        execution_status(&mut connection, "execution-invalid").await,
        "running"
    );
    assert_eq!(next_due_at_ms(&mut connection).await, 1_000);
}

#[tokio::test]
async fn hard_restart_interrupts_running_without_fake_failure_network_replay_or_budget_refund() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    let budgets = MonitoringBudgetRepository;
    executions
        .insert_execution(
            &mut connection,
            &execution("execution-running", "running", 1),
        )
        .await
        .expect("running execution");
    assert!(budgets
        .reserve_attempts(
            &mut connection,
            "budget-consumed",
            "monitor-1",
            Some("key-1"),
            0,
            86_400_000,
            1,
            10,
            1_000,
        )
        .await
        .expect("reserve consumed budget"));

    assert_eq!(
        executions
            .mark_startup_recovery_interrupted(&mut connection, 10_000)
            .await
            .expect("startup recovery"),
        1
    );

    let row = sqlx::query(
        "SELECT status, summary_failure_kind FROM channel_monitor_executions WHERE id = 'execution-running'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("execution row");
    assert_eq!(row.get::<String, _>("status"), "interrupted");
    assert_eq!(
        row.get::<Option<String>, _>("summary_failure_kind")
            .as_deref(),
        Some("interrupted")
    );
    assert_eq!(
        count(&mut connection, "channel_monitor_attempts").await,
        0,
        "startup recovery must not synthesize or replay probe attempts"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT attempt_count FROM channel_monitor_probe_budget_usage WHERE id = 'budget-consumed'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("budget remains consumed"),
        1,
        "reserved budget remains consumed across hard-kill recovery"
    );
    assert_eq!(next_due_at_ms(&mut connection).await, 70_000);
}

async fn ready_connection() -> SqliteConnection {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    MIGRATOR
        .run(&mut connection)
        .await
        .expect("fresh migrations");
    seed_station_monitor(&mut connection).await;
    connection
}

async fn seed_station_monitor(connection: &mut SqliteConnection) {
    sqlx::query(
        r#"
        INSERT INTO stations (
            id, name, station_type, website_url, api_base_url, enabled, priority,
            credit_per_cny, collection_interval_minutes, status, created_at, updated_at,
            endpoint_revision
        ) VALUES ('station-1', 'Station', 'openai-compatible', 'https://example.test',
                  'https://example.test/v1', 1, 0, 1.0, 30, 'unchecked', '1', '1', 1)
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("station");
    sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', 'station-1')")
        .execute(&mut *connection)
        .await
        .expect("station key");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_request_templates (
            id, name, endpoint_kind, method, path, request_body_json,
            enabled, built_in, created_at, updated_at
        ) VALUES ('template-1', 'Chat', 'chat', 'POST', '/v1/chat/completions', '{}', 1, 0, '1', '1')
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("template");
    sqlx::query(
        r#"
        INSERT INTO channel_monitors (
            id, name, target_type, station_id, station_key_id, template_id,
            enabled, interval_seconds, jitter_seconds, timeout_seconds,
            max_concurrency, consecutive_failure_threshold, fallback_models_json,
            next_run_at, created_at, updated_at, next_due_at_ms
        ) VALUES ('monitor-1', 'Primary', 'station_key', 'station-1', 'key-1', 'template-1',
                  1, 60, 0, 15, 1, 3, '["gpt-primary"]', '1000', '1', '1', 1000)
        "#,
    )
    .execute(connection)
    .await
    .expect("monitor");
}

fn execution(id: &str, status: &str, target_count: i64) -> NewExecutionRow {
    NewExecutionRow {
        id: id.to_string(),
        monitor_id: "monitor-1".to_string(),
        trigger_kind: "manual".to_string(),
        trigger_request_id: None,
        status: status.to_string(),
        planned_at_ms: 1_000,
        started_at_ms: Some(1_000),
        config_revision: 1,
        config_snapshot_hash: "hash".to_string(),
        endpoint_revision: 1,
        target_count,
        created_at_ms: 1_000,
    }
}

fn attempt(id: &str, execution_id: &str) -> NewAttemptRow {
    NewAttemptRow {
        id: id.to_string(),
        execution_id: execution_id.to_string(),
        monitor_id: "monitor-1".to_string(),
        station_id: "station-1".to_string(),
        station_key_id: "key-1".to_string(),
        model: "gpt-primary".to_string(),
        model_role: "primary".to_string(),
        model_index: 0,
        attempt_number: 0,
        protocol_kind: "generic_open_ai".to_string(),
        client_profile_id: "standard_api".to_string(),
        client_profile_version: 1,
        request_profile_hash: "hash".to_string(),
        transport_mode: "warm".to_string(),
        started_at_ms: 1_000,
        finished_at_ms: Some(1_120),
        latency_ms: Some(120),
        ttfb_ms: Some(40),
        first_content_ms: Some(55),
        http_status: Some(200),
        outcome: "available".to_string(),
        failure_kind: None,
        retryable: false,
        response_model: Some("gpt-primary".to_string()),
        content_extracted: true,
        validation_passed: true,
        output_bytes: 12,
        error_summary: None,
        canonical_failure_class: None,
        failure_origin: None,
        failure_scope_kind: None,
        failure_dimension: None,
        evidence_code: None,
        evidence_confidence: None,
        classifier_profile_version: None,
        created_at_ms: 1_000,
    }
}

fn target(
    id: &str,
    execution_id: &str,
    decisive_attempt_id: &str,
    outcome: &str,
) -> FinalizeTargetRow {
    FinalizeTargetRow {
        id: id.to_string(),
        execution_id: execution_id.to_string(),
        monitor_id: "monitor-1".to_string(),
        station_id: "station-1".to_string(),
        station_key_id: "key-1".to_string(),
        terminal_outcome: outcome.to_string(),
        terminal_failure_kind: None,
        requested_model: "gpt-primary".to_string(),
        effective_model: Some("gpt-primary".to_string()),
        used_fallback: false,
        attempt_count: 1,
        decisive_attempt_id: Some(decisive_attempt_id.to_string()),
        protocol_kind: Some("generic_open_ai".to_string()),
        resolved_adapter_kind: "generic_open_ai".to_string(),
        client_profile_id: "standard_api".to_string(),
        client_profile_version: 1,
        request_profile_hash: Some("hash".to_string()),
        traffic_equivalence: "standard_api".to_string(),
        latency_ms: Some(120),
        ttfb_ms: Some(40),
        first_content_ms: Some(55),
        semantic_confidence: "protocol_validated".to_string(),
        availability_eligible: true,
        latency_eligible: true,
        started_at_ms: 1_000,
        finished_at_ms: Some(1_120),
        exclusion_reason: None,
        technical_health_effect: "positive".to_string(),
        disposition_profile_version: "v1".to_string(),
        created_at_ms: 1_120,
    }
}

fn observation(id: &str, target_result_id: Option<&str>, observed_at_ms: i64) -> HealthObservation {
    HealthObservation {
        id: id.to_string(),
        station_key_id: "key-1".to_string(),
        target_result_id: target_result_id.map(str::to_string),
        source: HealthObservationSource::SyntheticMonitor,
        source_event_id: "execution-1".to_string(),
        observed_at_ms,
        endpoint_revision: 1,
        outcome: HealthObservationOutcome::Success,
        failure_kind: None,
        latency_ms: Some(120),
        retry_after_ms: None,
        error_summary: None,
        writeback_mode: HealthWritebackMode::Authoritative,
        traffic_equivalence: TrafficEquivalence::SyntheticStandard,
    }
}

async fn execution_status(connection: &mut SqliteConnection, execution_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM channel_monitor_executions WHERE id = ?1")
        .bind(execution_id)
        .fetch_one(connection)
        .await
        .expect("execution status")
}

async fn next_due_at_ms(connection: &mut SqliteConnection) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'",
    )
    .fetch_one(connection)
    .await
    .expect("next due")
}

async fn count(connection: &mut SqliteConnection, table: &str) -> i64 {
    let sql = format!("SELECT COUNT(*) AS count FROM {table}");
    sqlx::query(&sql)
        .fetch_one(connection)
        .await
        .expect("count")
        .get("count")
}
