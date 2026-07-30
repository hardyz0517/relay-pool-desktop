#[path = "../src/models/health.rs"]
mod health;
#[path = "../src/persistence/stores/health_observation_store.rs"]
mod health_observation_store;
#[path = "../src/application/health_transitions.rs"]
mod health_transitions;
#[path = "../src/persistence/error.rs"]
mod persistence_error;

mod models {
    pub(crate) mod health {
        pub(crate) use crate::health::*;
    }
}

mod persistence {
    pub(crate) mod error {
        pub(crate) use crate::persistence_error::*;
    }

    pub(crate) mod stores {
        pub(crate) mod health_observation_store {
            pub(crate) use crate::health_observation_store::*;
        }
    }
}

use health::{
    HealthObservation, HealthObservationOutcome, HealthObservationSource, HealthWritebackMode,
    TrafficEquivalence,
};
use health_transitions::{HealthTransitionService, HealthWritebackDecision};
use persistence::error::PersistenceError;
use sqlx::{Connection, Row, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn proxy_synthetic_and_manual_share_one_observation_contract() {
    let mut connection = ready_connection().await;
    let service = HealthTransitionService::new();
    seed_station_monitor_target(&mut connection, "target-standard", "key-1").await;

    let proxy_ack = service
        .record_observation(
            &mut connection,
            observation(
                "proxy-success",
                HealthObservationSource::ProxyRequest,
                "proxy:req-1:0",
                HealthObservationOutcome::Success,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                1_000,
            ),
        )
        .await
        .expect("proxy observation");
    assert!(proxy_ack.observation_inserted);
    assert!(proxy_ack.health_applied);
    assert_eq!(proxy_ack.writeback_decision, HealthWritebackDecision::Write);

    let synthetic_ack = service
        .record_observation(
            &mut connection,
            observation(
                "synthetic-cooldown",
                HealthObservationSource::SyntheticMonitor,
                "target-standard",
                HealthObservationOutcome::Cooldown,
                TrafficEquivalence::SyntheticStandard,
                HealthWritebackMode::Authoritative,
                Some("target-standard"),
                2_000,
            )
            .with_retry_after(45_000)
            .with_error("rate limited"),
        )
        .await
        .expect("synthetic observation");
    assert!(synthetic_ack.observation_inserted);
    assert!(synthetic_ack.health_applied);
    assert_eq!(
        synthetic_ack.writeback_decision,
        HealthWritebackDecision::Write
    );

    let manual_ack = service
        .record_observation(
            &mut connection,
            observation(
                "manual-diagnostic",
                HealthObservationSource::ManualConnectivity,
                "manual:station-1:key-1:probe-1",
                HealthObservationOutcome::ObserveFailure,
                TrafficEquivalence::Diagnostic,
                HealthWritebackMode::Authoritative,
                None,
                3_000,
            )
            .with_error("manual connectivity failed"),
        )
        .await
        .expect("manual observation");
    assert!(manual_ack.observation_inserted);
    assert!(!manual_ack.health_applied);
    assert_eq!(
        manual_ack.writeback_decision,
        HealthWritebackDecision::Suppressed
    );

    let rows = sqlx::query(
        "SELECT source, source_event_id, target_result_id, outcome, traffic_equivalence, writeback_decision
         FROM station_key_health_observations
         ORDER BY observed_at_ms ASC",
    )
    .fetch_all(&mut connection)
    .await
    .expect("observation rows");
    assert_eq!(rows.len(), 3);
    assert_observation_row(
        &rows[0],
        "proxy",
        "proxy:req-1:0",
        None,
        "success",
        "real_user_traffic",
        "write",
    );
    assert_observation_row(
        &rows[1],
        "monitoring",
        "target-standard",
        Some("target-standard"),
        "cooldown",
        "synthetic_standard",
        "write",
    );
    assert_observation_row(
        &rows[2],
        "manual",
        "manual:station-1:key-1:probe-1",
        None,
        "observe_failure",
        "diagnostic",
        "suppressed",
    );

    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("success_count"), 1);
    assert_eq!(health.get::<i64, _>("failure_count"), 1);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 1);
    assert_eq!(
        health
            .get::<Option<String>, _>("last_error_summary")
            .as_deref(),
        Some("rate limited")
    );
    assert_eq!(
        health.get::<Option<String>, _>("cooldown_until").as_deref(),
        Some("47000")
    );

    let status = station_key_status(&mut connection).await;
    assert_eq!(status.as_deref(), Some("error"));
}

