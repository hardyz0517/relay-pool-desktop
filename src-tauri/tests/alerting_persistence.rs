use sqlx::{Connection, Row, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn alerting_foundation_creates_contract_and_indexes() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    MIGRATOR.run(&mut connection).await.expect("all migrations");

    let schema_version = sqlx::query_scalar::<_, i64>(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema version");
    // Schema 40 adds persistent attention state; schema 41 adds published-status facts;
    // schema 42 normalizes legacy missing groups into informational changes.
    assert_eq!(schema_version, 42);

    let legacy_table_exists = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'change_events'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy table lookup");
    assert_eq!(
        legacy_table_exists, 0,
        "legacy table must be removed at latest schema"
    );

    for table in [
        "change_event_occurrences",
        "change_incidents",
        "incident_attention",
        "alert_policies",
        "notification_deliveries",
        "alerting_upgrade_progress",
    ] {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1",
        )
        .bind(table)
        .fetch_one(&mut connection)
        .await
        .expect("table lookup");
        assert_eq!(exists, 1, "missing table {table}");
    }

    for index in [
        "idx_change_incidents_lifecycle_severity_updated",
        "idx_change_event_occurrences_incident_episode_observed",
        "idx_change_event_occurrences_audit_unseen_observed",
        "idx_alert_policies_enabled_scope_priority",
        "idx_notification_deliveries_status_scheduled",
        "idx_notification_deliveries_delivery_key",
    ] {
        let exists = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
        )
        .bind(index)
        .fetch_one(&mut connection)
        .await
        .expect("index lookup");
        assert_eq!(exists, 1, "missing index {index}");
    }

    let columns = sqlx::query("PRAGMA table_info('change_incidents')")
        .fetch_all(&mut connection)
        .await
        .expect("incident columns");
    assert!(columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "last_observation_summary_json"));
    assert!(columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "fact_fresh_until_ms"));

    let occurrence_columns = sqlx::query("PRAGMA table_info('change_event_occurrences')")
        .fetch_all(&mut connection)
        .await
        .expect("occurrence columns");
    assert!(occurrence_columns
        .iter()
        .any(|row| row.get::<String, _>("name") == "seen_at_ms"));

    let progress_phase = sqlx::query_scalar::<_, String>(
        "SELECT phase FROM alerting_upgrade_progress WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("progress lookup");
    assert_eq!(progress_phase, "not_started");
}
