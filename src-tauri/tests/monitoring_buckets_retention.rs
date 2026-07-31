#[path = "../src/application/monitoring/retention.rs"]
pub mod app_retention;
#[path = "../src/application/monitoring/buckets.rs"]
pub mod buckets;
#[path = "../src/services/monitoring/maintenance_policy.rs"]
pub mod maintenance;
#[path = "../src/models/monitoring/read_model.rs"]
pub mod monitoring_read_model;
#[path = "../src/persistence/error.rs"]
pub mod persistence_error;
#[path = "../src/persistence/stores/monitoring/retention.rs"]
pub mod retention;
#[path = "../src/persistence/stores/monitoring/status_queries.rs"]
pub mod status_queries;

mod models {
    pub(crate) mod monitoring {
        pub(crate) use crate::monitoring_read_model::*;
    }
}

mod persistence {
    pub mod error {
        pub(crate) use crate::persistence_error::PersistenceError;
    }
}

use app_retention::{RetentionPolicy, RetentionRunLimits};
use buckets::{
    hourly_bucket_windows, local_day_bucket_windows, recent_target_result_limit,
    BucketAvailabilityState, BucketCounts, BucketTimezoneSource,
};
use chrono::{TimeZone, Utc};
use maintenance::{MonitoringMaintenanceConfig, MonitoringMaintenanceState};
use retention::MonitoringRetentionRepository;
use sqlx::{Connection, SqliteConnection};
use status_queries::MonitoringStatusQueryRepository;
use tokio_util::sync::CancellationToken;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[test]
fn bucket_windows_include_current_period_dst_boundaries_and_utc_fallback() {
    let now_ms = Utc
        .with_ymd_and_hms(2026, 3, 8, 12, 0, 0)
        .single()
        .expect("valid utc")
        .timestamp_millis();
    let hourly = hourly_bucket_windows(now_ms, 24);
    assert_eq!(hourly.windows.len(), 24);
    assert_eq!(
        hourly.windows.last().expect("current hour").start_ms,
        Utc.with_ymd_and_hms(2026, 3, 8, 12, 0, 0)
            .single()
            .expect("hour")
            .timestamp_millis()
    );

    let spring_forward = local_day_bucket_windows(now_ms, 1, Some("America/New_York"));
    assert_eq!(spring_forward.timezone_id, "America/New_York");
    assert_eq!(spring_forward.timezone_source, BucketTimezoneSource::Iana);
    assert_eq!(spring_forward.windows.len(), 1);
    let spring_day = &spring_forward.windows[0];
    assert_eq!(
        spring_day.end_ms - spring_day.start_ms,
        23 * 60 * 60 * 1_000
    );

    let fall_back = local_day_bucket_windows(
        Utc.with_ymd_and_hms(2026, 11, 1, 12, 0, 0)
            .single()
            .expect("valid utc")
            .timestamp_millis(),
        1,
        Some("America/New_York"),
    );
    assert_eq!(
        fall_back.windows[0].end_ms - fall_back.windows[0].start_ms,
        25 * 60 * 60 * 1_000
    );

    let fallback = local_day_bucket_windows(now_ms, 7, Some("Invalid/Zone"));
    assert_eq!(fallback.timezone_id, "UTC");
    assert_eq!(
        fallback.timezone_source,
        BucketTimezoneSource::UtcFallback {
            requested: Some("Invalid/Zone".to_string())
        }
    );

    let cross_year = local_day_bucket_windows(
        Utc.with_ymd_and_hms(2027, 1, 1, 2, 0, 0)
            .single()
            .expect("valid utc")
            .timestamp_millis(),
        2,
        Some("UTC"),
    );
    assert_eq!(cross_year.windows[0].label, "12-31");
    assert_eq!(cross_year.windows[1].label, "01-01");
}

#[test]
fn bucket_counts_distinguish_missing_skipped_unavailable_and_degraded_weight() {
    assert_eq!(
        BucketCounts {
            available_count: 0,
            degraded_count: 0,
            unavailable_count: 0,
            skipped_count: 0
        }
        .state(),
        BucketAvailabilityState::Missing
    );
    assert_eq!(
        BucketCounts {
            available_count: 0,
            degraded_count: 0,
            unavailable_count: 0,
            skipped_count: 2
        }
        .state(),
        BucketAvailabilityState::SkippedOnly
    );
    let mixed = BucketCounts {
        available_count: 1,
        degraded_count: 1,
        unavailable_count: 1,
        skipped_count: 9,
    };
    assert_eq!(mixed.state(), BucketAvailabilityState::Degraded);
    assert_eq!(mixed.strict_availability_bps(), Some(3_333));
    assert_eq!(mixed.effective_availability_bps(5_000), Some(5_000));
}

