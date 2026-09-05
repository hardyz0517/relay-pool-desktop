use std::borrow::Cow;

use sqlx::{migrate::Migrator, Connection, Row, SqliteConnection};

static MIGRATOR: Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn schema_71_upgrade_preserves_history_without_promoting_legacy_station_state() {
    let mut connection = open_schema(71).await;

    for (id, enabled, status) in [
        ("legacy-active", 1_i64, "healthy"),
        ("legacy-disabled", 0_i64, "failed"),
    ] {
        sqlx::query(
            "INSERT INTO stations (
                id, name, station_type, website_url, api_base_url,
                endpoint_revision, enabled, status, last_checked_at,
                last_pricing_fetched_at, created_at, updated_at
             ) VALUES (?1, ?1, 'sub2api', 'https://example.test',
                       'https://example.test/v1', 1, ?2, ?3,
                       'legacy-checked', 'legacy-pricing', '1', '1')",
        )
        .bind(id)
        .bind(enabled)
        .bind(status)
        .execute(&mut connection)
        .await
        .expect("legacy station");
        sqlx::query(
            "INSERT INTO domain_revisions (scope, revision, updated_at_ms, provenance)
             VALUES ('station_account:' || ?1, 4, 4, 'transactional_write')",
        )
        .bind(id)
        .execute(&mut connection)
        .await
        .expect("legacy credential revision");
    }

    sqlx::query(
        "INSERT INTO collector_runs (
            id, run_key, request_hash, station_id, endpoint_revision,
            adapter, task_type, status, started_at, finished_at,
            endpoint_count, success_count, failure_count, created_at
         ) VALUES ('legacy-run', 'legacy-run-key', 'legacy-hash',
                   'legacy-active', 1, 'sub2api', 'balance', 'success',
                   '10', '20', 1, 1, 0, '10')",
    )
    .execute(&mut connection)
    .await
    .expect("legacy collector run");
    sqlx::query(
        "INSERT INTO collector_task_state (
            station_id, task_type, last_run_id, last_status,
            last_success_at, consecutive_failures, updated_at
         ) VALUES ('legacy-active', 'balance', 'legacy-run', 'success',
                   '20', 0, '20')",
    )
    .execute(&mut connection)
    .await
    .expect("legacy collector task state");

    MIGRATOR
        .run(&mut connection)
        .await
        .expect("upgrade schema 71 to latest");

    assert_schema_version(&mut connection, 76).await;
    let stations: Vec<(String, String, Option<String>, Option<String>)> = sqlx::query_as(
        "SELECT id, status, last_checked_at, last_pricing_fetched_at
         FROM stations ORDER BY id",
    )
    .fetch_all(&mut connection)
    .await
    .expect("upgraded stations");
    assert_eq!(
        stations,
        vec![
            (
                "legacy-active".to_string(),
                "unchecked".to_string(),
                Some("legacy-checked".to_string()),
                Some("legacy-pricing".to_string()),
            ),
            (
                "legacy-disabled".to_string(),
                "disabled".to_string(),
                Some("legacy-checked".to_string()),
                Some("legacy-pricing".to_string()),
            ),
        ]
    );

    for table in [
        "station_collection_projection",
        "station_authorization_projection",
        "collector_operations",
    ] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT COUNT(*) FROM {table}"))
            .fetch_one(&mut connection)
            .await
            .expect("typed table count");
        assert_eq!(count, 0, "legacy state must not be promoted into {table}");
    }
    let retained_history: (i64, i64) = sqlx::query_as(
        "SELECT
            (SELECT COUNT(*) FROM collector_runs WHERE id = 'legacy-run'),
            (SELECT COUNT(*) FROM collector_task_state
             WHERE station_id = 'legacy-active' AND task_type = 'balance')",
    )
    .fetch_one(&mut connection)
    .await
    .expect("retained legacy history");
    assert_eq!(retained_history, (1, 1));
    let asset_revision: (i64, String) = sqlx::query_as(
        "SELECT revision, provenance FROM domain_revisions
         WHERE scope = 'read_model:station_assets'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("station assets baseline");
    assert_eq!(asset_revision, (1, "baseline_snapshot".to_string()));
    assert_foreign_keys_clean(&mut connection).await;
}