#[tokio::test]
async fn duplicate_source_event_is_exactly_once_for_observation_and_health() {
    let mut connection = ready_connection().await;
    let service = HealthTransitionService::new();

    let first = service
        .record_observation(
            &mut connection,
            observation(
                "proxy-failure",
                HealthObservationSource::ProxyRequest,
                "proxy:req-dup:0",
                HealthObservationOutcome::ObserveFailure,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                1_000,
            )
            .with_error("network timeout"),
        )
        .await
        .expect("first observation");
    let replay = service
        .record_observation(
            &mut connection,
            observation(
                "proxy-failure",
                HealthObservationSource::ProxyRequest,
                "proxy:req-dup:0",
                HealthObservationOutcome::ObserveFailure,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                1_000,
            )
            .with_error("network timeout"),
        )
        .await
        .expect("replayed observation");

    assert!(first.observation_inserted);
    assert!(first.health_applied);
    assert!(!replay.observation_inserted);
    assert!(!replay.health_applied);

    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM station_key_health_observations")
            .fetch_one(&mut connection)
            .await
            .expect("observation count"),
        1
    );
    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("failure_count"), 1);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 1);
}

#[tokio::test]
async fn outcome_matrix_handles_recovery_threshold_cooldown_and_non_applicable_rows() {
    let mut connection = ready_connection().await;
    let service = HealthTransitionService::new();

    for index in 0..3 {
        let ack = service
            .record_observation(
                &mut connection,
                observation(
                    &format!("proxy-observe-failure-{index}"),
                    HealthObservationSource::ProxyRequest,
                    &format!("proxy:req-failure:{index}"),
                    HealthObservationOutcome::ObserveFailure,
                    TrafficEquivalence::RealUserTraffic,
                    HealthWritebackMode::Authoritative,
                    None,
                    1_000 + index,
                )
                .with_error("upstream unavailable"),
            )
            .await
            .expect("observe failure");
        assert!(ack.health_applied);
    }

    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("failure_count"), 3);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 3);
    assert_eq!(
        health.get::<Option<String>, _>("cooldown_until").as_deref(),
        Some("121002")
    );

    let success = service
        .record_observation(
            &mut connection,
            observation(
                "proxy-recovery",
                HealthObservationSource::ProxyRequest,
                "proxy:req-recovery:0",
                HealthObservationOutcome::Success,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                2_000,
            ),
        )
        .await
        .expect("recovery");
    assert!(success.health_applied);
    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("success_count"), 1);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 0);
    assert_eq!(health.get::<Option<String>, _>("cooldown_until"), None);
    assert_eq!(
        station_key_status(&mut connection).await.as_deref(),
        Some("healthy")
    );

    let hard_fail = service
        .record_observation(
            &mut connection,
            observation(
                "proxy-auth-hard-fail",
                HealthObservationSource::ProxyRequest,
                "proxy:req-auth:0",
                HealthObservationOutcome::HardFail,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                3_000,
            )
            .with_error("authentication failed"),
        )
        .await
        .expect("hard fail");
    assert!(hard_fail.health_applied);
    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("failure_count"), 4);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 1);
    assert_eq!(
        health.get::<Option<String>, _>("cooldown_until").as_deref(),
        Some("903000")
    );
    assert_eq!(
        health
            .get::<Option<String>, _>("last_error_summary")
            .as_deref(),
        Some("authentication failed")
    );

    for (id, outcome) in [
        ("proxy-skipped", HealthObservationOutcome::Skipped),
        ("proxy-neutral", HealthObservationOutcome::Neutral),
    ] {
        let ack = service
            .record_observation(
                &mut connection,
                observation(
                    id,
                    HealthObservationSource::ProxyRequest,
                    id,
                    outcome,
                    TrafficEquivalence::RealUserTraffic,
                    HealthWritebackMode::Authoritative,
                    None,
                    4_000,
                ),
            )
            .await
            .expect("non-applicable observation");
        assert!(ack.observation_inserted);
        assert!(!ack.health_applied);
        assert_eq!(
            ack.writeback_decision,
            HealthWritebackDecision::NotApplicable
        );
    }

    let after_non_applicable = station_key_health(&mut connection).await;
    assert_eq!(after_non_applicable.get::<i64, _>("failure_count"), 4);
    assert_eq!(
        after_non_applicable
            .get::<Option<String>, _>("cooldown_until")
            .as_deref(),
        Some("903000")
    );
}

