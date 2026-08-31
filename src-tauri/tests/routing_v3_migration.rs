use std::borrow::Cow;

use sqlx::{migrate::Migrator, Connection, Row, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./src/persistence/migrations");

fn migrator_through(target_version: i64) -> Migrator {
    Migrator {
        migrations: Cow::Owned(
            MIGRATOR
                .iter()
                .filter(|migration| migration.version <= target_version)
                .cloned()
                .collect(),
        ),
        ignore_missing: false,
        locking: true,
        no_tx: false,
    }
}

async fn insert_v3_repair_observation(
    connection: &mut SqliteConnection,
    id: &str,
    station_key_id: &str,
    lifecycle_revision: i64,
    correlation_id: &str,
    producer_sequence: i64,
) {
    sqlx::query(
        "INSERT INTO routing_observations (
             id, producer_id, producer_sequence, payload_hash,
             event_at_ms, ingested_at_ms, scope, source, traffic_equivalence,
             outcome_kind, latency_ms, mass_basis_points, evidence_json,
             created_at_ms, event_id, attempt_id, correlation_id, station_key_id,
             station_key_lifecycle_revision, attempt_index, candidate_admitted,
             candidate_admitted_at_ms, boundary_crossed, response_origin,
             event_time_status, outcome, failure_attribution, comparability_key,
             observed_at_ms, recovery_origin, retry_disposition, algorithm_version,
             source_weight_revision, quality_policy_revision, generation_eligibility,
             cluster_finalized, cluster_expected_attempt_count,
             cluster_finalized_at_ms, cluster_finalization_reason
         ) VALUES (
             ?1, 'fixture', ?2, ?3, 100, 100, ?4, 'real_request',
             'exact_request', 'success', 20, 10000, '{}', 100, ?1, ?1,
             ?5, ?6, ?7, 0, 1, 100, 1, 'upstream', 'valid', 'success',
             'key', 'cmp:v1:fixture', 100, 'normal', 'end', 'routing-v3',
             1, 1, 'active', 1, 1, 100, 'attempt_terminal'
         )",
    )
    .bind(id)
    .bind(producer_sequence)
    .bind("a".repeat(64))
    .bind(format!("station_key:{station_key_id}"))
    .bind(correlation_id)
    .bind(station_key_id)
    .bind(lifecycle_revision)
    .execute(connection)
    .await
    .expect("insert v3 repair observation");
}