#[tokio::test]
async fn schema_73_failure_rolls_back_and_retry_is_idempotent() {
    let mut connection = open_schema(72).await;
    sqlx::query(
        "INSERT INTO stations (
            id, name, station_type, website_url, api_base_url,
            endpoint_revision, enabled, status, created_at, updated_at
         ) VALUES ('upgrade-station', 'Upgrade station', 'sub2api',
                   'https://example.test', 'https://example.test/v1',
                   3, 1, 'unchecked', '1', '1')",
    )
    .execute(&mut connection)
    .await
    .expect("schema 72 station");
    let legacy_error = "x".repeat(160);
    sqlx::query(
        "INSERT INTO post_authorization_collection_work (
            station_id, endpoint_revision, credential_revision, state,
            attempt_count, next_attempt_at_ms, last_error_code, operation_id,
            created_at_ms, updated_at_ms
         ) VALUES ('upgrade-station', 3, 7, 'failed', 9, 120,
                   ?1, 'legacy-operation', 100, 110)",
    )
    .bind(&legacy_error)
    .execute(&mut connection)
    .await
    .expect("schema 72 post-authorization work");

    // The v73 guard runs after all table rebuild statements. Making its
    // update ineligible forces the real migration to fail at its terminal
    // postcondition and proves the preceding DDL/data copy is transactional.
    sqlx::query(
        "UPDATE persistence_schema_compatibility
         SET schema_version = 99, updated_by_migration = 99
         WHERE singleton_key = 1",
    )
    .execute(&mut connection)
    .await
    .expect("install compatibility fault");
    let error = migrator_through(73)
        .run(&mut connection)
        .await
        .expect_err("schema 73 guard must reject an impossible source version");
    assert!(
        error.to_string().contains("CHECK constraint failed"),
        "unexpected migration failure: {error}"
    );

    assert_eq!(
        table_exists(&mut connection, "collector_operations").await,
        0
    );
    assert_eq!(
        table_exists(&mut connection, "post_authorization_collection_work_v72").await,
        0
    );
    assert_eq!(
        column_exists(
            &mut connection,
            "post_authorization_collection_work",
            "max_attempts",
        )
        .await,
        0
    );
    let rolled_back: (String, i64, String) = sqlx::query_as(
        "SELECT state, attempt_count, last_error_code
         FROM post_authorization_collection_work
         WHERE station_id = 'upgrade-station'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("rolled-back schema 72 row");
    assert_eq!(rolled_back, ("failed".to_string(), 9, legacy_error));
    let sqlx_version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut connection)
        .await
        .expect("rolled-back migration ledger");
    assert_eq!(sqlx_version, 72);

    sqlx::query(
        "UPDATE persistence_schema_compatibility
         SET schema_version = 72, updated_by_migration = 72
         WHERE singleton_key = 1",
    )
    .execute(&mut connection)
    .await
    .expect("repair compatibility fault");
    MIGRATOR
        .run(&mut connection)
        .await
        .expect("retry migrations through schema 76");

    assert_schema_version(&mut connection, 76).await;
    let migrated: (i64, i64, i64, String, i64, i64, String, String, i64, i64) = sqlx::query_as(
        "SELECT endpoint_revision, credential_revision, max_attempts, state,
                    attempt_count, next_attempt_at_ms, last_error_code,
                    operation_id, created_at_ms, updated_at_ms
             FROM post_authorization_collection_work
             WHERE station_id = 'upgrade-station'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("migrated post-authorization work");
    assert_eq!(migrated.0, 3);
    assert_eq!(migrated.1, 7);
    assert_eq!(migrated.2, 5);
    assert_eq!(migrated.3, "failed");
    assert_eq!(migrated.4, 9);
    assert_eq!(migrated.5, 120);
    assert_eq!(migrated.6, "x".repeat(128));
    assert_eq!(migrated.7, "legacy-operation");
    assert_eq!(migrated.8, 100);
    assert_eq!(migrated.9, 110);
    assert_eq!(
        table_exists(&mut connection, "collector_operations").await,
        1
    );
    assert_eq!(
        table_exists(&mut connection, "post_authorization_collection_work_v72").await,
        0
    );
    assert!(sqlx::query(
        "UPDATE post_authorization_collection_work SET max_attempts = 17
         WHERE station_id = 'upgrade-station'",
    )
    .execute(&mut connection)
    .await
    .is_err());
    assert_foreign_keys_clean(&mut connection).await;

    MIGRATOR
        .run(&mut connection)
        .await
        .expect("repeat latest migration run");
    let after_replay: (i64, String, String) = sqlx::query_as(
        "SELECT max_attempts, last_error_code, operation_id
         FROM post_authorization_collection_work
         WHERE station_id = 'upgrade-station'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("idempotent migrated row");
    assert_eq!(
        after_replay,
        (5, "x".repeat(128), "legacy-operation".to_string())
    );
    let migration_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM _sqlx_migrations WHERE version = 73")
            .fetch_one(&mut connection)
            .await
            .expect("schema 73 migration count");
    assert_eq!(migration_count, 1);
}

async fn open_schema(version: i64) -> SqliteConnection {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");
    migrator_through(version)
        .run(&mut connection)
        .await
        .expect("partial schema");
    connection
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

async fn assert_schema_version(connection: &mut SqliteConnection, expected: i64) {
    let row = sqlx::query(
        "SELECT
            (SELECT schema_version FROM persistence_schema_compatibility
             WHERE singleton_key = 1) AS compatibility_version,
            (SELECT MAX(version) FROM _sqlx_migrations) AS sqlx_version",
    )
    .fetch_one(&mut *connection)
    .await
    .expect("schema versions");
    assert_eq!(row.get::<i64, _>("compatibility_version"), expected);
    assert_eq!(row.get::<i64, _>("sqlx_version"), expected);
}

async fn table_exists(connection: &mut SqliteConnection, table: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?1")
        .bind(table)
        .fetch_one(connection)
        .await
        .expect("table existence")
}

async fn column_exists(connection: &mut SqliteConnection, table: &str, column: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM pragma_table_info(?1) WHERE name = ?2")
        .bind(table)
        .bind(column)
        .fetch_one(connection)
        .await
        .expect("column existence")
}

async fn assert_foreign_keys_clean(connection: &mut SqliteConnection) {
    let violations = sqlx::query("PRAGMA foreign_key_check")
        .fetch_all(connection)
        .await
        .expect("foreign key check");
    assert!(
        violations.is_empty(),
        "foreign key violation count: {}",
        violations.len()
    );
}