#[tokio::test]
async fn endpoint_revision_mismatch_fails_closed_and_revision_change_resets_health_snapshot() {
    let mut connection = ready_connection().await;
    let service = HealthTransitionService::new();

    let stale = service
        .record_observation(
            &mut connection,
            observation(
                "stale-revision",
                HealthObservationSource::ProxyRequest,
                "proxy:req-stale:0",
                HealthObservationOutcome::Success,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                1_000,
            )
            .with_endpoint_revision(2),
        )
        .await;
    assert!(matches!(stale, Err(PersistenceError::NotFound)));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM station_key_health_observations")
            .fetch_one(&mut connection)
            .await
            .expect("observation count"),
        0
    );

    service
        .record_observation(
            &mut connection,
            observation(
                "revision-one-success",
                HealthObservationSource::ProxyRequest,
                "proxy:req-r1:0",
                HealthObservationOutcome::Success,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                1_000,
            ),
        )
        .await
        .expect("revision 1 health");
    sqlx::query("UPDATE stations SET endpoint_revision = 2 WHERE id = 'station-1'")
        .execute(&mut connection)
        .await
        .expect("endpoint revision update");

    service
        .record_observation(
            &mut connection,
            observation(
                "revision-two-failure",
                HealthObservationSource::ProxyRequest,
                "proxy:req-r2:0",
                HealthObservationOutcome::ObserveFailure,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                2_000,
            )
            .with_endpoint_revision(2)
            .with_error("new endpoint failed"),
        )
        .await
        .expect("revision 2 health");

    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("endpoint_revision"), 2);
    assert_eq!(health.get::<i64, _>("success_count"), 0);
    assert_eq!(health.get::<i64, _>("failure_count"), 1);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 1);
}

#[tokio::test]
async fn cli_compat_and_observe_only_probes_are_recorded_without_route_health_writeback() {
    let mut connection = ready_connection().await;
    let service = HealthTransitionService::new();
    seed_station_monitor_target(&mut connection, "target-cli", "key-1").await;
    seed_station_monitor_target(&mut connection, "target-observe-only", "key-1").await;

    service
        .record_observation(
            &mut connection,
            observation(
                "baseline-proxy-success",
                HealthObservationSource::ProxyRequest,
                "proxy:req-baseline:0",
                HealthObservationOutcome::Success,
                TrafficEquivalence::RealUserTraffic,
                HealthWritebackMode::Authoritative,
                None,
                1_000,
            ),
        )
        .await
        .expect("baseline success");

    let cli_ack = service
        .record_observation(
            &mut connection,
            observation(
                "synthetic-cli-auth-failure",
                HealthObservationSource::SyntheticMonitor,
                "target-cli",
                HealthObservationOutcome::HardFail,
                TrafficEquivalence::SyntheticCliCompat,
                HealthWritebackMode::Authoritative,
                Some("target-cli"),
                2_000,
            )
            .with_error("cli profile auth failed"),
        )
        .await
        .expect("cli compat observation");
    assert!(cli_ack.observation_inserted);
    assert!(!cli_ack.health_applied);
    assert_eq!(
        cli_ack.writeback_decision,
        HealthWritebackDecision::Suppressed
    );

    let observe_only_ack = service
        .record_observation(
            &mut connection,
            observation(
                "synthetic-observe-only-failure",
                HealthObservationSource::SyntheticMonitor,
                "target-observe-only",
                HealthObservationOutcome::HardFail,
                TrafficEquivalence::SyntheticStandard,
                HealthWritebackMode::ObserveOnly,
                Some("target-observe-only"),
                3_000,
            )
            .with_error("observe only hard fail"),
        )
        .await
        .expect("observe-only observation");
    assert!(observe_only_ack.observation_inserted);
    assert!(!observe_only_ack.health_applied);
    assert_eq!(
        observe_only_ack.writeback_decision,
        HealthWritebackDecision::ObserveOnly
    );

    let health = station_key_health(&mut connection).await;
    assert_eq!(health.get::<i64, _>("success_count"), 1);
    assert_eq!(health.get::<i64, _>("failure_count"), 0);
    assert_eq!(health.get::<i64, _>("consecutive_failures"), 0);
    assert_eq!(health.get::<Option<String>, _>("cooldown_until"), None);
    assert_eq!(
        station_key_status(&mut connection).await.as_deref(),
        Some("healthy")
    );

    let decisions = sqlx::query(
        "SELECT source_event_id, writeback_decision FROM station_key_health_observations",
    )
    .fetch_all(&mut connection)
    .await
    .expect("writeback decisions");
    assert!(decisions.iter().any(|row| {
        row.get::<String, _>("source_event_id") == "target-cli"
            && row.get::<String, _>("writeback_decision") == "suppressed"
    }));
    assert!(decisions.iter().any(|row| {
        row.get::<String, _>("source_event_id") == "target-observe-only"
            && row.get::<String, _>("writeback_decision") == "observe_only"
    }));
}