#[tokio::test]
async fn schema_71_aliases_group_rate_lifecycle_drift_without_rewriting_v3_facts() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    migrator_through(70)
        .run(&mut connection)
        .await
        .expect("schema 70");

    sqlx::query(
        "INSERT INTO stations (
             id, name, station_type, website_url, api_base_url, created_at, updated_at
         ) VALUES ('station-repair', 'Repair fixture', 'sub2api',
                   'https://repair.example.test', 'https://repair.example.test/v1', '0', '0')",
    )
    .execute(&mut connection)
    .await
    .expect("insert station");
    sqlx::query(
        "INSERT INTO station_keys (
             id, station_id, rate_source, rate_collected_at, created_at, updated_at
         ) VALUES ('key-repair', 'station-repair', 'sub2api_groups_rates',
                   '1700000000000', '0', '1700000000000')",
    )
    .execute(&mut connection)
    .await
    .expect("insert station key");
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('station_key:key-repair', 2, 1700000000000, 'transactional_write')",
    )
    .execute(&mut connection)
    .await
    .expect("insert drifted key revision");
    insert_v3_repair_observation(
        &mut connection,
        "observation-repair",
        "key-repair",
        2,
        "correlation-repair",
        1,
    )
    .await;
    sqlx::query(
        "INSERT INTO routing_attempt_v3 (
             attempt_id, event_id, correlation_id, source, station_key_id,
             station_key_lifecycle_revision, attempt_index, candidate_admitted,
             candidate_admitted_at_ms, boundary_crossed, boundary_crossed_at_ms,
             response_origin, event_time_status, outcome, failure_attribution,
             latency_ms, event_at_ms, observed_at_ms, ingested_at_ms,
             comparability_key, recovery_origin, retry_disposition,
             algorithm_version, source_weight_revision, quality_policy_revision,
             generation_eligibility, terminal_state, terminal_at_ms, released_at_ms,
             created_at_ms, updated_at_ms
         ) VALUES (
             'attempt-repair', 'attempt-event-repair', 'correlation-repair',
             'real_request', 'key-repair', 2, 0, 1, 100, 1, 100, 'upstream',
             'valid', 'success', 'key', 20, 100, 100, 100, 'cmp:v1:fixture',
             'normal', 'end', 'routing-v3', 1, 1, 'active', 'success', 100, 100,
             100, 100
         )",
    )
    .execute(&mut connection)
    .await
    .expect("insert repair attempt");
    sqlx::query(
        "INSERT INTO routing_attempt_cluster_v3 (
             source, station_key_id, station_key_lifecycle_revision,
             correlation_id, expected_attempt_count, cluster_finalized,
             cluster_finalized_at_ms, cluster_finalization_reason,
             generation_eligibility, created_at_ms, updated_at_ms
         ) VALUES ('real_request', 'key-repair', 2, 'correlation-repair', 1,
                   1, 100, 'attempt_terminal', 'active', 100, 100)",
    )
    .execute(&mut connection)
    .await
    .expect("insert repair cluster");

    let observation_before: (i64, Option<i64>, String, String) = sqlx::query_as(
        "SELECT station_key_lifecycle_revision, ingestion_sequence, event_id, attempt_id
         FROM routing_observations WHERE id = 'observation-repair'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read observation before repair");
    let attempt_before: (i64, String, String) = sqlx::query_as(
        "SELECT station_key_lifecycle_revision, event_id, terminal_state
         FROM routing_attempt_v3 WHERE attempt_id = 'attempt-repair'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read attempt before repair");
    let cluster_before: (i64, i64, String) = sqlx::query_as(
        "SELECT station_key_lifecycle_revision, cluster_finalized, cluster_finalization_reason
         FROM routing_attempt_cluster_v3
         WHERE source = 'real_request' AND correlation_id = 'correlation-repair'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read cluster before repair");

    migrator_through(71)
        .run(&mut connection)
        .await
        .expect("schema 71");

    let alias: (String, i64, String) = sqlx::query_as(
        "SELECT station_key_id, target_lifecycle_revision, reason_code
         FROM routing_quality_lifecycle_alias_v1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read lifecycle alias");
    assert_eq!(
        alias,
        (
            "key-repair".to_string(),
            2,
            "group_rate_projection_lifecycle_drift".to_string()
        )
    );

    let observation_after: (i64, Option<i64>, String, String) = sqlx::query_as(
        "SELECT station_key_lifecycle_revision, ingestion_sequence, event_id, attempt_id
         FROM routing_observations WHERE id = 'observation-repair'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read observation after repair");
    let attempt_after: (i64, String, String) = sqlx::query_as(
        "SELECT station_key_lifecycle_revision, event_id, terminal_state
         FROM routing_attempt_v3 WHERE attempt_id = 'attempt-repair'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read attempt after repair");
    let cluster_after: (i64, i64, String) = sqlx::query_as(
        "SELECT station_key_lifecycle_revision, cluster_finalized, cluster_finalization_reason
         FROM routing_attempt_cluster_v3
         WHERE source = 'real_request' AND correlation_id = 'correlation-repair'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read cluster after repair");
    assert_eq!(observation_after, observation_before);
    assert_eq!(attempt_after, attempt_before);
    assert_eq!(cluster_after, cluster_before);
}

#[tokio::test]
async fn schema_71_does_not_alias_a_key_with_mixed_lifecycle_observations() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    migrator_through(70)
        .run(&mut connection)
        .await
        .expect("schema 70");

    sqlx::query(
        "INSERT INTO stations (
             id, name, station_type, website_url, api_base_url, created_at, updated_at
         ) VALUES ('station-mixed', 'Mixed fixture', 'sub2api',
                   'https://mixed.example.test', 'https://mixed.example.test/v1', '0', '0')",
    )
    .execute(&mut connection)
    .await
    .expect("insert station");
    sqlx::query(
        "INSERT INTO station_keys (
             id, station_id, rate_source, rate_collected_at, created_at, updated_at
         ) VALUES ('key-mixed', 'station-mixed', 'sub2api_groups_rates',
                   '1800000000000', '0', '1800000000000')",
    )
    .execute(&mut connection)
    .await
    .expect("insert station key");
    sqlx::query(
        "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
         VALUES ('station_key:key-mixed', 3, 1800000000000, 'transactional_write')",
    )
    .execute(&mut connection)
    .await
    .expect("insert mixed key revision");
    insert_v3_repair_observation(
        &mut connection,
        "observation-mixed-old",
        "key-mixed",
        1,
        "correlation-mixed",
        1,
    )
    .await;
    insert_v3_repair_observation(
        &mut connection,
        "observation-mixed-current",
        "key-mixed",
        3,
        "correlation-mixed",
        2,
    )
    .await;

    migrator_through(71)
        .run(&mut connection)
        .await
        .expect("schema 71");

    let alias_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_quality_lifecycle_alias_v1
         WHERE station_key_id = 'key-mixed'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read mixed alias count");
    assert_eq!(alias_count, 0);
    let revisions: Vec<i64> = sqlx::query_scalar(
        "SELECT station_key_lifecycle_revision
         FROM routing_observations
         WHERE station_key_id = 'key-mixed'
         ORDER BY id",
    )
    .fetch_all(&mut connection)
    .await
    .expect("read mixed observation revisions");
    assert_eq!(revisions, vec![3, 1]);
}

#[tokio::test]
async fn routing_v3_migrations_register_and_satisfy_schema_postconditions() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    MIGRATOR.run(&mut connection).await.expect("run migrations");

    let latest: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema compatibility");
    assert_eq!(latest, 71);
    assert_eq!(
        MIGRATOR.iter().map(|migration| migration.version).max(),
        Some(71)
    );
    assert!(MIGRATOR.iter().any(|migration| migration.version == 61));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 62));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 63));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 64));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 65));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 66));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 67));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 68));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 69));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 70));
    assert!(MIGRATOR.iter().any(|migration| migration.version == 71));

    for table in [
        "routing_attempt_v3",
        "routing_attempt_cluster_v3",
        "routing_quality_generation_v3",
        "routing_quality_generation_v3_checkpoint",
        "routing_quality_summary_v3",
        "routing_quality_health_axis_v3",
        "routing_quality_incremental_checkpoint_v3",
        "routing_quality_pending_cluster_v3",
        "routing_quality_lifecycle_alias_v1",
        "routing_circuit_state_v3",
        "routing_circuit_event_v3",
        "routing_circuit_generation_v3",
        "routing_circuit_generation_v3_checkpoint",
        "routing_circuit_state_generation_v3",
        "routing_circuit_event_applied_generation_v3",
        "routing_runtime_generation",
        "routing_runtime_cutover_marker",
        "routing_generation_transition_audit",
        "routing_generation_qualification",
        "routing_generation_qualification_report",
        "routing_generation_qualification_v2",
        "routing_generation_qualification_report_v2",
        "routing_raw_event_retention_rollup",
        "routing_raw_event_retention_run",
        "routing_attempt_late_audit_v3",
        "routing_quality_source_profile_snapshot_v3",
        "routing_quality_source_profile_snapshot_item_v3",
        "routing_generation_report_secret",
        "routing_circuit_persistence_gate_v3",
        "routing_circuit_persistence_health_v3",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        )
        .bind(table)
        .fetch_one(&mut connection)
        .await
        .expect("table lookup");
        assert_eq!(count, 1, "missing {table}");
    }

    for index in [
        "idx_routing_observations_v3_event_id",
        "idx_routing_observations_v3_attempt_id",
        "idx_routing_observations_v3_attempt_identity",
        "idx_routing_runtime_generation_one_active",
        "idx_routing_runtime_generation_one_fencing",
        "idx_routing_observations_v3_retention",
        "idx_routing_circuit_event_v3_retention",
        "idx_routing_attempt_late_audit_v3_cluster",
        "idx_routing_quality_profile_snapshot_item_v3_key",
        "idx_routing_quality_generation_v3_build_request",
        "idx_routing_circuit_persistence_gate_v3_status",
        "idx_routing_circuit_event_v3_applied_sequence",
    ] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        )
        .bind(index)
        .fetch_one(&mut connection)
        .await
        .expect("index lookup");
        assert_eq!(count, 1, "missing {index}");
    }

    let marker: (String, Option<String>, Option<String>, i64) = sqlx::query_as(
        "SELECT status, runtime_generation_id, fenced_runtime_generation_id, fence_revision
         FROM routing_runtime_cutover_marker WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("cutover marker");
    assert_eq!(marker.0, "pre_cutover");
    assert_eq!(marker.1, None);
    assert_eq!(marker.2, None);
    assert_eq!(marker.3, 0);

    let active_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM routing_runtime_generation WHERE status = 'active'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("active generation count");
    assert_eq!(
        active_count, 0,
        "migration must not invent an active generation"
    );

    let columns = sqlx::query("PRAGMA table_info(routing_observations)")
        .fetch_all(&mut connection)
        .await
        .expect("observation columns");
    for required in [
        "ingestion_sequence",
        "event_id",
        "attempt_id",
        "correlation_id",
        "station_key_lifecycle_revision",
        "generation_eligibility",
        "event_time_status",
        "cluster_finalized",
        "cluster_expected_attempt_count",
    ] {
        assert!(
            columns
                .iter()
                .any(|column| column.get::<String, _>("name") == required),
            "missing routing_observations.{required}"
        );
    }

    let circuit_event_columns = sqlx::query("PRAGMA table_info(routing_circuit_event_v3)")
        .fetch_all(&mut connection)
        .await
        .expect("circuit event columns");
    assert!(
        circuit_event_columns
            .iter()
            .any(|column| column.get::<String, _>("name") == "applied"),
        "missing routing_circuit_event_v3.applied"
    );

    let quality_generation_columns =
        sqlx::query("PRAGMA table_info(routing_quality_generation_v3)")
            .fetch_all(&mut connection)
            .await
            .expect("quality generation columns");
    for required in [
        "source_profile_snapshot_id",
        "build_request_hash",
        "expected_input_observation_count",
        "expected_output_scope_count",
    ] {
        assert!(
            quality_generation_columns
                .iter()
                .any(|column| column.get::<String, _>("name") == required),
            "missing routing_quality_generation_v3.{required}"
        );
    }
}