#[tokio::test]
async fn recent_query_is_fixed_to_latest_60_target_results_not_attempts() {
    let mut connection = ready_connection().await;
    let statuses = MonitoringStatusQueryRepository;

    for index in 0..65 {
        seed_target_result(
            &mut connection,
            &format!("execution-{index:03}"),
            &format!("target-{index:03}"),
            10_000 + i64::from(index),
            "available",
            None,
            Some(10),
        )
        .await;
        seed_attempt(&mut connection, &format!("execution-{index:03}"), index, 0).await;
        seed_attempt(&mut connection, &format!("execution-{index:03}"), index, 1).await;
    }

    let recent = statuses
        .recent_target_results(&mut connection, "monitor-1", None, 500)
        .await
        .expect("recent target results");

    assert_eq!(recent.len(), recent_target_result_limit() as usize);
    assert_eq!(recent[0].id, "target-064");
    assert_eq!(recent.last().expect("oldest returned").id, "target-005");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_attempts")
            .fetch_one(&mut connection)
            .await
            .expect("attempt count"),
        130,
        "attempt retries remain traceable but do not add recent target cells"
    );
}

#[tokio::test]
async fn rollup_repair_is_idempotent_merges_dirty_ranges_and_keeps_skipped_out_of_denominator() {
    let mut connection = ready_connection().await;
    let retention = MonitoringRetentionRepository;
    let statuses = MonitoringStatusQueryRepository;
    seed_target_result(
        &mut connection,
        "execution-a",
        "target-a",
        3_600_000 + 10,
        "available",
        None,
        Some(100),
    )
    .await;
    seed_target_result(
        &mut connection,
        "execution-b",
        "target-b",
        3_600_000 + 20,
        "degraded",
        Some("timeout"),
        Some(200),
    )
    .await;
    seed_target_result(
        &mut connection,
        "execution-c",
        "target-c",
        3_600_000 + 30,
        "skipped",
        Some("needs_configuration"),
        None,
    )
    .await;

    retention
        .mark_dirty_range(
            &mut connection,
            "dirty-a",
            "monitor-1",
            Some("key-1"),
            3_600_000,
            3_600_100,
            "rollup_failed",
            10,
        )
        .await
        .expect("dirty a");
    retention
        .mark_dirty_range(
            &mut connection,
            "dirty-b",
            "monitor-1",
            Some("key-1"),
            3_600_050,
            3_600_200,
            "rollup_failed",
            11,
        )
        .await
        .expect("dirty b merges");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM channel_monitor_rollup_dirty_ranges")
            .fetch_one(&mut connection)
            .await
            .expect("dirty count"),
        1
    );

    let first_repair = retention
        .repair_dirty_ranges(&mut connection, 10, 99_000)
        .await
        .expect("repair");
    let second_repair = retention
        .repair_dirty_ranges(&mut connection, 10, 100_000)
        .await
        .expect("idempotent repair");
    assert_eq!(first_repair.repaired_ranges, 1);
    assert_eq!(second_repair.repaired_ranges, 0);

    let buckets = statuses
        .bucket_rollups(
            &mut connection,
            "monitor-1",
            Some("key-1"),
            "hour",
            3_600_000,
            7_200_000,
        )
        .await
        .expect("bucket rollups");
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].total_count, 2);
    assert_eq!(buckets[0].available_count, 1);
    assert_eq!(buckets[0].degraded_count, 1);
    assert_eq!(buckets[0].skipped_count, 1);
    assert_eq!(buckets[0].failure_counts.get("timeout"), Some(&1));
}

#[tokio::test]
async fn rollup_repair_rebuilds_complete_buckets_for_narrow_dirty_ranges() {
    let mut connection = ready_connection().await;
    let retention = MonitoringRetentionRepository;
    let statuses = MonitoringStatusQueryRepository;
    seed_target_result(
        &mut connection,
        "execution-a",
        "target-a",
        3_600_000 + 10,
        "available",
        None,
        Some(100),
    )
    .await;
    seed_target_result(
        &mut connection,
        "execution-b",
        "target-b",
        3_600_000 + 1_000,
        "unavailable",
        Some("timeout"),
        Some(200),
    )
    .await;

    retention
        .mark_dirty_range(
            &mut connection,
            "dirty-single-result",
            "monitor-1",
            Some("key-1"),
            3_600_000 + 1_000,
            3_600_000 + 1_001,
            "target_result_committed",
            10,
        )
        .await
        .expect("dirty single result");
    retention
        .repair_dirty_ranges(&mut connection, 10, 99_000)
        .await
        .expect("repair single result");

    let buckets = statuses
        .bucket_rollups(
            &mut connection,
            "monitor-1",
            Some("key-1"),
            "hour",
            3_600_000,
            7_200_000,
        )
        .await
        .expect("bucket rollups");
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].total_count, 2);
    assert_eq!(buckets[0].available_count, 1);
    assert_eq!(buckets[0].unavailable_count, 1);
    assert_eq!(buckets[0].failure_counts.get("timeout"), Some(&1));
}

