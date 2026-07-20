use sqlx::{Connection, Row, SqliteConnection};

#[tokio::test]
async fn schema_eight_supports_bounded_deterministic_growth_queries() {
    let mut connection = migrated_connection().await;
    seed_station(&mut connection).await;

    sqlx::query(
        r#"
        INSERT INTO channel_monitor_request_templates (
            id, name, endpoint_kind, method, path, request_body_json,
            enabled, built_in, created_at, updated_at
        ) VALUES ('template-1', 'Chat', 'chat', 'POST', '/v1/chat/completions', '{}', 1, 0, '1', '1')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("template");
    sqlx::query(
        r#"
        INSERT INTO channel_monitors (
            id, name, target_type, station_id, station_key_id, template_id,
            enabled, interval_seconds, jitter_seconds, timeout_seconds,
            max_concurrency, consecutive_failure_threshold, fallback_models_json,
            next_run_at, created_at, updated_at
        ) VALUES ('monitor-1', 'Primary', 'station', 'station-1', NULL, 'template-1',
                  1, 30, 5, 15, 1, 3, '[]', '1000', '1', '1')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("monitor");

    for id in ["run-a", "run-c", "run-b"] {
        sqlx::query(
            r#"
            INSERT INTO channel_monitor_runs (
                id, monitor_id, template_id, station_id, status, started_at, created_at
            ) VALUES (?1, 'monitor-1', 'template-1', 'station-1', 'success', '2000', '2000')
            "#,
        )
        .bind(id)
        .execute(&mut connection)
        .await
        .expect("monitor run");
    }

    let first = monitor_run_ids(&mut connection, None, 2).await;
    let second = monitor_run_ids(&mut connection, Some((2_000, "run-b")), 2).await;
    assert_eq!(first, vec!["run-c", "run-b"]);
    assert_eq!(second, vec!["run-a"]);

    for id in ["event-a", "event-c", "event-b"] {
        sqlx::query(
            r#"
            INSERT INTO change_events (
                id, severity, event_type, status, title, message, object_type,
                dedupe_key, source, detected_at, created_at, updated_at
            ) VALUES (?1, 'info', 'rate_changed', 'unread', 'Rate', 'Changed',
                      'station', ?1, 'collector', '3000', '3000', '3000')
            "#,
        )
        .bind(id)
        .execute(&mut connection)
        .await
        .expect("change event");
    }
    let event_ids =
        sqlx::query("SELECT id FROM change_events ORDER BY updated_at DESC, id DESC LIMIT 2")
            .fetch_all(&mut connection)
            .await
            .expect("change page")
            .into_iter()
            .map(|row| row.get::<String, _>("id"))
            .collect::<Vec<_>>();
    assert_eq!(event_ids, vec!["event-c", "event-b"]);
}

#[tokio::test]
async fn latest_pricing_evidence_has_explicit_tie_breakers() {
    let mut connection = migrated_connection().await;
    seed_station(&mut connection).await;

    for (id, value) in [("balance-a", 10.0), ("balance-b", 20.0)] {
        sqlx::query(
            r#"
            INSERT INTO balance_snapshots (
                id, station_id, scope, value, currency, status, source, confidence,
                created_at, updated_at
            ) VALUES (?1, 'station-1', 'station', ?2, 'CNY', 'normal', 'fixture', 1.0, '4000', '4000')
            "#,
        )
        .bind(id)
        .bind(value)
        .execute(&mut connection)
        .await
        .expect("balance");
    }
    let latest_balance = sqlx::query(
        r#"
        SELECT id, value
        FROM balance_snapshots INDEXED BY idx_balance_snapshots_latest_station_scope
        WHERE station_id = 'station-1' AND scope = 'station'
        ORDER BY updated_at DESC, created_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .fetch_one(&mut connection)
    .await
    .expect("latest balance");
    assert_eq!(latest_balance.get::<String, _>("id"), "balance-b");
    assert_eq!(latest_balance.get::<f64, _>("value"), 20.0);

    sqlx::query(
        r#"
        INSERT INTO station_group_bindings (
            id, station_id, binding_kind, group_key_hash, group_name, binding_status,
            confidence, created_at, updated_at
        ) VALUES ('binding-1', 'station-1', 'station_group', 'hash-1', 'default',
                  'available', 1.0, '1', '1')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("group binding");
    for (id, multiplier) in [("rate-a", 1.0), ("rate-b", 0.5)] {
        sqlx::query(
            r#"
            INSERT INTO group_rate_records (
                id, station_id, group_binding_id, binding_kind, group_key_hash,
                group_name, effective_rate_multiplier, source, confidence,
                checked_at, created_at
            ) VALUES (?1, 'station-1', 'binding-1', 'station_group', 'hash-1',
                      'default', ?2, 'fixture', 1.0, '5000', '5000')
            "#,
        )
        .bind(id)
        .bind(multiplier)
        .execute(&mut connection)
        .await
        .expect("group rate");
    }
    let latest_rate = sqlx::query(
        r#"
        SELECT id, effective_rate_multiplier
        FROM group_rate_records
        WHERE group_binding_id = 'binding-1'
        ORDER BY checked_at DESC, created_at DESC, id DESC
        LIMIT 1
        "#,
    )
    .fetch_one(&mut connection)
    .await
    .expect("latest group rate");
    assert_eq!(latest_rate.get::<String, _>("id"), "rate-b");
    assert_eq!(latest_rate.get::<f64, _>("effective_rate_multiplier"), 0.5);
}

#[tokio::test]
async fn schema_eight_query_plans_use_growth_indexes() {
    let mut connection = migrated_connection().await;
    let cases = [
        (
            "EXPLAIN QUERY PLAN SELECT id FROM channel_monitor_runs WHERE monitor_id = 'm' ORDER BY CAST(started_at AS INTEGER) DESC, id DESC LIMIT 20",
            "idx_channel_monitor_runs_monitor_started",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM channel_monitors WHERE enabled = 1 AND (next_run_at IS NULL OR CAST(next_run_at AS INTEGER) <= 1000) ORDER BY COALESCE(CAST(next_run_at AS INTEGER), 0) ASC, id ASC LIMIT 20",
            "idx_channel_monitors_due",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM change_events ORDER BY updated_at DESC, id DESC LIMIT 20",
            "idx_change_events_page",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM balance_snapshots WHERE station_id = 's' AND scope = 'station' ORDER BY updated_at DESC, created_at DESC, id DESC LIMIT 1",
            "idx_balance_snapshots_latest_station_scope",
        ),
        (
            "EXPLAIN QUERY PLAN SELECT id FROM pricing_rules INDEXED BY idx_pricing_rules_comparison ORDER BY enabled DESC, station_id ASC, model ASC, updated_at DESC, created_at DESC, id DESC LIMIT 20",
            "idx_pricing_rules_comparison",
        ),
    ];
    for (sql, expected_index) in cases {
        let details = sqlx::query(sql)
            .fetch_all(&mut connection)
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
}

#[tokio::test]
async fn schema_eight_preserves_released_login_and_endpoint_health_fields() {
    let mut connection = migrated_connection().await;
    seed_station(&mut connection).await;

    sqlx::query(
        r#"
        INSERT INTO station_credentials (
            station_id, login_username, remember_password, login_status,
            session_status, session_source, created_at, updated_at
        ) VALUES ('station-1', 'released-user', 1, 'logged_in',
                  'active', 'web', '100', '200')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("station credentials");
    sqlx::query(
        r#"
        INSERT INTO station_endpoint_health (
            station_id, endpoint_revision, status, latency_ms,
            checked_at, error_summary, updated_at
        ) VALUES ('station-1', 1, 'failed', 321, '300', 'timeout', '400')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("station endpoint health");

    let credentials = sqlx::query(
        "SELECT login_username, created_at FROM station_credentials WHERE station_id = 'station-1'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read station credentials");
    assert_eq!(
        credentials.get::<String, _>("login_username"),
        "released-user"
    );
    assert_eq!(credentials.get::<String, _>("created_at"), "100");

    let health = sqlx::query(
        "SELECT status, latency_ms, checked_at, error_summary, updated_at
         FROM station_endpoint_health WHERE station_id = 'station-1'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("read station endpoint health");
    assert_eq!(health.get::<String, _>("status"), "failed");
    assert_eq!(health.get::<i64, _>("latency_ms"), 321);
    assert_eq!(health.get::<String, _>("checked_at"), "300");
    assert_eq!(health.get::<String, _>("error_summary"), "timeout");
    assert_eq!(health.get::<String, _>("updated_at"), "400");
}

async fn migrated_connection() -> SqliteConnection {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    for migration in [
        include_str!("../src/persistence/migrations/0001_v2_initial.sql"),
        include_str!("../src/persistence/migrations/0002_catalog_settings.sql"),
        include_str!("../src/persistence/migrations/0003_credentials_keys.sql"),
        include_str!("../src/persistence/migrations/0004_routing.sql"),
        include_str!("../src/persistence/migrations/0005_request_logs.sql"),
        include_str!("../src/persistence/migrations/0006_collectors_changes.sql"),
        include_str!("../src/persistence/migrations/0007_pricing_monitoring.sql"),
        include_str!("../src/persistence/migrations/0008_legacy_parity.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&mut connection)
            .await
            .expect("migration");
    }
    let schema_version = sqlx::query_scalar::<_, i64>(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema version");
    assert_eq!(schema_version, 8);
    connection
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
    .execute(connection)
    .await
    .expect("station");
}

async fn monitor_run_ids(
    connection: &mut SqliteConnection,
    cursor: Option<(i64, &str)>,
    limit: i64,
) -> Vec<String> {
    let rows = if let Some((started_at, id)) = cursor {
        sqlx::query(
            r#"
            SELECT id FROM channel_monitor_runs
            WHERE monitor_id = 'monitor-1'
              AND (CAST(started_at AS INTEGER) < ?1
                   OR (CAST(started_at AS INTEGER) = ?1 AND id < ?2))
            ORDER BY CAST(started_at AS INTEGER) DESC, id DESC
            LIMIT ?3
            "#,
        )
        .bind(started_at)
        .bind(id)
        .bind(limit)
        .fetch_all(&mut *connection)
        .await
        .expect("next monitor page")
    } else {
        sqlx::query(
            r#"
            SELECT id FROM channel_monitor_runs
            WHERE monitor_id = 'monitor-1'
            ORDER BY CAST(started_at AS INTEGER) DESC, id DESC
            LIMIT ?1
            "#,
        )
        .bind(limit)
        .fetch_all(&mut *connection)
        .await
        .expect("first monitor page")
    };
    rows.into_iter()
        .map(|row| row.get::<String, _>("id"))
        .collect()
}