#[tokio::test]
async fn routing_v3_runtime_active_pointer_is_unique_and_circuit_shapes_are_guarded() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    MIGRATOR.run(&mut connection).await.expect("run migrations");

    let duplicate_marker = sqlx::query(
        "INSERT INTO routing_runtime_cutover_marker
             (singleton_key, status, fence_revision, updated_at_ms)
         VALUES (1, 'pre_cutover', 0, 0)",
    )
    .execute(&mut connection)
    .await;
    assert!(
        duplicate_marker.is_err(),
        "cutover marker must be singleton"
    );

    let bad_state = sqlx::query(
        "INSERT INTO routing_circuit_state_v3 (
             station_key_id, station_key_lifecycle_revision, state, state_revision,
             opened_at_ms, cooldown_until_ms, updated_at_ms
         ) VALUES ('key-1', 1, 'closed', 1, 1, 2, 2)",
    )
    .execute(&mut connection)
    .await;
    assert!(
        bad_state.is_err(),
        "closed state must not carry open timestamps"
    );

    let bad_event = sqlx::query(
        "INSERT INTO routing_circuit_event_v3 (
             event_id, effect_kind, source, attempt_id, station_key_id,
             station_key_lifecycle_revision, reducer_commit_sequence, policy_revision,
             expected_state_revision, occurred_at_ms, canonical_outcome,
             failure_attribution, recovery_origin, retry_disposition,
             boundary_crossed, created_at_ms
         ) VALUES (
             'event-1', 'circuit', 'active_probe', 'attempt-1', 'key-1', 1, 1, 1,
             1, 1, 'success', 'key', 'normal', 'end', 1, 1
         )",
    )
    .execute(&mut connection)
    .await;
    assert!(
        bad_event.is_err(),
        "circuit events must be real-request sourced"
    );
}

