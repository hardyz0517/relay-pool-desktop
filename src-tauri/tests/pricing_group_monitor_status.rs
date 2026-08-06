#[path = "../src/persistence/stores/monitoring/group_status_repository.rs"]
mod group_status_repository;
#[path = "../src/persistence/error.rs"]
mod persistence_error;
#[path = "../src/models/pricing_group_monitoring.rs"]
mod pricing_models;

mod models {
    pub(crate) mod pricing_group_monitoring {
        pub(crate) use crate::pricing_models::*;
    }
}

mod persistence {
    pub(crate) mod error {
        pub(crate) use crate::persistence_error::*;
    }

    pub(crate) struct ReadSession;

    impl ReadSession {
        pub(crate) fn connection(&mut self) -> &mut sqlx::SqliteConnection {
            unimplemented!("repository integration tests call load_connection directly")
        }
    }
}

use group_status_repository::PricingGroupMonitorStatusRepository;
use models::pricing_group_monitoring::{
    canonicalize_group_refs, group_refs_hash, reduce_pricing_group_monitor_summary,
    CanonicalGroupRef, DisplayState, LatestOutcome, MatchKind, PricingGroupMonitorReducerInput,
};
use sqlx::{Connection, Row, SqliteConnection};
use std::sync::OnceLock;

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");
static TEST_LOCK: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();

fn test_lock() -> &'static tokio::sync::Mutex<()> {
    TEST_LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

#[tokio::test]
async fn repository_reads_real_schema_and_keeps_identity_boundaries() {
    let _guard = test_lock().lock().await;
    let mut connection = ready_connection().await;
    seed_fixture(&mut connection).await;
    let refs = vec![
        binding_ref("b-exact"),
        binding_ref("b-parent"),
        group_id_ref("gid-only"),
        group_key_ref("gkey-only"),
        binding_ref("missing-binding"),
    ];
    let encoded = serde_json::to_string(
        &refs
            .iter()
            .map(|group| {
                serde_json::json!({
                    "stationId": group.station_id,
                    "groupBindingId": group.group_binding_id,
                    "groupIdHash": group.group_id_hash,
                    "groupKeyHash": group.group_key_hash,
                    "canonicalKey": group.canonical_key().expect("canonical key"),
                })
            })
            .collect::<Vec<_>>(),
    )
    .expect("encoded refs");
    PricingGroupMonitorStatusRepository::reset_query_count();
    let rows = PricingGroupMonitorStatusRepository
        .load_connection(&mut connection, &refs, &encoded)
        .await
        .expect("batched status read");
    assert_eq!(
        PricingGroupMonitorStatusRepository::query_count(),
        5,
        "one batched read session must use fixed query count, not per-group SQL"
    );

    assert_eq!(rows.resolutions.len(), 4);
    assert!(rows
        .keys
        .iter()
        .any(|row| row.key.id == "key-exact-primary" && row.match_kind == "exact_binding"));
    assert!(rows
        .keys
        .iter()
        .any(|row| row.key.id == "key-parent" && row.match_kind == "parent_binding"));
    assert!(rows
        .keys
        .iter()
        .any(|row| row.key.id == "key-id" && row.match_kind == "group_id_hash"));
    assert!(rows
        .keys
        .iter()
        .any(|row| row.key.id == "key-key" && row.match_kind == "group_key_hash"));
    assert!(rows
        .keys
        .iter()
        .any(|row| row.key.id == "key-id-bound" && row.match_kind == "group_id_hash"));
    assert!(!rows
        .keys
        .iter()
        .any(|row| row.group_ref_key.contains("missing-binding")));

    let exact_keys = rows
        .keys
        .iter()
        .filter(|row| row.group_ref_key == binding_ref("b-exact").canonical_key().unwrap())
        .map(|row| row.key.id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        exact_keys,
        vec!["key-exact-primary", "key-exact-secondary"],
        "repository ordering is deterministic before reducer tie-breaks"
    );
}

