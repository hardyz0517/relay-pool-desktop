#[path = "../src/persistence/stores/monitoring/budgets.rs"]
pub mod budgets;
#[path = "../src/persistence/stores/monitoring/definitions.rs"]
pub mod definitions;
#[path = "../src/persistence/stores/monitoring/executions.rs"]
pub mod executions;
#[path = "../src/models/monitoring/mod.rs"]
pub mod monitoring_models;
#[path = "../src/persistence/error.rs"]
pub mod persistence_error;
#[path = "../src/persistence/stores/monitoring/retention.rs"]
pub mod retention;
#[path = "../src/persistence/stores/monitoring/status_read_repository.rs"]
pub mod status_queries;

mod models {
    pub(crate) mod monitoring {
        pub(crate) use crate::monitoring_models::*;
    }
}

mod persistence {
    pub mod error {
        pub(crate) use crate::persistence_error::PersistenceError;
    }

    pub(crate) struct ReadSession;

    impl ReadSession {
        pub(crate) fn connection(&mut self) -> &mut sqlx::SqliteConnection {
            unimplemented!("path-based repository tests do not construct ReadSession")
        }
    }
}

use budgets::MonitoringBudgetRepository;
use definitions::MonitoringDefinitionRepository;
use executions::{
    FinalizeTargetRow, MonitoringExecutionRepository, NewAttemptRow, NewExecutionRow,
};
use persistence::error::PersistenceError;
use retention::MonitoringRetentionRepository;
use sqlx::{Connection, Row, SqliteConnection};
use status_queries::MonitoringStatusQueryRepository;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn monitoring_repositories_append_attempts_idempotently_by_id_and_tuple() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    seed_monitor_execution(&mut connection, "execution-1", 1).await;

    let attempt = attempt("attempt-1", "execution-1", "key-1", 0);
    executions
        .append_attempt(&mut connection, &attempt)
        .await
        .expect("append attempt");
    executions
        .append_attempt(&mut connection, &attempt)
        .await
        .expect("id replay");

    let same_tuple_different_id = NewAttemptRow {
        id: "attempt-replay-other-id".to_string(),
        ..attempt.clone()
    };
    executions
        .append_attempt(&mut connection, &same_tuple_different_id)
        .await
        .expect("tuple replay does not duplicate");

    let count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_attempts")
        .fetch_one(&mut connection)
        .await
        .expect("attempt count");
    assert_eq!(count, 1);
}

#[tokio::test]
async fn monitoring_repositories_finalize_target_idempotently_and_rollback_invalid_ownership() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    seed_monitor_execution(&mut connection, "execution-1", 1).await;
    executions
        .append_attempt(
            &mut connection,
            &attempt("attempt-1", "execution-1", "key-1", 0),
        )
        .await
        .expect("attempt");

    let invalid = FinalizeTargetRow {
        attempt_count: 2,
        ..target("target-1", "execution-1", "key-1", "attempt-1", "available")
    };
    assert!(matches!(
        executions.finalize_target(&mut connection, &invalid).await,
        Err(PersistenceError::ConstraintViolation)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_target_results")
            .fetch_one(&mut connection)
            .await
            .expect("target count"),
        0
    );

    let valid = target("target-1", "execution-1", "key-1", "attempt-1", "available");
    executions
        .finalize_target(&mut connection, &valid)
        .await
        .expect("target finalize");
    executions
        .finalize_target(&mut connection, &valid)
        .await
        .expect("target replay");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_target_results")
            .fetch_one(&mut connection)
            .await
            .expect("target count"),
        1
    );
}

#[tokio::test]
async fn monitoring_repositories_finalize_skipped_target_without_fake_attempt() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    seed_monitor_execution(&mut connection, "execution-skipped", 1).await;

    let skipped = FinalizeTargetRow {
        terminal_outcome: "skipped".to_string(),
        terminal_failure_kind: Some("needs_configuration".to_string()),
        attempt_count: 0,
        decisive_attempt_id: None,
        ..target(
            "target-skipped",
            "execution-skipped",
            "key-1",
            "attempt-unused",
            "skipped",
        )
    };
    executions
        .finalize_target(&mut connection, &skipped)
        .await
        .expect("skipped target");
    let summary = executions
        .finalize_execution_and_advance_schedule(
            &mut connection,
            "execution-skipped",
            "monitor-1",
            100,
            Some(200),
        )
        .await
        .expect("finalize skipped execution");

    assert_eq!(summary.available_count, 0);
    assert_eq!(summary.skipped_count, 1);
    assert_eq!(summary.summary_outcome.as_deref(), Some("skipped"));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_attempts")
            .fetch_one(&mut connection)
            .await
            .expect("attempt count"),
        0
    );
}