#[tokio::test]
async fn routing_v3_runtime_generation_transitions_and_fencing_are_guarded() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    MIGRATOR.run(&mut connection).await.expect("run migrations");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut connection)
        .await
        .expect("disable foreign keys for isolated registry fixture");

    for suffix in ["a", "b"] {
        sqlx::query(
            "INSERT INTO routing_runtime_generation (
                 runtime_generation_id, policy_generation_id,
                 quality_generation_id, circuit_generation_id,
                 policy_revision, quality_policy_revision,
                 circuit_policy_revision, algorithm_version, status,
                 input_observation_watermark, input_circuit_event_watermark,
                 policy_input_hash, quality_input_hash, circuit_input_hash,
                 policy_content_hash, quality_content_hash, circuit_content_hash,
                 checkpoint_ref, policy_checkpoint_ref,
                 quality_checkpoint_ref, circuit_checkpoint_ref,
                 created_at_ms, ready_at_ms, updated_at_ms
             ) VALUES (
                 ?1, ?2, ?3, ?4, 1, 1, 1, 'routing-v3', 'ready',
                 1, 1, ?5, ?5, ?5, ?5, ?5, ?5,
                 ?6, ?7, ?8, ?9, 0, 0, 0
             )",
        )
        .bind(format!("rg1_{suffix}"))
        .bind(format!("pg1_{suffix}"))
        .bind(format!("qg1_{suffix}"))
        .bind(format!("cg1_{suffix}"))
        .bind("0".repeat(64))
        .bind(format!("runtime-checkpoint-{suffix}"))
        .bind(format!("policy-checkpoint-{suffix}"))
        .bind(format!("quality-checkpoint-{suffix}"))
        .bind(format!("circuit-checkpoint-{suffix}"))
        .execute(&mut connection)
        .await
        .expect("insert ready runtime generation");
    }

    let illegal_transition = sqlx::query(
        "UPDATE routing_runtime_generation
         SET status = 'retired', retired_at_ms = 1, updated_at_ms = 1
         WHERE runtime_generation_id = 'rg1_a'",
    )
    .execute(&mut connection)
    .await;
    assert!(
        illegal_transition.is_err(),
        "ready generations cannot skip fencing and activation"
    );

    sqlx::query(
        "UPDATE routing_runtime_generation
         SET status = 'cutover_fencing', cutover_fence_revision = 1,
             updated_at_ms = 1
         WHERE runtime_generation_id = 'rg1_a'",
    )
    .execute(&mut connection)
    .await
    .expect("first fencing generation");
    let duplicate_fencing = sqlx::query(
        "UPDATE routing_runtime_generation
         SET status = 'cutover_fencing', cutover_fence_revision = 2,
             updated_at_ms = 2
         WHERE runtime_generation_id = 'rg1_b'",
    )
    .execute(&mut connection)
    .await;
    assert!(
        duplicate_fencing.is_err(),
        "only one runtime generation may be fencing"
    );
}

