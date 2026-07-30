use sqlx::{Connection, Row, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn status_monitoring_v2_fresh_migrator_reaches_current_schema() {
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
        .expect("fresh v2 migrations");

    let schema_version = sqlx::query_scalar::<_, i64>(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema version");
    let sqlx_version = sqlx::query_scalar::<_, i64>("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut connection)
        .await
        .expect("sqlx migration version");
    assert_eq!(schema_version, sqlx_version);
    assert!(
        schema_version >= 10,
        "Monitoring V2 requires migration 0010"
    );

    let execution_table = sqlx::query_scalar::<_, String>(
        "SELECT name FROM sqlite_master WHERE type = 'table' AND name = 'channel_monitor_executions'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("execution table");
    assert_eq!(execution_table, "channel_monitor_executions");
}

#[tokio::test]
async fn status_monitoring_v2_migration_backfills_legacy_runs_without_health_observations() {
    let mut connection = migrate_to_v9().await;
    seed_station_monitor_and_legacy_run(&mut connection).await;

    sqlx::raw_sql(include_str!(
        "../src/persistence/migrations/0010_status_monitoring_v2.sql"
    ))
    .execute(&mut connection)
    .await
    .expect("status monitoring v2 migration");

    let schema_version = sqlx::query_scalar::<_, i64>(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema version");
    assert_eq!(schema_version, 10);

    let monitor = sqlx::query(
        r#"
        SELECT primary_model, fallback_models_v2_json, next_due_at_ms,
               protocol_kind, client_profile_id, client_profile_version,
               attempt_timeout_ms, execution_timeout_ms, schedule_revision
        FROM channel_monitors
        WHERE id = 'monitor-1'
        "#,
    )
    .fetch_one(&mut connection)
    .await
    .expect("monitor v2 fields");
    assert_eq!(monitor.get::<String, _>("primary_model"), "gpt-primary");
    assert_eq!(
        monitor.get::<String, _>("fallback_models_v2_json"),
        r#"["gpt-fallback"]"#
    );
    assert_eq!(monitor.get::<i64, _>("next_due_at_ms"), 1700000060000);
    assert_eq!(monitor.get::<String, _>("protocol_kind"), "generic_open_ai");
    assert_eq!(
        monitor.get::<String, _>("client_profile_id"),
        "standard_api"
    );
    assert_eq!(monitor.get::<i64, _>("client_profile_version"), 1);
    assert_eq!(monitor.get::<i64, _>("attempt_timeout_ms"), 14000);
    assert_eq!(monitor.get::<i64, _>("execution_timeout_ms"), 15000);
    assert_eq!(monitor.get::<i64, _>("schedule_revision"), 1);

    let joined = sqlx::query(
        r#"
        SELECT e.id AS execution_id, e.trigger_kind, e.status AS execution_status,
               e.summary_outcome, t.id AS target_result_id, t.terminal_outcome,
               t.semantic_confidence, t.health_writeback_decision,
               a.id AS attempt_id, a.outcome AS attempt_outcome,
               a.validation_passed
        FROM channel_monitor_executions e
        JOIN channel_monitor_target_results t ON t.execution_id = e.id
        JOIN channel_monitor_attempts a ON a.id = t.decisive_attempt_id
        WHERE e.id = 'legacy-execution-run-1'
        "#,
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy backfill join");
    assert_eq!(joined.get::<String, _>("trigger_kind"), "legacy_import");
    assert_eq!(joined.get::<String, _>("execution_status"), "completed");
    assert_eq!(joined.get::<String, _>("summary_outcome"), "available");
    assert_eq!(joined.get::<String, _>("terminal_outcome"), "available");
    assert_eq!(
        joined.get::<String, _>("semantic_confidence"),
        "legacy_http_only"
    );
    assert_eq!(
        joined.get::<String, _>("health_writeback_decision"),
        "suppressed"
    );
    assert_eq!(joined.get::<String, _>("attempt_outcome"), "available");
    assert_eq!(joined.get::<i64, _>("validation_passed"), 0);

    let health_observations =
        sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM station_key_health_observations")
            .fetch_one(&mut connection)
            .await
            .expect("health observation count");
    assert_eq!(health_observations, 0);
}

#[tokio::test]
async fn status_monitoring_v2_constraints_reject_invalid_outcomes_and_duplicate_targets() {
    let mut connection = migrate_to_v10().await;
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
        ) VALUES ('monitor-1', 'Primary', 'station_key', 'station-1', 'key-1', 'template-1',
                  1, 60, 5, 15, 1, 3, '["gpt-primary"]', '1700000060000', '1', '1')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("monitor");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_executions (
            id, monitor_id, trigger_kind, status, planned_at_ms,
            config_snapshot_hash, created_at_ms
        ) VALUES ('execution-1', 'monitor-1', 'manual', 'running', 1, 'hash', 1)
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("execution");

    let invalid_attempt = sqlx::query(
        r#"
        INSERT INTO channel_monitor_attempts (
            id, execution_id, monitor_id, station_id, station_key_id, model,
            model_role, protocol_kind, client_profile_id, client_profile_version,
            request_profile_hash, transport_mode, started_at_ms, outcome, created_at_ms
        ) VALUES ('attempt-invalid', 'execution-1', 'monitor-1', 'station-1', 'key-1',
                  'gpt-primary', 'primary', 'generic_open_ai', 'standard_api', 1,
                  'hash', 'warm', 1, 'greenish', 1)
        "#,
    )
    .execute(&mut connection)
    .await;
    assert!(invalid_attempt.is_err());

    sqlx::query(
        r#"
        INSERT INTO channel_monitor_attempts (
            id, execution_id, monitor_id, station_id, station_key_id, model,
            model_role, protocol_kind, client_profile_id, client_profile_version,
            request_profile_hash, transport_mode, started_at_ms, outcome, created_at_ms
        ) VALUES ('attempt-1', 'execution-1', 'monitor-1', 'station-1', 'key-1',
                  'gpt-primary', 'primary', 'generic_open_ai', 'standard_api', 1,
                  'hash', 'warm', 1, 'available', 1)
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("attempt");

    for id in ["target-1", "target-duplicate"] {
        let result = sqlx::query(
            r#"
            INSERT INTO channel_monitor_target_results (
                id, execution_id, monitor_id, station_id, station_key_id,
                terminal_outcome, requested_model, effective_model, attempt_count,
                decisive_attempt_id, protocol_kind, resolved_adapter_kind,
                client_profile_id, client_profile_version, request_profile_hash,
                traffic_equivalence, health_writeback_mode, health_writeback_decision,
                semantic_confidence, started_at_ms, created_at_ms
            ) VALUES (?1, 'execution-1', 'monitor-1', 'station-1', 'key-1',
                      'available', 'gpt-primary', 'gpt-primary', 1, 'attempt-1',
                      'generic_open_ai', 'generic_open_ai', 'standard_api', 1,
                      'hash', 'standard_api', 'observe_only', 'observe_only',
                      'protocol_validated', 1, 1)
            "#,
        )
        .bind(id)
        .execute(&mut connection)
        .await;
        if id == "target-1" {
            result.expect("first target result");
        } else {
            assert!(result.is_err(), "duplicate target result must fail");
        }
    }
}

#[tokio::test]
async fn monitor_timeout_migration_only_upgrades_legacy_key_pool_defaults() {
    let mut connection = migrate_to_v14().await;
    seed_station(&mut connection).await;
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_request_templates (
            id, name, endpoint_kind, method, path, request_body_json,
            enabled, built_in, created_at, updated_at
        ) VALUES ('template-timeout', 'Responses', 'responses', 'POST', '/v1/responses', '{}', 1, 0, '1', '1')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("template");

    for (id, attempt_timeout_ms, execution_timeout_ms) in [
        ("legacy-default", 10_000, 30_000),
        ("custom", 20_000, 40_000),
    ] {
        sqlx::query(
            r#"
            INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                next_run_at, created_at, updated_at, note,
                attempt_timeout_ms, execution_timeout_ms
            ) VALUES (?1, ?1, 'station_key', 'station-1', 'key-1', 'template-timeout',
                      1, 300, 15, 30, 1, 3, '[]', '1', '1', '1',
                      '由密钥池监控开关创建', ?2, ?3)
            "#,
        )
        .bind(id)
        .bind(attempt_timeout_ms)
        .bind(execution_timeout_ms)
        .execute(&mut connection)
        .await
        .expect("monitor");
    }

    sqlx::raw_sql(include_str!(
        "../src/persistence/migrations/0015_monitor_probe_timeout_defaults.sql"
    ))
    .execute(&mut connection)
    .await
    .expect("migration 0015");

    let legacy = sqlx::query(
        "SELECT attempt_timeout_ms, execution_timeout_ms FROM channel_monitors WHERE id = 'legacy-default'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy monitor");
    assert_eq!(legacy.get::<i64, _>("attempt_timeout_ms"), 30_000);
    assert_eq!(legacy.get::<i64, _>("execution_timeout_ms"), 45_000);

    let custom = sqlx::query(
        "SELECT attempt_timeout_ms, execution_timeout_ms FROM channel_monitors WHERE id = 'custom'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("custom monitor");
    assert_eq!(custom.get::<i64, _>("attempt_timeout_ms"), 20_000);
    assert_eq!(custom.get::<i64, _>("execution_timeout_ms"), 40_000);
}

#[tokio::test]
async fn monitor_sub2api_latency_migration_only_upgrades_key_pool_defaults() {
    let mut connection = migrate_to_v15().await;
    seed_station(&mut connection).await;
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_request_templates (
            id, name, endpoint_kind, method, path, request_body_json,
            enabled, built_in, created_at, updated_at
        ) VALUES ('template-timeout', 'Responses', 'responses', 'POST', '/v1/responses', '{}', 1, 0, '1', '1')
        "#,
    )
    .execute(&mut connection)
    .await
    .expect("template");

    for (id, note, attempt_timeout_ms, execution_timeout_ms) in [
        ("key-pool-default", "由密钥池监控开关创建", 30_000, 45_000),
        ("custom-budget", "由密钥池监控开关创建", 30_000, 50_000),
        ("manual-monitor", "手动创建", 30_000, 45_000),
    ] {
        sqlx::query(
            r#"
            INSERT INTO channel_monitors (
                id, name, target_type, station_id, station_key_id, template_id,
                enabled, interval_seconds, jitter_seconds, timeout_seconds,
                max_concurrency, consecutive_failure_threshold, fallback_models_json,
                next_run_at, created_at, updated_at, note,
                attempt_timeout_ms, execution_timeout_ms
            ) VALUES (?1, ?1, 'station_key', 'station-1', 'key-1', 'template-timeout',
                      1, 300, 15, 45, 1, 3, '[]', '1', '1', '1',
                      ?2, ?3, ?4)
            "#,
        )
        .bind(id)
        .bind(note)
        .bind(attempt_timeout_ms)
        .bind(execution_timeout_ms)
        .execute(&mut connection)
        .await
        .expect("monitor");
    }

    sqlx::raw_sql(include_str!(
        "../src/persistence/migrations/0016_monitor_sub2api_latency_defaults.sql"
    ))
    .execute(&mut connection)
    .await
    .expect("migration 0016");

    let key_pool = sqlx::query(
        "SELECT attempt_timeout_ms, execution_timeout_ms FROM channel_monitors WHERE id = 'key-pool-default'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("key pool monitor");
    assert_eq!(key_pool.get::<i64, _>("attempt_timeout_ms"), 45_000);
    assert_eq!(key_pool.get::<i64, _>("execution_timeout_ms"), 60_000);

    let custom = sqlx::query(
        "SELECT attempt_timeout_ms, execution_timeout_ms FROM channel_monitors WHERE id = 'custom-budget'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("custom monitor");
    assert_eq!(custom.get::<i64, _>("attempt_timeout_ms"), 30_000);
    assert_eq!(custom.get::<i64, _>("execution_timeout_ms"), 50_000);

    let manual = sqlx::query(
        "SELECT attempt_timeout_ms, execution_timeout_ms FROM channel_monitors WHERE id = 'manual-monitor'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("manual monitor");
    assert_eq!(manual.get::<i64, _>("attempt_timeout_ms"), 30_000);
    assert_eq!(manual.get::<i64, _>("execution_timeout_ms"), 45_000);
}

async fn migrate_to_v15() -> SqliteConnection {
    let mut connection = migrate_to_v14().await;
    sqlx::raw_sql(include_str!(
        "../src/persistence/migrations/0015_monitor_probe_timeout_defaults.sql"
    ))
    .execute(&mut connection)
    .await
    .expect("migration 0015");
    connection
}

async fn migrate_to_v14() -> SqliteConnection {
    let mut connection = migrate_to_v10().await;
    for migration in [
        include_str!("../src/persistence/migrations/0011_remote_key_one_to_one.sql"),
        include_str!("../src/persistence/migrations/0012_seed_builtin_monitor_templates.sql"),
        include_str!("../src/persistence/migrations/0013_remote_key_discovery_order.sql"),
        include_str!("../src/persistence/migrations/0014_monitor_profile_v2.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&mut connection)
            .await
            .expect("migration");
    }
    connection
}

async fn migrate_to_v10() -> SqliteConnection {
    let mut connection = migrate_to_v9().await;
    sqlx::raw_sql(include_str!(
        "../src/persistence/migrations/0010_status_monitoring_v2.sql"
    ))
    .execute(&mut connection)
    .await
    .expect("migration 0010");
    connection
}

async fn migrate_to_v9() -> SqliteConnection {
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
        include_str!("../src/persistence/migrations/0009_provider_drafts.sql"),
    ] {
        sqlx::raw_sql(migration)
            .execute(&mut connection)
            .await
            .expect("migration");
    }
    connection
}

async fn seed_station_monitor_and_legacy_run(connection: &mut SqliteConnection) {
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
                  1, 60, 5, 15, 1, 3, '["gpt-primary", "gpt-fallback", "gpt-fallback"]',
                  '1700000060000', '1', '1')
        "#,
    )
    .execute(&mut *connection)
    .await
    .expect("monitor");
    sqlx::query(
        r#"
        INSERT INTO channel_monitor_runs (
            id, monitor_id, template_id, station_id, station_key_id, status,
            started_at, finished_at, duration_ms, http_status, latency_ms,
            response_model, created_at
        ) VALUES ('run-1', 'monitor-1', 'template-1', 'station-1', 'key-1',
                  'success', '1700000000000', '1700000001234', 1234, 200, 321,
                  'gpt-primary', '1700000000000')
        "#,
    )
    .execute(connection)
    .await
    .expect("legacy run");
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
    sqlx::query("INSERT INTO station_keys (id, station_id) VALUES ('key-1', 'station-1')")
        .execute(connection)
        .await
        .expect("station key");
}
