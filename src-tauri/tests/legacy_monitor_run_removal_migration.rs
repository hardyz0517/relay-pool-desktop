use std::borrow::Cow;

use sqlx::{migrate::Migrator, Connection, SqliteConnection};

static MIGRATOR: Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn schema_33_removes_the_legacy_monitor_run_table_at_schema_34() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");

    migrator_through(33)
        .run(&mut connection)
        .await
        .expect("schema 33");
    let legacy_table_before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'channel_monitor_runs'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy table before migration");
    assert_eq!(legacy_table_before, 1);

    migrator_through(34)
        .run(&mut connection)
        .await
        .expect("upgrade through schema 34");

    let legacy_table_after: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'channel_monitor_runs'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("legacy table after migration");
    assert_eq!(legacy_table_after, 0);

    let schema_version: i64 = sqlx::query_scalar(
        "SELECT schema_version FROM persistence_schema_compatibility WHERE singleton_key = 1",
    )
    .fetch_one(&mut connection)
    .await
    .expect("compatibility schema");
    let sqlx_version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut connection)
        .await
        .expect("migration ledger");
    assert_eq!(schema_version, 34);
    assert_eq!(sqlx_version, 34);

    let v2_execution_table: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'channel_monitor_executions'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("V2 execution table");
    assert_eq!(v2_execution_table, 1);
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
