use std::borrow::Cow;

use sqlx::{migrate::Migrator, Connection, Row, SqliteConnection};

static MIGRATOR: Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn schema_31_upgrades_request_logs_with_nullable_validated_http_status() {
    let mut connection = SqliteConnection::connect("sqlite::memory:")
        .await
        .expect("in-memory sqlite");
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut connection)
        .await
        .expect("foreign keys");

    migrator_through(31)
        .run(&mut connection)
        .await
        .expect("schema 31");
    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, status, created_at
         ) VALUES ('historical', 'historical', '1', 'POST', '/v1/responses',
                   'responses', 'success', '1')",
    )
    .execute(&mut connection)
    .await
    .expect("historical request log");

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
    let sqlx_version: i64 = sqlx::query_scalar("SELECT MAX(version) FROM _sqlx_migrations")
        .fetch_one(&mut connection)
        .await
        .expect("migration ledger");
    let current_schema = MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .expect("migration catalog is non-empty");
    assert_eq!(schema_version, current_schema);
    assert_eq!(sqlx_version, current_schema);

    let column = sqlx::query(
        "SELECT name, \"notnull\" FROM pragma_table_info('request_logs') WHERE name = 'http_status'",
    )
    .fetch_one(&mut connection)
    .await
    .expect("http_status column");
    assert_eq!(column.get::<String, _>("name"), "http_status");
    assert_eq!(column.get::<i64, _>("notnull"), 0);

    let historical_status: Option<i64> =
        sqlx::query_scalar("SELECT http_status FROM request_logs WHERE id = 'historical'")
            .fetch_one(&mut connection)
            .await
            .expect("historical status");
    assert_eq!(historical_status, None);

    sqlx::query("UPDATE request_logs SET http_status = 200 WHERE id = 'historical'")
        .execute(&mut connection)
        .await
        .expect("valid HTTP status");
    assert!(
        sqlx::query("UPDATE request_logs SET http_status = 99 WHERE id = 'historical'")
            .execute(&mut connection)
            .await
            .is_err(),
        "status below the HTTP range must be rejected"
    );
    assert!(
        sqlx::query("UPDATE request_logs SET http_status = 600 WHERE id = 'historical'")
            .execute(&mut connection)
            .await
            .is_err(),
        "status above the HTTP range must be rejected"
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