#[tokio::test]
async fn latest_result_and_running_overlay_are_scoped_to_representative_target() {
    let _guard = test_lock().lock().await;
    let mut connection = ready_connection().await;
    seed_fixture(&mut connection).await;
    let refs = vec![binding_ref("b-exact"), binding_ref("b-parent")];
    let encoded = encode_refs(&refs);
    let rows = PricingGroupMonitorStatusRepository
        .load_connection(&mut connection, &refs, &encoded)
        .await
        .expect("status rows");

    let exact = binding_ref("b-exact");
    let exact_key = rows
        .keys
        .iter()
        .filter(|row| row.group_ref_key == exact.canonical_key().unwrap())
        .map(|row| row.key.clone())
        .collect::<Vec<_>>();
    let exact_monitors = rows
        .monitors
        .iter()
        .filter(|monitor| monitor.station_key_id.as_deref() == Some("key-exact-primary"))
        .cloned()
        .collect::<Vec<_>>();
    let exact_summary = reduce_pricing_group_monitor_summary(PricingGroupMonitorReducerInput {
        group_ref: exact,
        match_kind: MatchKind::ExactBinding,
        resolution_state: models::pricing_group_monitoring::ResolutionState::Resolved,
        keys: exact_key,
        monitors: exact_monitors,
        target_results: rows.target_results.clone(),
        running: rows.running.clone(),
        generated_at_ms: 1,
    });
    assert_eq!(
        exact_summary.representative_monitor_id.as_deref(),
        Some("monitor-exact-first")
    );
    assert_eq!(exact_summary.display_state, DisplayState::Running);
    assert_eq!(exact_summary.latest_outcome, LatestOutcome::Available);
    assert_eq!(
        exact_summary.latest_target_result_id.as_deref(),
        Some("result-exact-terminal")
    );

    let parent = binding_ref("b-parent");
    let parent_keys = rows
        .keys
        .iter()
        .filter(|row| row.group_ref_key == parent.canonical_key().unwrap())
        .map(|row| row.key.clone())
        .collect::<Vec<_>>();
    let parent_monitors = rows
        .monitors
        .iter()
        .filter(|monitor| monitor.target_type == "station")
        .cloned()
        .collect::<Vec<_>>();
    let parent_summary = reduce_pricing_group_monitor_summary(PricingGroupMonitorReducerInput {
        group_ref: parent,
        match_kind: MatchKind::ParentBinding,
        resolution_state: models::pricing_group_monitoring::ResolutionState::Resolved,
        keys: parent_keys,
        monitors: parent_monitors,
        target_results: rows.target_results,
        running: rows.running,
        generated_at_ms: 1,
    });
    assert_eq!(parent_summary.display_state, DisplayState::Available);
    assert_eq!(
        parent_summary.latest_target_result_id.as_deref(),
        Some("result-parent-key")
    );
    assert_eq!(parent_summary.tested_key_count, 1);
}

#[tokio::test]
async fn batch_boundary_is_explicit_and_does_not_drop_rows() {
    let _guard = test_lock().lock().await;
    let refs = (0..500)
        .map(|index| group_key_ref(&format!("gkey-{index:03}")))
        .collect::<Vec<_>>();
    assert_eq!(canonicalize_group_refs(&refs).expect("500 refs").len(), 500);
    assert_eq!(group_refs_hash(&refs).expect("500 refs hash").len(), 64);
    assert!(canonicalize_group_refs(
        &(0..501)
            .map(|index| group_key_ref(&format!("gkey-{index:03}")))
            .collect::<Vec<_>>()
    )
    .is_err());

    let mut connection = ready_connection().await;
    let encoded = encode_refs(&refs);
    let rows = PricingGroupMonitorStatusRepository
        .load_connection(&mut connection, &refs, &encoded)
        .await
        .expect("500 refs read");
    assert!(rows.resolutions.is_empty());
    assert!(rows.keys.is_empty());
}

#[tokio::test]
async fn explain_plans_use_the_expected_read_indexes() {
    let _guard = test_lock().lock().await;
    let mut connection = ready_connection().await;
    seed_fixture(&mut connection).await;
    let plans = [
        (
            "keys",
            "EXPLAIN QUERY PLAN SELECT k.id FROM station_keys k WHERE k.station_id = 'station-1' AND k.group_id_hash = 'gid-only'",
            "idx_station_keys",
        ),
        (
            "monitors",
            "EXPLAIN QUERY PLAN SELECT m.id FROM channel_monitors m WHERE m.enabled = 1 ORDER BY m.enabled DESC, m.created_at ASC, m.id ASC",
            "idx_channel_monitors",
        ),
        (
            "latest-results",
            "EXPLAIN QUERY PLAN SELECT tr.id FROM channel_monitor_target_results tr WHERE tr.monitor_id = 'monitor-exact-first' AND tr.station_key_id = 'key-exact-primary' ORDER BY tr.finished_at_ms DESC, tr.id DESC",
            "idx_channel_monitor_target_results_monitor_station_finished",
        ),
    ];
    for (name, sql, expected_index) in plans {
        let details = sqlx::query(sql)
            .fetch_all(&mut connection)
            .await
            .expect(name)
            .into_iter()
            .map(|row| row.get::<String, _>("detail"))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            details.contains(expected_index),
            "{name} plan did not use {expected_index}: {details}"
        );
    }
}

