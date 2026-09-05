use std::borrow::Cow;

use sqlx::{migrate::Migrator, Connection, Row, SqliteConnection};

static MIGRATOR: Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn schema_75_adds_nullable_resolved_models_without_rewriting_history() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    migrator_through(74)
        .run(&mut connection)
        .await
        .expect("schema 74");

    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, model, status, created_at
         ) VALUES (
            'historical', 'historical', '1', 'POST', '/v1/chat/completions',
            'chat_completions', 'client-model', 'success', '1'
         )",
    )
    .execute(&mut connection)
    .await
    .expect("historical request log");
    sqlx::query(
        "INSERT INTO request_attempts (
            request_id, ordinal, station_id, station_key_id, endpoint_revision,
            started_at_ms, terminal_kind, health_effect, output_committed, terminal_at_ms
         ) VALUES (
            'historical', 0, 'station-1', 'key-1', 1,
            1, 'succeeded', 'success', 1, 2
         )",
    )
    .execute(&mut connection)
    .await
    .expect("historical request attempt");

    migrator_through(75)
        .run(&mut connection)
        .await
        .expect("upgrade schema 74 to schema 75");

    let versions = sqlx::query(
        "SELECT
            (SELECT schema_version FROM persistence_schema_compatibility
             WHERE singleton_key = 1) AS compatibility_version,
            (SELECT MAX(version) FROM _sqlx_migrations) AS sqlx_version",
    )
    .fetch_one(&mut connection)
    .await
    .expect("schema versions");
    assert_eq!(versions.get::<i64, _>("compatibility_version"), 75);
    assert_eq!(versions.get::<i64, _>("sqlx_version"), 75);

    for table in ["request_logs", "request_attempts"] {
        let column = sqlx::query(
            "SELECT name, \"notnull\" FROM pragma_table_info(?1)
             WHERE name = 'resolved_upstream_model'",
        )
        .bind(table)
        .fetch_one(&mut connection)
        .await
        .expect("resolved model column");
        assert_eq!(column.get::<String, _>("name"), "resolved_upstream_model");
        assert_eq!(column.get::<i64, _>("notnull"), 0);
    }

    let historical: (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT
            (SELECT resolved_upstream_model FROM request_logs WHERE id = 'historical'),
            (SELECT resolved_upstream_model FROM request_attempts
             WHERE request_id = 'historical' AND ordinal = 0)",
    )
    .fetch_one(&mut connection)
    .await
    .expect("historical resolved models");
    assert_eq!(historical, (None, None));

    sqlx::query(
        "UPDATE request_attempts SET resolved_upstream_model = 'native-model'
         WHERE request_id = 'historical' AND ordinal = 0",
    )
    .execute(&mut connection)
    .await
    .expect("write attempt resolved model");
    sqlx::query(
        "UPDATE request_logs SET resolved_upstream_model = 'native-model'
         WHERE id = 'historical'",
    )
    .execute(&mut connection)
    .await
    .expect("write request resolved model");

    let persisted: (String, String) = sqlx::query_as(
        "SELECT
            (SELECT resolved_upstream_model FROM request_logs WHERE id = 'historical'),
            (SELECT resolved_upstream_model FROM request_attempts
             WHERE request_id = 'historical' AND ordinal = 0)",
    )
    .fetch_one(&mut connection)
    .await
    .expect("persisted resolved models");
    assert_eq!(
        persisted,
        ("native-model".to_string(), "native-model".to_string())
    );
}

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