#[tokio::test]
async fn monitoring_execution_finalization_requires_all_targets_and_replays_summary_and_due() {
    let mut connection = ready_connection().await;
    let executions = MonitoringExecutionRepository;
    seed_monitor_execution(&mut connection, "execution-1", 2).await;
    seed_key(&mut connection, "key-2").await;
    seed_finalized_target(&mut connection, "execution-1", "target-1", "key-1").await;

    assert!(matches!(
        executions
            .finalize_execution_and_advance_schedule(
                &mut connection,
                "execution-1",
                "monitor-1",
                20,
                Some(2_000)
            )
            .await,
        Err(PersistenceError::ConstraintViolation)
    ));

    executions
        .append_attempt(
            &mut connection,
            &attempt("attempt-key-2", "execution-1", "key-2", 0),
        )
        .await
        .expect("attempt key 2");
    executions
        .finalize_target(
            &mut connection,
            &target(
                "target-2",
                "execution-1",
                "key-2",
                "attempt-key-2",
                "unavailable",
            ),
        )
        .await
        .expect("target key 2");

    let summary = executions
        .finalize_execution_and_advance_schedule(
            &mut connection,
            "execution-1",
            "monitor-1",
            20,
            Some(2_000),
        )
        .await
        .expect("execution finalize");
    assert_eq!(summary.available_count, 1);
    assert_eq!(summary.unavailable_count, 1);
    assert_eq!(summary.summary_outcome.as_deref(), Some("degraded"));

    let replay = executions
        .finalize_execution_and_advance_schedule(
            &mut connection,
            "execution-1",
            "monitor-1",
            20,
            Some(2_000),
        )
        .await
        .expect("execution replay");
    assert_eq!(replay, summary);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("next due"),
        2_000
    );
}