fn binding_ref(binding_id: &str) -> CanonicalGroupRef {
    CanonicalGroupRef {
        station_id: "station-1".into(),
        group_binding_id: Some(binding_id.into()),
        group_id_hash: None,
        group_key_hash: format!("unused-{binding_id}"),
    }
}

fn group_id_ref(group_id_hash: &str) -> CanonicalGroupRef {
    CanonicalGroupRef {
        station_id: "station-1".into(),
        group_binding_id: None,
        group_id_hash: Some(group_id_hash.into()),
        group_key_hash: format!("unused-{group_id_hash}"),
    }
}

fn group_key_ref(group_key_hash: &str) -> CanonicalGroupRef {
    CanonicalGroupRef {
        station_id: "station-1".into(),
        group_binding_id: None,
        group_id_hash: None,
        group_key_hash: group_key_hash.into(),
    }
}

fn encode_refs(groups: &[CanonicalGroupRef]) -> String {
    serde_json::to_string(
        &groups
            .iter()
            .map(|group| {
                serde_json::json!({
                    "stationId": group.station_id,
                    "groupBindingId": group.group_binding_id,
                    "groupIdHash": group.group_id_hash,
                    "groupKeyHash": group.group_key_hash,
                    "canonicalKey": group.canonical_key().expect("canonical key"),
                })
            })
            .collect::<Vec<_>>(),
    )
    .expect("encoded refs")
}

async fn ready_connection() -> SqliteConnection {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    MIGRATOR.run(&mut connection).await.expect("migrations");
    sqlx::query("INSERT INTO stations (id, name, station_type, website_url, api_base_url, created_at, updated_at) VALUES ('station-1', 'Fixture', 'openai-compatible', 'https://example.test', 'https://example.test/v1', '1', '1')")
        .execute(&mut connection)
        .await
        .expect("station");
    sqlx::query("INSERT INTO channel_monitor_request_templates (id, name, endpoint_kind, method, path, request_body_json, created_at, updated_at) VALUES ('template-1', 'Fixture', 'chat', 'POST', '/v1/chat/completions', '{}', '1', '1')")
        .execute(&mut connection)
        .await
        .expect("template");
    connection
}

async fn seed_fixture(connection: &mut SqliteConnection) {
    for (id, group_key, group_id, kind, parent) in [
        (
            "b-exact",
            "gkey-exact",
            Some("gid-exact"),
            "station_group",
            Option::<&str>::None,
        ),
        (
            "b-parent",
            "gkey-parent",
            Some("gid-parent"),
            "station_group",
            Option::<&str>::None,
        ),
        (
            "b-id",
            "gkey-id",
            Some("gid-only"),
            "station_group",
            Option::<&str>::None,
        ),
        (
            "b-key",
            "gkey-only",
            None,
            "station_group",
            Option::<&str>::None,
        ),
    ] {
        sqlx::query(
            "INSERT INTO station_group_bindings (id, station_id, station_key_id, binding_kind, parent_group_binding_id, group_key_hash, group_id_hash, group_name, binding_status, confidence, created_at, updated_at) VALUES (?1, 'station-1', ?2, ?3, ?4, ?5, ?6, 'Shared Name', 'bound', 1.0, '1', '1')",
        )
        .bind(id)
        .bind(Option::<&str>::None)
        .bind(kind)
        .bind(parent)
        .bind(group_key)
        .bind(group_id)
        .execute(&mut *connection)
        .await
        .expect("binding");
    }
    insert_key(
        connection,
        "key-exact-primary",
        Some("b-exact"),
        None,
        1,
        10,
        1,
        "x",
    )
    .await;
    insert_key(
        connection,
        "key-exact-secondary",
        Some("b-exact"),
        None,
        0,
        20,
        1,
        "",
    )
    .await;
    insert_key(
        connection,
        "key-parent",
        Some("b-parent-child"),
        None,
        1,
        10,
        1,
        "x",
    )
    .await;
    insert_key(connection, "key-id", None, Some("gid-only"), 1, 1, 1, "x").await;
    insert_key(connection, "key-id-bound", Some("b-id"), None, 1, 1, 1, "x").await;
    insert_key(connection, "key-key", None, None, 1, 1, 1, "x").await;
    sqlx::query("UPDATE station_keys SET group_name = 'Shared Name' WHERE id = 'key-key'")
        .execute(&mut *connection)
        .await
        .expect("key group name");
    sqlx::query("UPDATE station_keys SET group_binding_id = 'b-key' WHERE id = 'key-key'")
        .execute(&mut *connection)
        .await
        .expect("key binding");
    sqlx::query(
        "INSERT INTO station_group_bindings (id, station_id, station_key_id, binding_kind, parent_group_binding_id, group_key_hash, group_id_hash, group_name, binding_status, confidence, created_at, updated_at) VALUES ('b-parent-child', 'station-1', 'key-parent', 'key_binding', 'b-parent', 'gkey-parent-child', NULL, 'Shared Name', 'bound', 1.0, '1', '1')",
    )
    .execute(&mut *connection)
    .await
    .expect("parent binding");
    for (id, target_type, key_id, enabled, created) in [
        (
            "monitor-exact-first",
            "station_key",
            Some("key-exact-primary"),
            1,
            "1",
        ),
        (
            "monitor-exact-second",
            "station_key",
            Some("key-exact-primary"),
            1,
            "2",
        ),
        ("monitor-parent-station", "station", None, 1, "3"),
        (
            "monitor-disabled",
            "station_key",
            Some("key-exact-primary"),
            0,
            "0",
        ),
    ] {
        sqlx::query("INSERT INTO channel_monitors (id, name, target_type, station_id, station_key_id, template_id, enabled, interval_seconds, timeout_seconds, fallback_models_json, created_at, updated_at) VALUES (?1, 'Fixture', ?2, 'station-1', ?3, 'template-1', ?4, 60, 5, '[]', ?5, ?5)")
            .bind(id)
            .bind(target_type)
            .bind(key_id)
            .bind(enabled)
            .bind(created)
            .execute(&mut *connection)
            .await
            .expect("monitor");
    }
    insert_result(
        connection,
        "monitor-exact-first",
        "result-exact-terminal",
        Some("key-exact-primary"),
        100,
        "available",
    )
    .await;
    insert_result(
        connection,
        "monitor-exact-second",
        "result-exact-later",
        Some("key-exact-primary"),
        200,
        "unavailable",
    )
    .await;
    insert_result(
        connection,
        "monitor-parent-station",
        "result-parent-key",
        Some("key-parent"),
        300,
        "available",
    )
    .await;
    insert_result(
        connection,
        "monitor-parent-station",
        "result-parent-station-wide",
        None,
        400,
        "unavailable",
    )
    .await;
    sqlx::query("INSERT INTO channel_monitor_executions (id, monitor_id, trigger_kind, status, planned_at_ms, config_snapshot_hash, created_at_ms) VALUES ('execution-running', 'monitor-exact-first', 'manual', 'running', 500, 'fixture', 500)")
        .execute(&mut *connection)
        .await
        .expect("running execution");
}