async fn ready_connection() -> SqliteConnection {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("sqlite memory connection");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    MIGRATOR
        .run(&mut connection)
        .await
        .expect("fresh migrations");
    seed_station(&mut connection).await;
    connection
}

async fn seed_station(connection: &mut SqliteConnection) {
    sqlx::query(
        r#"
        INSERT INTO stations (
            id, name, station_type, website_url, api_base_url, endpoint_revision,
            enabled, priority, credit_per_cny, collection_interval_minutes,
            status, created_at, updated_at
        ) VALUES (
            'station-1', 'Station', 'openai-compatible', 'https://example.test',
            'https://example.test/v1', 1, 1, 0, 1.0, 30, 'unchecked', '1', '1'
        )
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("station");
    sqlx::query(
        r#"
        INSERT INTO station_keys (
            id, station_id, name, api_key, enabled, priority, max_concurrency,
            schedulable, status, created_at, updated_at
        ) VALUES (
            'key-1', 'station-1', 'Key 1', '', 1, 0, 3, 1, 'unchecked', '1', '1'
        )
        "#,
    )
    .execute(connection)
    .await
    .expect("station key");
}

async fn seed_station_monitor_target(
    connection: &mut SqliteConnection,
    target_id: &str,
    station_key_id: &str,
) {
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO channel_monitor_request_templates (
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
        INSERT OR IGNORE INTO channel_monitors (
            id, name, target_type, station_id, station_key_id, template_id,
            enabled, interval_seconds, jitter_seconds, timeout_seconds,
            max_concurrency, consecutive_failure_threshold, fallback_models_json,
            next_run_at, created_at, updated_at
        ) VALUES ('monitor-1', 'Primary', 'station_key', 'station-1', 'key-1', 'template-1',
                  1, 60, 5, 15, 1, 3, '["gpt-primary"]', '1000', '1', '1')
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("monitor");
    let execution_id = format!("execution-{target_id}");
    let attempt_id = format!("attempt-{target_id}");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_executions (
            id, monitor_id, trigger_kind, status, planned_at_ms, started_at_ms,
            config_snapshot_hash, endpoint_revision, target_count, created_at_ms
        ) VALUES (?1, 'monitor-1', 'manual', 'running', 1, 1, 'hash', 1, 1, 1)
        "#,
    )
    .bind(&execution_id)
    .execute(&mut *connection)
    .await
    .expect("execution");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_attempts (
            id, execution_id, monitor_id, station_id, station_key_id, endpoint_revision,
            model, model_role, model_index, attempt_number, protocol_kind,
            client_profile_id, client_profile_version, request_profile_hash, transport_mode,
            started_at_ms, finished_at_ms, latency_ms, outcome, retryable,
            content_extracted, validation_passed, output_bytes, created_at_ms
        ) VALUES (
            ?1, ?2, 'monitor-1', 'station-1', ?3, 1, 'gpt-primary', 'primary', 0, 0,
            'generic_open_ai', 'standard_api', 1, 'hash', 'warm', 1, 2, 1, 'available',
            0, 1, 1, 0, 1
        )
        "#,
    )
    .bind(&attempt_id)
    .bind(&execution_id)
    .bind(station_key_id)
    .execute(&mut *connection)
    .await
    .expect("attempt");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_target_results (
            id, execution_id, monitor_id, station_id, station_key_id, endpoint_revision,
            terminal_outcome, requested_model, effective_model, used_fallback,
            attempt_count, decisive_attempt_id, protocol_kind, resolved_adapter_kind,
            client_profile_id, client_profile_version, request_profile_hash,
            traffic_equivalence, health_writeback_mode, health_writeback_decision,
            latency_ms, semantic_confidence, started_at_ms, finished_at_ms, created_at_ms
        ) VALUES (
            ?1, ?2, 'monitor-1', 'station-1', ?3, 1, 'available', 'gpt-primary',
            'gpt-primary', 0, 1, ?4, 'generic_open_ai', 'generic_open_ai',
            'standard_api', 1, 'hash', 'standard_api', 'authoritative', 'write',
            1, 'protocol_validated', 1, 2, 1
        )
        "#,
    )
    .bind(target_id)
    .bind(&execution_id)
    .bind(station_key_id)
    .bind(&attempt_id)
    .execute(connection)
    .await
    .expect("target result");
}