#[tokio::test]
async fn monitoring_dirty_ranges_merge_without_rolling_back_completed_execution() {
    let mut connection = ready_connection().await;
    let retention = MonitoringRetentionRepository;
    let executions = MonitoringExecutionRepository;
    seed_finalized_target(&mut connection, "execution-1", "target-1", "key-1").await;
    executions
        .finalize_execution_and_advance_schedule(
            &mut connection,
            "execution-1",
            "monitor-1",
            20,
            Some(2_000),
        )
        .await
        .expect("execution finalize");

    retention
        .mark_dirty_range(
            &mut connection,
            "dirty-1",
            "monitor-1",
            Some("key-1"),
            10,
            20,
            "rollup_failed",
            30,
        )
        .await
        .expect("dirty range");
    retention
        .mark_dirty_range(
            &mut connection,
            "dirty-2",
            "monitor-1",
            Some("key-1"),
            15,
            25,
            "rollup_failed",
            31,
        )
        .await
        .expect("dirty merge");

    let dirty = sqlx::query(
        "SELECT COUNT(*) AS count, MIN(range_start_ms) AS start, MAX(range_end_ms) AS end
         FROM channel_monitor_rollup_dirty_ranges WHERE reason = 'rollup_failed'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("dirty count");
    assert_eq!(dirty.get::<i64, _>("count"), 1);
    assert_eq!(dirty.get::<i64, _>("start"), 10);
    assert_eq!(dirty.get::<i64, _>("end"), 25);
    assert_eq!(
        sqlx::query_scalar::<_, String>(
            "SELECT status FROM channel_monitor_executions WHERE id = 'execution-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("execution status"),
        "completed"
    );
}

#[tokio::test]
async fn monitoring_budget_reservation_is_atomic_and_never_exceeds_limit() {
    let mut connection = ready_connection().await;
    let budgets = MonitoringBudgetRepository;

    assert!(budgets
        .reserve_attempts(
            &mut connection,
            "budget-1",
            "monitor-1",
            Some("key-1"),
            0,
            86_400_000,
            2,
            3,
            10,
        )
        .await
        .expect("reserve first"));
    assert!(!budgets
        .reserve_attempts(
            &mut connection,
            "budget-2",
            "monitor-1",
            Some("key-1"),
            0,
            86_400_000,
            2,
            3,
            11,
        )
        .await
        .expect("reserve rejected"));
    assert!(budgets
        .reserve_attempts(
            &mut connection,
            "budget-3",
            "monitor-1",
            Some("key-1"),
            0,
            86_400_000,
            1,
            3,
            12,
        )
        .await
        .expect("reserve final"));

    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT attempt_count FROM channel_monitor_probe_budget_usage WHERE id = 'budget-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("budget count"),
        3
    );
}

#[tokio::test]
async fn startup_recovery_marks_queued_and_running_interrupted_without_replaying_network() {
    let mut connection = ready_connection().await;
    seed_monitor_execution_with_status(&mut connection, "queued-execution", 1, "queued", None)
        .await;
    seed_monitor_execution_with_status(&mut connection, "running-execution", 1, "running", Some(1))
        .await;
    seed_monitor_execution_with_status(
        &mut connection,
        "completed-execution",
        1,
        "completed",
        Some(1),
    )
    .await;

    let affected = MonitoringExecutionRepository
        .mark_startup_recovery_interrupted(&mut connection, 20_000)
        .await
        .expect("startup recovery");

    assert_eq!(affected, 2);
    assert_eq!(
        execution_status(&mut connection, "queued-execution").await,
        "interrupted"
    );
    assert_eq!(
        execution_status(&mut connection, "running-execution").await,
        "interrupted"
    );
    assert_eq!(
        execution_status(&mut connection, "completed-execution").await,
        "completed"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT next_due_at_ms FROM channel_monitors WHERE id = 'monitor-1'"
        )
        .fetch_one(&mut connection)
        .await
        .expect("next due"),
        80_000,
        "startup recovery moves the scheduled baseline forward instead of causing catch-up probes"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_attempts")
            .fetch_one(&mut connection)
            .await
            .expect("attempt count"),
        0,
        "startup recovery must not synthesize or replay network attempts"
    );

    assert_eq!(
        MonitoringExecutionRepository
            .mark_startup_recovery_interrupted(&mut connection, 21_000)
            .await
            .expect("idempotent recovery"),
        0
    );
}

#[tokio::test]
async fn monitoring_list_queries_are_bounded_cursor_stable_and_indexed() {
    let mut connection = ready_connection().await;
    let definitions = MonitoringDefinitionRepository;
    let statuses = MonitoringStatusQueryRepository;
    seed_finalized_target(&mut connection, "execution-1", "target-a", "key-1").await;
    sqlx::query(
        "UPDATE channel_monitor_target_results SET finished_at_ms = 100 WHERE id = 'target-a'",
    )
    .execute(&mut connection)
    .await
    .expect("target finished");
    seed_key(&mut connection, "key-2").await;
    seed_monitor_execution(&mut connection, "execution-2", 1).await;
    MonitoringExecutionRepository
        .append_attempt(
            &mut connection,
            &attempt("attempt-b", "execution-2", "key-2", 0),
        )
        .await
        .expect("attempt b");
    MonitoringExecutionRepository
        .finalize_target(
            &mut connection,
            &FinalizeTargetRow {
                id: "target-b".to_string(),
                finished_at_ms: Some(100),
                ..target("target-b", "execution-2", "key-2", "attempt-b", "available")
            },
        )
        .await
        .expect("target b");

    let due = definitions
        .list_due(&mut connection, 10_000, 10_000)
        .await
        .expect("due list");
    assert_eq!(due.len(), 1);
    assert_eq!(due[0].id, "monitor-1");

    let first = statuses
        .recent_target_results(&mut connection, "monitor-1", None, 1)
        .await
        .expect("recent first");
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].id, "target-b");
    let second = statuses
        .recent_target_results(
            &mut connection,
            "monitor-1",
            Some((first[0].finished_at_ms.unwrap(), first[0].id.clone())),
            1,
        )
        .await
        .expect("recent second");
    assert_eq!(second[0].id, "target-a");

    assert_plan_uses(
        &mut connection,
        "EXPLAIN QUERY PLAN SELECT id FROM channel_monitors WHERE enabled = 1 AND (next_due_at_ms IS NULL OR next_due_at_ms <= 10000) ORDER BY COALESCE(next_due_at_ms, 0) ASC, id ASC LIMIT 20",
        "idx_channel_monitors_v2_due",
    )
    .await;
    assert_plan_uses(
        &mut connection,
        "EXPLAIN QUERY PLAN SELECT id FROM channel_monitor_target_results WHERE monitor_id = 'monitor-1' ORDER BY finished_at_ms DESC, id DESC LIMIT 20",
        "idx_channel_monitor_target_results_monitor_finished",
    )
    .await;
}