async fn insert_key(
    connection: &mut SqliteConnection,
    id: &str,
    group_binding_id: Option<&str>,
    group_id_hash: Option<&str>,
    enabled: i64,
    priority: i64,
    created_at: i64,
    api_key: &str,
) {
    sqlx::query("INSERT INTO station_keys (id, station_id, name, api_key, enabled, priority, group_binding_id, group_id_hash, created_at, updated_at) VALUES (?1, 'station-1', 'Fixture', ?2, ?3, ?4, ?5, ?6, ?7, ?7)")
        .bind(id)
        .bind(api_key)
        .bind(enabled)
        .bind(priority)
        .bind(group_binding_id)
        .bind(group_id_hash)
        .bind(created_at.to_string())
        .execute(&mut *connection)
        .await
        .expect("key");
}

async fn insert_result(
    connection: &mut SqliteConnection,
    monitor_id: &str,
    result_id: &str,
    station_key_id: Option<&str>,
    finished_at_ms: i64,
    outcome: &str,
) {
    let execution_id = format!("execution-{result_id}");
    sqlx::query("INSERT INTO channel_monitor_executions (id, monitor_id, trigger_kind, status, planned_at_ms, started_at_ms, finished_at_ms, config_snapshot_hash, target_count, created_at_ms) VALUES (?1, ?2, 'manual', 'completed', ?3, ?3, ?4, 'fixture', 1, ?3)")
        .bind(&execution_id)
        .bind(monitor_id)
        .bind(finished_at_ms - 1)
        .bind(finished_at_ms)
        .execute(&mut *connection)
        .await
        .expect("execution result");
    sqlx::query("INSERT INTO channel_monitor_target_results (id, execution_id, monitor_id, station_id, station_key_id, terminal_outcome, requested_model, attempt_count, resolved_adapter_kind, client_profile_id, client_profile_version, traffic_equivalence, semantic_confidence, started_at_ms, finished_at_ms, created_at_ms) VALUES (?1, ?2, ?3, 'station-1', ?4, ?5, 'fixture-model', 1, 'generic_open_ai', 'standard_api', 1, 'standard_api', 'protocol_validated', ?6, ?7, ?7)")
        .bind(result_id)
        .bind(&execution_id)
        .bind(monitor_id)
        .bind(station_key_id)
        .bind(outcome)
        .bind(finished_at_ms - 1)
        .bind(finished_at_ms)
        .execute(&mut *connection)
        .await
        .expect("target result");
}