#[tokio::test]
async fn routing_v3_generation_transition_audit_is_append_only() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    MIGRATOR.run(&mut connection).await.expect("run migrations");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut connection)
        .await
        .expect("disable foreign keys for isolated audit fixture");
    sqlx::query(
        "INSERT INTO routing_generation_transition_audit (
             transition_kind, target_runtime_generation_id,
             fence_revision, created_at_ms
         ) VALUES ('cutover_started', 'rg1_target', 1, 0)",
    )
    .execute(&mut connection)
    .await
    .expect("insert transition audit");

    let update = sqlx::query(
        "UPDATE routing_generation_transition_audit
         SET reason_code = 'changed' WHERE transition_id = 1",
    )
    .execute(&mut connection)
    .await;
    let delete =
        sqlx::query("DELETE FROM routing_generation_transition_audit WHERE transition_id = 1")
            .execute(&mut connection)
            .await;
    assert!(update.is_err(), "transition audit updates must be rejected");
    assert!(delete.is_err(), "transition audit deletes must be rejected");
}

#[tokio::test]
async fn routing_v3_generation_qualification_is_immutable_and_requires_passed_evidence() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    MIGRATOR.run(&mut connection).await.expect("run migrations");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut connection)
        .await
        .expect("disable foreign keys for isolated qualification fixture");
    let invalid = sqlx::query(
        "INSERT INTO routing_generation_qualification_v2 (
             runtime_generation_id, qualification_version,
             comparison_status, comparison_report_hash,
             replay_status, replay_report_hash, qualified_at_ms
         ) VALUES ('rg1_invalid', 'routing-generation-qualification-v2',
                   'failed', ?1, 'passed', ?1, 0)",
    )
    .bind("0".repeat(64))
    .execute(&mut connection)
    .await;
    assert!(
        invalid.is_err(),
        "failed evidence cannot qualify a generation"
    );

    sqlx::query(
        "INSERT INTO routing_generation_qualification_v2 (
             runtime_generation_id, qualification_version,
             comparison_status, comparison_report_hash,
             replay_status, replay_report_hash, qualified_at_ms
         ) VALUES ('rg1_ready', 'routing-generation-qualification-v2',
                   'passed', ?1, 'passed', ?2, 0)",
    )
    .bind("1".repeat(64))
    .bind("2".repeat(64))
    .execute(&mut connection)
    .await
    .expect("insert qualification");
    let update = sqlx::query(
        "UPDATE routing_generation_qualification_v2
         SET comparison_report_hash = ?1 WHERE runtime_generation_id = 'rg1_ready'",
    )
    .bind("3".repeat(64))
    .execute(&mut connection)
    .await;
    let delete = sqlx::query(
        "DELETE FROM routing_generation_qualification_v2
         WHERE runtime_generation_id = 'rg1_ready'",
    )
    .execute(&mut connection)
    .await;
    assert!(update.is_err(), "qualification updates must be rejected");
    assert!(delete.is_err(), "qualification deletes must be rejected");
}

