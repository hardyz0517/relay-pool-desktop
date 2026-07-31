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

#[path = "../src/persistence/stores/routing_decisions/mod.rs"]
mod routing_decisions;

use std::collections::BTreeMap;

use routing_decisions::{
    queries::RoutingDecisionQueries,
    retention::RoutingDecisionRetention,
    write::{
        RouteCandidateDecisionWrite, RoutingDecisionWrite, RoutingDecisionWriter,
        RoutingTraceStatus,
    },
    MAX_ROUTE_CANDIDATE_DECISION_DETAILS, ROUTING_DECISION_RETENTION_MAX_COUNT,
};
use serde_json::json;
use sqlx::{sqlite::SqlitePoolOptions, Row, SqlitePool};

async fn test_pool() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("pool");
    sqlx::query(include_str!(
        "../src/persistence/migrations/0016_routing_decisions.sql"
    ))
    .execute(&pool)
    .await
    .expect("migration");
    pool
}

async fn connection(pool: &SqlitePool) -> sqlx::pool::PoolConnection<sqlx::Sqlite> {
    pool.acquire().await.expect("connection")
}

fn decision(
    id: &str,
    decided_at_ms: i64,
    candidates: Vec<RouteCandidateDecisionWrite>,
) -> RoutingDecisionWrite {
    let mut rejection_counts = BTreeMap::new();
    rejection_counts.insert("health_hard_reject".to_string(), 12);
    RoutingDecisionWrite {
        decision_id: id.to_string(),
        request_id: format!("request-{id}"),
        decided_at_ms,
        ordering_profile: "cost_first".to_string(),
        selected_station_key_id: Some("key-selected".to_string()),
        selected_station_id: Some("station-selected".to_string()),
        selected_endpoint_revision: Some(3),
        candidate_count: candidates.len() as u32,
        rejection_counts,
        snapshot_id: "snapshot-a".to_string(),
        fact_version_vector: "station=1,key=2,settings=3".to_string(),
        planner_version: "hierarchical_route_planner_v1".to_string(),
        projector_version: "route_candidate_projection_v1".to_string(),
        runtime_overlay_revision: 7,
        trace_status: RoutingTraceStatus::Complete,
        candidates,
    }
}

fn candidate(index: usize) -> RouteCandidateDecisionWrite {
    RouteCandidateDecisionWrite {
        station_key_id: format!("key-{index:03}"),
        station_id: format!("station-{index:03}"),
        endpoint_revision: 3,
        selected: index == 39,
        attempted: index == 38,
        primary_rejection_representative: index % 7 == 0,
        availability_tier: "primary".to_string(),
        hard_rejection_code: (index % 2 == 0).then(|| "health_hard_reject".to_string()),
        hard_rejection_gate: (index % 2 == 0).then(|| "health".to_string()),
        priority: index as i64,
        cost_basis: "exact_price".to_string(),
        cost_currency: Some("USD".to_string()),
        cost_unit: Some("per_1m_tokens".to_string()),
        cost_comparison_value: Some(index as f64),
        snapshot_id: "snapshot-a".to_string(),
        fact_version_vector: "station=1,key=2,settings=3".to_string(),
        evidence: json!({ "reason": "bounded_fixture", "rank": index }),
    }
}

#[tokio::test]
async fn writer_truncates_candidate_details_but_keeps_summary_and_priority_representatives() {
    let pool = test_pool().await;
    let mut connection = connection(&pool).await;
    let candidates = (0..40).map(candidate).collect::<Vec<_>>();

    let outcome = RoutingDecisionWriter
        .upsert_decision(
            &mut connection,
            &decision("decision-a", 10_000, candidates),
            10_001,
        )
        .await
        .expect("write");

    assert_eq!(
        outcome.candidate_detail_count,
        MAX_ROUTE_CANDIDATE_DECISION_DETAILS
    );
    assert!(outcome.candidate_detail_truncated);

    let page = RoutingDecisionQueries
        .list_decisions(&mut connection, None, 10)
        .await
        .expect("page");
    assert_eq!(page.rows[0].candidate_count, 40);
    assert_eq!(
        page.rows[0].candidate_detail_count,
        MAX_ROUTE_CANDIDATE_DECISION_DETAILS as u32
    );
    assert!(page.rows[0].candidate_detail_truncated);
    assert_eq!(page.rows[0].rejection_counts["health_hard_reject"], 12);

    let details = RoutingDecisionQueries
        .list_candidate_details(&mut connection, "decision-a", 64)
        .await
        .expect("details");
    assert_eq!(details.len(), MAX_ROUTE_CANDIDATE_DECISION_DETAILS);
    assert!(details.iter().any(|row| row.station_key_id == "key-039"));
    assert!(details.iter().any(|row| row.station_key_id == "key-038"));
    assert!(details
        .iter()
        .any(|row| row.retained_reason == "primary_rejection_representative"));
}