#[tokio::test]
async fn corrupt_failure_counts_mark_dirty_and_do_not_return_damaged_counts() {
    let mut connection = ready_connection().await;
    let retention = MonitoringRetentionRepository;
    let statuses = MonitoringStatusQueryRepository;
    seed_target_result(
        &mut connection,
        "execution-a",
        "target-a",
        3_600_000 + 10,
        "unavailable",
        Some("server_error"),
        Some(100),
    )
    .await;
    retention
        .rebuild_rollups_for_range(
            &mut connection,
            "monitor-1",
            Some("key-1"),
            3_600_000,
            7_200_000,
            1,
        )
        .await
        .expect("rollup");

    sqlx::query("PRAGMA ignore_check_constraints = ON")
        .execute(&mut connection)
        .await
        .expect("ignore checks");
    sqlx::query("UPDATE channel_monitor_bucket_rollups SET failure_counts_json = 'not-json'")
        .execute(&mut connection)
        .await
        .expect("corrupt json");
    sqlx::query("PRAGMA ignore_check_constraints = OFF")
        .execute(&mut connection)
        .await
        .expect("restore checks");

    let dirty_count = retention
        .mark_corrupt_rollups_dirty(&mut connection, 2_000)
        .await
        .expect("mark corrupt");
    assert_eq!(
        dirty_count, 2,
        "repair produced hour and day rollups, and both corrupt summaries are marked dirty"
    );

    let buckets = statuses
        .bucket_rollups(
            &mut connection,
            "monitor-1",
            Some("key-1"),
            "hour",
            3_600_000,
            7_200_000,
        )
        .await
        .expect("read corrupt bucket");
    assert!(buckets[0].corrupt_failure_counts);
    assert!(buckets[0].dirty);
    assert!(buckets[0].failure_counts.is_empty());
}

#[tokio::test]
async fn retention_requires_rollup_and_skips_dirty_raw_sources() {
    let mut connection = ready_connection().await;
    let retention = MonitoringRetentionRepository;
    seed_target_result(
        &mut connection,
        "old-unrolled",
        "target-unrolled",
        1_000,
        "available",
        None,
        Some(10),
    )
    .await;
    assert_eq!(
        retention
            .delete_rolled_up_raw_executions(&mut connection, 10_000, 10, 10)
            .await
            .expect("no delete before rollup")
            .deleted_executions,
        0
    );

    retention
        .rebuild_rollups_for_range(
            &mut connection,
            "monitor-1",
            Some("key-1"),
            0,
            3_600_000,
            10_000,
        )
        .await
        .expect("rollup old");
    retention
        .mark_dirty_range(
            &mut connection,
            "dirty-old",
            "monitor-1",
            Some("key-1"),
            0,
            3_600_000,
            "repair_pending",
            10_000,
        )
        .await
        .expect("dirty old");
    assert_eq!(
        retention
            .delete_rolled_up_raw_executions(&mut connection, 10_000, 10, 10)
            .await
            .expect("no dirty delete")
            .deleted_executions,
        0
    );

    retention
        .repair_dirty_ranges(&mut connection, 10, 11_000)
        .await
        .expect("repair dirty");
    assert_eq!(
        retention
            .delete_rolled_up_raw_executions(&mut connection, 10_000, 10, 10)
            .await
            .expect("delete after clean rollup")
            .deleted_executions,
        1
    );
}