async fn station_key_health(connection: &mut SqliteConnection) -> sqlx::sqlite::SqliteRow {
    sqlx::query(
        "SELECT endpoint_revision, success_count, failure_count, consecutive_failures,
                last_error_summary, cooldown_until
         FROM station_key_health WHERE station_key_id = 'key-1'",
    )
    .fetch_one(connection)
    .await
    .expect("station key health")
}

async fn station_key_status(connection: &mut SqliteConnection) -> Option<String> {
    sqlx::query_scalar::<_, Option<String>>("SELECT status FROM station_keys WHERE id = 'key-1'")
        .fetch_one(connection)
        .await
        .expect("station key status")
}

fn assert_observation_row(
    row: &sqlx::sqlite::SqliteRow,
    source: &str,
    source_event_id: &str,
    target_result_id: Option<&str>,
    outcome: &str,
    traffic_equivalence: &str,
    writeback_decision: &str,
) {
    assert_eq!(row.get::<String, _>("source"), source);
    assert_eq!(row.get::<String, _>("source_event_id"), source_event_id);
    assert_eq!(
        row.get::<Option<String>, _>("target_result_id").as_deref(),
        target_result_id
    );
    assert_eq!(row.get::<String, _>("outcome"), outcome);
    assert_eq!(
        row.get::<String, _>("traffic_equivalence"),
        traffic_equivalence
    );
    assert_eq!(
        row.get::<String, _>("writeback_decision"),
        writeback_decision
    );
}

fn observation(
    id: &str,
    source: HealthObservationSource,
    source_event_id: &str,
    outcome: HealthObservationOutcome,
    traffic_equivalence: TrafficEquivalence,
    writeback_mode: HealthWritebackMode,
    target_result_id: Option<&str>,
    observed_at_ms: i64,
) -> HealthObservation {
    HealthObservation {
        id: format!("health-observation-{id}"),
        station_key_id: "key-1".to_string(),
        target_result_id: target_result_id.map(ToOwned::to_owned),
        source,
        source_event_id: source_event_id.to_string(),
        observed_at_ms,
        endpoint_revision: 1,
        outcome,
        failure_kind: match outcome {
            HealthObservationOutcome::Success
            | HealthObservationOutcome::Skipped
            | HealthObservationOutcome::Neutral => None,
            HealthObservationOutcome::ObserveFailure => Some("network".to_string()),
            HealthObservationOutcome::Cooldown => Some("rate_limit".to_string()),
            HealthObservationOutcome::HardFail => Some("auth".to_string()),
        },
        latency_ms: Some(100),
        retry_after_ms: None,
        error_summary: None,
        writeback_mode,
        traffic_equivalence,
    }
}

trait ObservationTestExt {
    fn with_retry_after(self, retry_after_ms: i64) -> Self;
    fn with_error(self, error_summary: &str) -> Self;
    fn with_endpoint_revision(self, endpoint_revision: i64) -> Self;
}

impl ObservationTestExt for HealthObservation {
    fn with_retry_after(mut self, retry_after_ms: i64) -> Self {
        self.retry_after_ms = Some(retry_after_ms);
        self
    }

    fn with_error(mut self, error_summary: &str) -> Self {
        self.error_summary = Some(error_summary.to_string());
        self
    }

    fn with_endpoint_revision(mut self, endpoint_revision: i64) -> Self {
        self.endpoint_revision = endpoint_revision;
        self
    }
}