#[tokio::test]
async fn cursor_pagination_is_stable_without_offset_drift() {
    let pool = test_pool().await;
    let mut connection = connection(&pool).await;
    for index in 0..3 {
        RoutingDecisionWriter
            .upsert_decision(
                &mut connection,
                &decision(
                    &format!("decision-{index}"),
                    10_000 + index,
                    vec![candidate(index as usize)],
                ),
                20_000 + index,
            )
            .await
            .expect("write");
    }

    let first = RoutingDecisionQueries
        .list_decisions(&mut connection, None, 2)
        .await
        .expect("first");
    assert_eq!(
        first
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["decision-2", "decision-1"]
    );

    RoutingDecisionWriter
        .upsert_decision(
            &mut connection,
            &decision("decision-new", 20_000, vec![candidate(9)]),
            30_000,
        )
        .await
        .expect("insert between pages");

    let second = RoutingDecisionQueries
        .list_decisions(&mut connection, first.next_cursor.as_ref(), 2)
        .await
        .expect("second");
    assert_eq!(
        second
            .rows
            .iter()
            .map(|row| row.id.as_str())
            .collect::<Vec<_>>(),
        vec!["decision-0"]
    );
}

#[tokio::test]
async fn retention_enforces_age_and_count_with_bounded_batches() {
    let pool = test_pool().await;
    let mut connection = connection(&pool).await;
    for index in 0..12 {
        let decided_at_ms = if index < 3 { 1_000 } else { 100_000 + index };
        RoutingDecisionWriter
            .upsert_decision(
                &mut connection,
                &decision(
                    &format!("decision-{index:02}"),
                    decided_at_ms,
                    vec![candidate(index as usize)],
                ),
                200_000 + index,
            )
            .await
            .expect("write");
    }

    let outcome = RoutingDecisionRetention
        .enforce(&mut connection, 31 * 86_400_000, 2)
        .await
        .expect("retention");

    assert_eq!(outcome.deleted_decisions, 2);
    assert_eq!(outcome.deleted_candidate_details, 2);
    assert!(outcome.has_more);
}

#[tokio::test]
async fn retention_enforces_ten_thousand_decision_count_cap() {
    let pool = test_pool().await;
    let mut connection = connection(&pool).await;
    let total = ROUTING_DECISION_RETENTION_MAX_COUNT + 2;
    for index in 0..total {
        sqlx::query(
            r#"
            INSERT INTO route_decisions (
                id, request_id, decided_at_ms, ordering_profile,
                candidate_count, candidate_detail_count, candidate_detail_truncated,
                rejection_counts_json, snapshot_id, fact_version_vector,
                planner_version, projector_version, runtime_overlay_revision,
                trace_status, created_at_ms, updated_at_ms
            ) VALUES (?1, ?2, ?3, 'cost_first', 1, 1, 0, '{}', 'snapshot-a',
                'station=1,key=2,settings=3', 'hierarchical_route_planner_v1',
                'route_candidate_projection_v1', 7, 'complete', ?3, ?3)
            "#,
        )
        .bind(format!("decision-count-{index:05}"))
        .bind(format!("request-count-{index:05}"))
        .bind(i64::from(index))
        .execute(&mut *connection)
        .await
        .expect("decision insert");
        sqlx::query(
            r#"
            INSERT INTO route_candidate_decisions (
                id, decision_id, request_id, station_key_id, station_id,
                endpoint_revision, selected, attempted, retained_reason,
                availability_tier, priority, cost_basis, snapshot_id,
                fact_version_vector, evidence_json, created_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, 1, 0, 0, 'bounded_sample',
                'primary', ?6, 'exact_price', 'snapshot-a',
                'station=1,key=2,settings=3', '{"reason":"count_cap"}', ?6)
            "#,
        )
        .bind(format!("candidate-count-{index:05}"))
        .bind(format!("decision-count-{index:05}"))
        .bind(format!("request-count-{index:05}"))
        .bind(format!("key-count-{index:05}"))
        .bind(format!("station-count-{index:05}"))
        .bind(i64::from(index))
        .execute(&mut *connection)
        .await
        .expect("candidate insert");
    }

    let outcome = RoutingDecisionRetention
        .enforce(&mut connection, i64::from(total) + 86_400_000, 500)
        .await
        .expect("retention");

    assert_eq!(outcome.deleted_decisions, 2);
    assert_eq!(outcome.deleted_candidate_details, 2);
    assert!(!outcome.has_more);
    let remaining: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM route_decisions")
        .fetch_one(&mut *connection)
        .await
        .expect("remaining decisions");
    assert_eq!(remaining, i64::from(ROUTING_DECISION_RETENTION_MAX_COUNT));
    let oldest_deleted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM route_decisions WHERE id IN (?1, ?2)")
            .bind("decision-count-00000")
            .bind("decision-count-00001")
            .fetch_one(&mut *connection)
            .await
            .expect("oldest deleted");
    assert_eq!(oldest_deleted, 0);
}

