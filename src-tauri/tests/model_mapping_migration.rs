use sqlx::{Connection, SqliteConnection};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn model_mapping_rejection_metadata_is_current_and_constrained() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("open sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("enable foreign keys");
    MIGRATOR.run(&mut connection).await.expect("run migrations");

    let current_schema: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema version");
    assert_eq!(
        current_schema,
        MIGRATOR
            .iter()
            .map(|migration| migration.version)
            .max()
            .expect("migration catalog")
    );

    for column in ["rejection_kind", "rejection_message"] {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM pragma_table_info('model_mapping_rules') WHERE name = ?1",
        )
        .bind(column)
        .fetch_one(&mut connection)
        .await
        .expect("model mapping column lookup");
        assert_eq!(count, 1, "missing model mapping column {column}");
    }

    sqlx::query(
        "INSERT INTO model_mapping_rules
            (id, priority, enabled, matcher_kind, matcher_value,
             endpoint_conditions_json, stream_condition, tools_condition,
             vision_condition, reasoning_condition, action_kind, fallback_trigger,
             rejection_kind, rejection_message, note, created_at_ms, updated_at_ms, revision)
         VALUES ('migration-rejection-test', 10, 1, 'exact', 'fixture-model',
                 '[]', 'any', 'any', 'any', 'any', 'reject', NULL,
                 'unsupported_model', 'fixture rejection', NULL, 1, 1, 1)",
    )
    .execute(&mut connection)
    .await
    .expect("valid rejection metadata");

    let unknown_kind = sqlx::query(
        "INSERT INTO model_mapping_rules
            (id, priority, enabled, matcher_kind, matcher_value,
             endpoint_conditions_json, stream_condition, tools_condition,
             vision_condition, reasoning_condition, action_kind, fallback_trigger,
             rejection_kind, rejection_message, note, created_at_ms, updated_at_ms, revision)
         VALUES ('migration-rejection-invalid-kind', 10, 1, 'exact', 'fixture-model',
                 '[]', 'any', 'any', 'any', 'any', 'reject', NULL,
                 'scanner', NULL, NULL, 1, 1, 1)",
    )
    .execute(&mut connection)
    .await;
    assert!(
        unknown_kind.is_err(),
        "unknown rejection kind must be rejected"
    );

    let oversized_message = sqlx::query(
        "INSERT INTO model_mapping_rules
            (id, priority, enabled, matcher_kind, matcher_value,
             endpoint_conditions_json, stream_condition, tools_condition,
             vision_condition, reasoning_condition, action_kind, fallback_trigger,
             rejection_kind, rejection_message, note, created_at_ms, updated_at_ms, revision)
         VALUES ('migration-rejection-oversized-message', 10, 1, 'exact', 'fixture-model',
                 '[]', 'any', 'any', 'any', 'any', 'reject', NULL,
                 'policy_denied', ?1, NULL, 1, 1, 1)",
    )
    .bind("x".repeat(257))
    .execute(&mut connection)
    .await;
    assert!(
        oversized_message.is_err(),
        "oversized rejection message must be rejected"
    );
}