#[test]
fn maintenance_policy_has_startup_jitter_no_reentry_row_time_budget_and_cancellation() {
    RetentionPolicy::default()
        .validate()
        .expect("retention policy");
    RetentionRunLimits::default()
        .validate()
        .expect("run limits");

    let config = MonitoringMaintenanceConfig {
        startup_delay_ms: 1_000,
        startup_jitter_ms: 500,
        interval_ms: 60_000,
        row_budget: 10,
        time_budget_ms: 60_000,
    };
    config.validate().expect("maintenance config");
    let delay = config.deterministic_startup_delay(999);
    assert!(delay.as_millis() >= 1_000);
    assert!(delay.as_millis() <= 1_500);

    let state = MonitoringMaintenanceState::default();
    let guard = state.try_begin_cycle().expect("first cycle starts");
    assert!(state.try_begin_cycle().is_none(), "no reentry");
    let cancellation = CancellationToken::new();
    assert!(guard.should_continue(&cancellation, 9, &config));
    assert!(!guard.should_continue(&cancellation, 10, &config));
    cancellation.cancel();
    assert!(!guard.should_continue(&cancellation, 0, &config));
    drop(guard);
    assert!(
        state.try_begin_cycle().is_some(),
        "guard drop releases cycle"
    );
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
            credit_per_cny, collection_interval_minutes, status, created_at, updated_at
        ) VALUES ('station-1', 'Station', 'openai-compatible', 'https://example.test',
                  'https://example.test/v1', 1, 0, 1.0, 30, 'unchecked', '1', '1')
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
                  1, 300, 0, 15, 1, 3, '["gpt-primary"]', '999', '1', '1', 999)
        "#,
    )
    .execute(connection)
    .await
    .expect("monitor");
}

async fn seed_target_result(
    connection: &mut SqliteConnection,
    execution_id: &str,
    target_id: &str,
    finished_at_ms: i64,
    outcome: &str,
    failure_kind: Option<&str>,
    latency_ms: Option<i64>,
) {
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_executions (
            id, monitor_id, trigger_kind, status, planned_at_ms, started_at_ms,
            finished_at_ms, config_snapshot_hash, target_count, created_at_ms
        ) VALUES (?1, 'monitor-1', 'manual', 'completed', ?2, ?2, ?3, 'hash', 1, ?2)
        "#,
    )
    .bind(execution_id)
    .bind(finished_at_ms.saturating_sub(10))
    .bind(finished_at_ms)
    .execute(&mut *connection)
    .await
    .expect("execution");

    sqlx::query(
        r#"
        INSERT INTO channel_monitor_target_results (
            id, execution_id, monitor_id, station_id, station_key_id, endpoint_revision,
            terminal_outcome, terminal_failure_kind, requested_model, effective_model,
            used_fallback, attempt_count, decisive_attempt_id, protocol_kind,
            resolved_adapter_kind, client_profile_id, client_profile_version,
            request_profile_hash, traffic_equivalence, health_writeback_mode,
            health_writeback_decision, latency_ms, semantic_confidence,
            started_at_ms, finished_at_ms, created_at_ms
        ) VALUES (
            ?1, ?2, 'monitor-1', 'station-1', 'key-1', 1,
            ?3, ?4, 'gpt-primary', 'gpt-primary',
            0, 1, NULL, 'generic_open_ai',
            'generic_open_ai', 'standard_api', 1,
            'hash', 'standard_api', 'observe_only',
            'observe_only', ?5, 'protocol_validated',
            ?6, ?7, ?7
        )
        "#,
    )
    .bind(target_id)
    .bind(execution_id)
    .bind(outcome)
    .bind(failure_kind)
    .bind(latency_ms)
    .bind(finished_at_ms.saturating_sub(10))
    .bind(finished_at_ms)
    .execute(connection)
    .await
    .expect("target result");
}

async fn seed_attempt(
    connection: &mut SqliteConnection,
    execution_id: &str,
    execution_index: i32,
    attempt_number: i32,
) {
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_attempts (
            id, execution_id, monitor_id, station_id, station_key_id, model,
            model_role, model_index, attempt_number, protocol_kind,
            client_profile_id, client_profile_version, request_profile_hash,
            transport_mode, started_at_ms, finished_at_ms, latency_ms, http_status,
            outcome, retryable, response_model, content_extracted,
            validation_passed, output_bytes, created_at_ms
        ) VALUES (
            ?1, ?2, 'monitor-1', 'station-1', 'key-1', 'gpt-primary',
            'primary', 0, ?3, 'generic_open_ai',
            'standard_api', 1, 'hash',
            'warm', ?4, ?5, 10, 200,
            'available', 0, 'gpt-primary', 1,
            1, 12, ?4
        )
        "#,
    )
    .bind(format!("attempt-{execution_index:03}-{attempt_number}"))
    .bind(execution_id)
    .bind(i64::from(attempt_number))
    .bind(10_000 + i64::from(execution_index))
    .bind(10_010 + i64::from(execution_index))
    .execute(connection)
    .await
    .expect("attempt");
}