#[tokio::test]
async fn million_candidate_rows_detail_query_uses_bounded_indexed_shape() {
    let pool = test_pool().await;
    let mut connection = connection(&pool).await;
    sqlx::query(
        r#"
        INSERT INTO route_decisions (
            id, request_id, decided_at_ms, ordering_profile,
            candidate_count, candidate_detail_count, candidate_detail_truncated,
            rejection_counts_json, snapshot_id, fact_version_vector,
            planner_version, projector_version, runtime_overlay_revision,
            trace_status, created_at_ms, updated_at_ms
        ) VALUES ('decision-million', 'request-million', 1, 'cost_first',
            1000000, 32, 1, '{}', 'snapshot-a', 'station=1,key=2,settings=3',
            'hierarchical_route_planner_v1', 'route_candidate_projection_v1',
            7, 'complete', 1, 1)
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("decision insert");

    sqlx::query(
        r#"
        WITH RECURSIVE candidate_number(value) AS (
            SELECT 0
            UNION ALL
            SELECT value + 1 FROM candidate_number WHERE value < 999999
        )
        INSERT INTO route_candidate_decisions (
            id, decision_id, request_id, station_key_id, station_id,
            endpoint_revision, selected, attempted, retained_reason,
            availability_tier, priority, cost_basis, snapshot_id,
            fact_version_vector, evidence_json, created_at_ms
        )
        SELECT
            'candidate-million-' || printf('%07d', value),
            'decision-million',
            'request-million',
            'key-million-' || printf('%07d', value),
            'station-million-' || printf('%07d', value),
            1,
            CASE WHEN value = 999999 THEN 1 ELSE 0 END,
            CASE WHEN value = 999998 THEN 1 ELSE 0 END,
            'bounded_sample',
            'primary',
            value,
            'exact_price',
            'snapshot-a',
            'station=1,key=2,settings=3',
            '{"reason":"million_fixture"}',
            1
        FROM candidate_number
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("million candidate insert");

    let plan_rows = sqlx::query(
        r#"
        EXPLAIN QUERY PLAN
        SELECT station_key_id, station_id, selected, attempted, retained_reason,
               hard_rejection_code, cost_basis, evidence_json
        FROM route_candidate_decisions
        WHERE decision_id = ?1
        ORDER BY selected DESC, attempted DESC, station_key_id ASC
        LIMIT ?2
        "#,
    )
    .bind("decision-million")
    .bind(64_i64)
    .fetch_all(&mut *connection)
    .await
    .expect("query plan");
    let query_plan = plan_rows
        .iter()
        .map(|row| row.get::<String, _>("detail"))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        query_plan.contains("idx_route_candidate_decisions_decision"),
        "{query_plan}"
    );

    let details = RoutingDecisionQueries
        .list_candidate_details(&mut connection, "decision-million", 64)
        .await
        .expect("details");
    assert_eq!(details.len(), 64);
    assert!(details[0].selected);
    assert_eq!(details[0].station_key_id, "key-million-0999999");
}

#[tokio::test]
async fn writer_rejects_secret_url_payload_and_raw_body_shapes() {
    let pool = test_pool().await;
    let mut connection = connection(&pool).await;
    let mut unsafe_candidate = candidate(1);
    unsafe_candidate.evidence = json!({
        "upstream": "https://provider.invalid/v1?token=secret"
    });

    let error = RoutingDecisionWriter
        .upsert_decision(
            &mut connection,
            &decision("decision-unsafe", 10_000, vec![unsafe_candidate]),
            10_001,
        )
        .await
        .expect_err("unsafe trace");

    assert!(format!("{error}").contains("unsafe high-cardinality"));
}