#[tokio::test]
async fn monitoring_definition_config_loads_v2_fields_without_legacy_fallback_primary_guessing() {
    let mut connection = ready_connection().await;
    let definitions = MonitoringDefinitionRepository;
    sqlx::query(
        r#"
        UPDATE channel_monitors
        SET protocol_kind = 'open_ai_responses',
            client_profile_id = 'codex_cli_compat',
            client_profile_version = 3,
            primary_model = 'gpt-v2-primary',
            fallback_models_json = '["legacy-wrong-primary"]',
            fallback_models_v2_json = '["gpt-v2-fallback"]',
            retry_max_attempts_per_model = 2,
            retry_initial_backoff_ms = 300,
            retry_max_backoff_ms = 900,
            risk_daily_probe_budget = 77,
            health_writeback_mode = 'authoritative',
            health_failure_threshold = 4,
            health_recovery_threshold = 5,
            attempt_timeout_ms = 7000,
            execution_timeout_ms = 17000,
            schedule_revision = 9,
            next_due_at_ms = 12345
        WHERE id = 'monitor-1'
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("update v2 fields");

    let config = definitions
        .load_config(&mut connection, "monitor-1")
        .await
        .expect("load config");

    assert_eq!(config.protocol_kind, "open_ai_responses");
    assert_eq!(config.client_profile_id, "codex_cli_compat");
    assert_eq!(config.client_profile_version, 3);
    assert_eq!(config.primary_model, "gpt-v2-primary");
    assert_eq!(config.fallback_models_json, r#"["gpt-v2-fallback"]"#);
    assert_eq!(config.retry_max_attempts_per_model, 2);
    assert_eq!(config.retry_initial_backoff_ms, 300);
    assert_eq!(config.retry_max_backoff_ms, 900);
    assert_eq!(config.risk_daily_probe_budget, 77);
    assert_eq!(config.health_writeback_mode, "authoritative");
    assert_eq!(config.health_failure_threshold, 4);
    assert_eq!(config.health_recovery_threshold, 5);
    assert_eq!(config.attempt_timeout_ms, 7000);
    assert_eq!(config.execution_timeout_ms, 17000);
    assert_eq!(config.schedule_revision, 9);
    assert_eq!(config.next_due_at_ms, Some(12345));
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
    seed_station(connection).await;
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
            next_run_at, created_at, updated_at
        ) VALUES ('monitor-1', 'Primary', 'station_key', 'station-1', 'key-1', 'template-1',
                  1, 60, 5, 15, 1, 3, '["gpt-primary"]', '1000', '1', '1')
        "#,
    )
    .execute(connection)
    .await
    .expect("monitor");
}