#[tokio::test]
async fn routing_v3_generation_qualification_reports_are_append_only() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    MIGRATOR.run(&mut connection).await.expect("run migrations");
    sqlx::query("PRAGMA foreign_keys = OFF")
        .execute(&mut connection)
        .await
        .expect("disable foreign keys for isolated report fixture");
    let report = r#"{"report_version":"fixture-v1"}"#;
    sqlx::query(
        "INSERT INTO routing_generation_qualification_report_v2 (
             runtime_generation_id, comparison_report_json,
             comparison_report_hash, replay_report_json,
             replay_report_hash, created_at_ms
         ) VALUES ('rg1_report', ?1, ?2, ?1, ?3, 0)",
    )
    .bind(report)
    .bind("1".repeat(64))
    .bind("2".repeat(64))
    .execute(&mut connection)
    .await
    .expect("insert report");
    let update = sqlx::query(
        "UPDATE routing_generation_qualification_report_v2
         SET comparison_report_json = '{}' WHERE runtime_generation_id = 'rg1_report'",
    )
    .execute(&mut connection)
    .await;
    let delete = sqlx::query(
        "DELETE FROM routing_generation_qualification_report_v2
         WHERE runtime_generation_id = 'rg1_report'",
    )
    .execute(&mut connection)
    .await;
    assert!(
        update.is_err(),
        "qualification report updates must be rejected"
    );
    assert!(
        delete.is_err(),
        "qualification report deletes must be rejected"
    );
}
