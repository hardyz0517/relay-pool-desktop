use std::borrow::Cow;

use sqlx::{migrate::Migrator, Connection, Row, SqliteConnection};

static MIGRATOR: Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn schema_39_upgrades_information_changes_with_nullable_seen_state() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");

    migrator_through(39)
        .run(&mut connection)
        .await
        .expect("schema 39");
    sqlx::query(
        "INSERT INTO change_event_occurrences (
            id, source_observation_key, event_type, category, observation_kind,
            severity, object_type, source, observed_at_ms, created_at_ms
         ) VALUES ('historical-info', 'fixture:historical-info', 'audit_change',
                   'audit_change', 'change', 'info', 'global', 'fixture', 1, 1)",
    )
    .execute(&mut connection)
    .await
    .expect("historical informational change");

    MIGRATOR
        .run(&mut connection)
        .await
        .expect("upgrade to latest schema");

    let schema_version: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("compatibility schema");
    assert_eq!(schema_version, 42);

    let column = sqlx::query(
        "SELECT name, \"notnull\" FROM pragma_table_info('change_event_occurrences') WHERE name = 'seen_at_ms'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("seen state column");
    assert_eq!(column.get::<String, _>("name"), "seen_at_ms");
    assert_eq!(column.get::<i64, _>("notnull"), 0);

    let historical_seen_at: Option<i64> = sqlx::query_scalar(
        "SELECT seen_at_ms FROM change_event_occurrences WHERE id = 'historical-info'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("historical seen state");
    assert_eq!(historical_seen_at, None);

    sqlx::query("UPDATE change_event_occurrences SET seen_at_ms = 2 WHERE id = 'historical-info'")
        .execute(&mut connection)
        .await
        .expect("valid seen state");
    assert!(sqlx::query(
        "UPDATE change_event_occurrences SET seen_at_ms = -1 WHERE id = 'historical-info'",
    )
    .execute(&mut connection)
    .await
    .is_err());
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