async fn seed_station(connection: &mut SqliteConnection) {
    sqlx::query(
        r#"
        INSERT INTO stations (
            id, name, station_type, website_url, api_base_url, enabled, priority,
            credit_per_cny, collection_interval_minutes, status, created_at, updated_at
        ) VALUES ('station-1', 'Station', 'openai-compatible', 'https://example.test',
                  'https://example.test/v1', 1, 0, 1.0, 30, 'unchecked', '1', '1')
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("station");
    seed_key(connection, "key-1").await;
}

async fn seed_key(connection: &mut SqliteConnection, key_id: &str) {
    sqlx::query("INSERT OR IGNORE INTO station_keys (id, station_id) VALUES (?1, 'station-1')")
        .bind(key_id)
        .execute(connection)
        .await
        .expect("station key");
}

async fn seed_monitor_execution(
    connection: &mut SqliteConnection,
    execution_id: &str,
    target_count: i64,
) {
    seed_monitor_execution_with_status(connection, execution_id, target_count, "running", Some(1))
        .await;
}

async fn seed_monitor_execution_with_status(
    connection: &mut SqliteConnection,
    execution_id: &str,
    target_count: i64,
    status: &str,
    started_at_ms: Option<i64>,
) {
    MonitoringExecutionRepository
        .insert_execution(
            connection,
            &NewExecutionRow {
                id: execution_id.to_string(),
                monitor_id: "monitor-1".to_string(),
                trigger_kind: "manual".to_string(),
                trigger_request_id: None,
                status: status.to_string(),
                planned_at_ms: 1,
                started_at_ms,
                config_revision: 1,
                config_snapshot_hash: "hash".to_string(),
                endpoint_revision: 1,
                target_count,
                created_at_ms: 1,
            },
        )
        .await
        .expect("execution");
}

async fn execution_status(connection: &mut SqliteConnection, execution_id: &str) -> String {
    sqlx::query_scalar::<_, String>("SELECT status FROM channel_monitor_executions WHERE id = ?1")
        .bind(execution_id)
        .fetch_one(connection)
        .await
        .expect("execution status")
}

async fn seed_finalized_target(
    connection: &mut SqliteConnection,
    execution_id: &str,
    target_id: &str,
    key_id: &str,
) {
    seed_monitor_execution(connection, execution_id, 1).await;
    MonitoringExecutionRepository
        .append_attempt(connection, &attempt("attempt-1", execution_id, key_id, 0))
        .await
        .expect("attempt");
    MonitoringExecutionRepository
        .finalize_target(
            connection,
            &target(target_id, execution_id, key_id, "attempt-1", "available"),
        )
        .await
        .expect("target");
}

fn attempt(id: &str, execution_id: &str, key_id: &str, attempt_number: i64) -> NewAttemptRow {
    NewAttemptRow {
        id: id.to_string(),
        execution_id: execution_id.to_string(),
        monitor_id: "monitor-1".to_string(),
        station_id: "station-1".to_string(),
        station_key_id: key_id.to_string(),
        model: "gpt-primary".to_string(),
        model_role: "primary".to_string(),
        model_index: 0,
        attempt_number,
        protocol_kind: "generic_open_ai".to_string(),
        client_profile_id: "standard_api".to_string(),
        client_profile_version: 1,
        request_profile_hash: "hash".to_string(),
        transport_mode: "warm".to_string(),
        started_at_ms: 10,
        finished_at_ms: Some(20),
        latency_ms: Some(10),
        http_status: Some(200),
        outcome: "available".to_string(),
        failure_kind: None,
        retryable: false,
        response_model: Some("gpt-primary".to_string()),
        content_extracted: true,
        validation_passed: true,
        output_bytes: 12,
        error_summary: None,
        created_at_ms: 10,
    }
}

fn target(
    id: &str,
    execution_id: &str,
    key_id: &str,
    decisive_attempt_id: &str,
    outcome: &str,
) -> FinalizeTargetRow {
    FinalizeTargetRow {
        id: id.to_string(),
        execution_id: execution_id.to_string(),
        monitor_id: "monitor-1".to_string(),
        station_id: "station-1".to_string(),
        station_key_id: key_id.to_string(),
        terminal_outcome: outcome.to_string(),
        terminal_failure_kind: if outcome == "unavailable" {
            Some("server_error".to_string())
        } else {
            None
        },
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
        health_writeback_mode: "observe_only".to_string(),
        health_writeback_decision: "observe_only".to_string(),
        latency_ms: Some(10),
        semantic_confidence: "protocol_validated".to_string(),
        started_at_ms: 10,
        finished_at_ms: Some(20),
        created_at_ms: 20,
    }
}

async fn assert_plan_uses(connection: &mut SqliteConnection, sql: &str, expected_index: &str) {
    let details = sqlx::query(sql)
        .fetch_all(connection)
        .await
        .expect("query plan")
        .into_iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        details.contains(expected_index),
        "expected {expected_index} in plan:\n{details}"
    );
}
