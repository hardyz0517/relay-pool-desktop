use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    Row,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("src/persistence/migrations");

#[tokio::test]
async fn lifecycle_persistence_schema_enforces_attempt_identity_and_terminal_cas() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let path = tempdir.path().join("relay-pool.sqlite3");
    let options = SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .expect("pool");
    MIGRATOR.run(&pool).await.expect("migrations");

    let fk_target = sqlx::query("PRAGMA foreign_key_list(request_attempts)")
        .fetch_all(&pool)
        .await
        .expect("foreign key pragma");
    assert!(fk_target.iter().any(|row| {
        row.get::<String, _>(2) == "request_logs"
            && row.get::<String, _>(3) == "request_id"
            && row.get::<String, _>(4) == "id"
    }));

    sqlx::query(
        "INSERT INTO request_logs (
            id, request_id, started_at, method, path, endpoint, status,
            lifecycle_status, created_at
         ) VALUES (?, ?, '1', 'POST', '/v1/chat/completions',
                   '/v1/chat/completions', 'in_progress', 'admitted', '1')",
    )
    .bind("req-persist-schema")
    .bind("req-persist-schema")
    .execute(&pool)
    .await
    .expect("request start");

    let attempt_sql = "INSERT INTO request_attempts (
        request_id, ordinal, station_id, station_key_id, endpoint_revision,
        started_at_ms, terminal_kind, health_effect, output_committed, terminal_at_ms
    ) VALUES (?, 0, 'station-persist', 'key-persist', 1, 2, 'succeeded', 'success', 1, 3)";
    sqlx::query(attempt_sql)
        .bind("req-persist-schema")
        .execute(&pool)
        .await
        .expect("first attempt");
    let duplicate = sqlx::query(attempt_sql)
        .bind("req-persist-schema")
        .execute(&pool)
        .await
        .expect_err("duplicate attempt ordinal");
    assert!(duplicate.to_string().contains("UNIQUE"));

    let finalized = sqlx::query(
        "UPDATE request_logs SET status = 'success', lifecycle_status = 'completed',
            terminal_kind = 'completed', protocol_completed = 1,
            delivery_terminal = 'BodyCompleted', selected_attempt_ordinal = 0,
            attempt_count = 1, terminal_at_ms = 4
         WHERE request_id = ? AND terminal_at_ms IS NULL",
    )
    .bind("req-persist-schema")
    .execute(&pool)
    .await
    .expect("terminal update");
    assert_eq!(finalized.rows_affected(), 1);
    let duplicate_finalized = sqlx::query(
        "UPDATE request_logs SET status = 'success', lifecycle_status = 'completed',
            terminal_kind = 'completed', protocol_completed = 1,
            delivery_terminal = 'BodyCompleted', selected_attempt_ordinal = 0,
            attempt_count = 1, terminal_at_ms = 4
         WHERE request_id = ? AND terminal_at_ms IS NULL",
    )
    .bind("req-persist-schema")
    .execute(&pool)
    .await
    .expect("duplicate terminal update");
    assert_eq!(duplicate_finalized.rows_affected(), 0);

    let observed = sqlx::query(
        "SELECT status, lifecycle_status, terminal_kind, selected_attempt_ordinal
         FROM request_logs WHERE request_id = ?",
    )
    .bind("req-persist-schema")
    .fetch_one(&pool)
    .await
    .expect("request row");
    assert_eq!(observed.get::<String, _>(0), "success");
    assert_eq!(observed.get::<String, _>(1), "completed");
    assert_eq!(observed.get::<String, _>(2), "completed");
    assert_eq!(observed.get::<i64, _>(3), 0);
}
