use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use rusqlite::{params, Connection};

mod persistence {
    #[path = "../../src/persistence/error.rs"]
    pub(crate) mod error;
    #[path = "../../src/persistence/migrations.rs"]
    pub(crate) mod migrations;
}

#[test]
fn lifecycle_persistence_schema_enforces_attempt_identity_and_terminal_cas() {
    let db_path = temp_db_path("schema-contract");
    let connection = Connection::open(&db_path).expect("open sqlite");
    connection
        .pragma_update(None, "foreign_keys", "ON")
        .expect("foreign keys");
    create_request_log_base_schema(&connection);

    persistence::migrations::apply_migrations(&connection).expect("migrations");

    let fk_target = connection
        .prepare("PRAGMA foreign_key_list(request_attempts)")
        .expect("foreign key pragma")
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .expect("foreign key rows")
        .collect::<Result<Vec<_>, _>>()
        .expect("foreign key collect");
    assert!(
        fk_target
            .iter()
            .any(|(table, from, to)| table == "request_logs" && from == "request_id" && to == "id"),
        "request_attempts must reference canonical request log id: {fk_target:?}"
    );

    insert_request_start(&connection, "req-persist-schema");
    insert_attempt_terminal(&connection, "req-persist-schema", 0).expect("first attempt insert");
    let duplicate = insert_attempt_terminal(&connection, "req-persist-schema", 0)
        .expect_err("duplicate attempt ordinal must be rejected by PK");
    assert!(
        duplicate.to_string().contains("UNIQUE")
            || duplicate.to_string().contains("constraint failed"),
        "{duplicate}"
    );
    let missing_request =
        insert_attempt_terminal(&connection, "missing-request", 0).expect_err("missing request FK");
    assert!(
        missing_request.to_string().contains("FOREIGN KEY"),
        "{missing_request}"
    );

    let finalized = finish_request_once(&connection, "req-persist-schema", "completed");
    assert_eq!(finalized, 1);
    let duplicate_finalized = finish_request_once(&connection, "req-persist-schema", "completed");
    assert_eq!(
        duplicate_finalized, 0,
        "terminal CAS must not finalize a request twice"
    );

    let observed = connection
        .query_row(
            "SELECT status, lifecycle_status, terminal_kind, selected_attempt_ordinal
             FROM request_logs WHERE request_id = ?1",
            ["req-persist-schema"],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            },
        )
        .expect("request row");
    assert_eq!(
        observed,
        (
            "success".to_string(),
            "completed".to_string(),
            Some("Completed".to_string()),
            Some(0)
        )
    );
}

fn create_request_log_base_schema(connection: &Connection) {
    connection
        .execute_batch(
            r#"
            CREATE TABLE request_logs (
                id TEXT PRIMARY KEY,
                request_id TEXT,
                started_at TEXT NOT NULL,
                finished_at TEXT,
                duration_ms INTEGER,
                method TEXT NOT NULL,
                path TEXT NOT NULL,
                model TEXT,
                stream INTEGER NOT NULL DEFAULT 0,
                status TEXT NOT NULL,
                lifecycle_status TEXT,
                station_key_id TEXT,
                station_id TEXT,
                upstream_base_url TEXT,
                fallback_count INTEGER NOT NULL DEFAULT 0,
                error_message TEXT,
                route_policy TEXT,
                route_reason TEXT,
                rejected_candidates_json TEXT,
                body_bytes INTEGER,
                attempt_count INTEGER,
                route_wait_ms INTEGER,
                upstream_headers_ms INTEGER,
                failure_source TEXT,
                attempts_json TEXT,
                completion_source TEXT,
                prompt_tokens INTEGER,
                completion_tokens INTEGER,
                total_tokens INTEGER,
                cache_creation_tokens INTEGER,
                cache_read_tokens INTEGER,
                reasoning_effort TEXT,
                first_token_ms INTEGER,
                billing_mode TEXT,
                estimated_input_cost REAL,
                estimated_output_cost REAL,
                estimated_total_cost REAL,
                base_input_cost REAL,
                base_output_cost REAL,
                base_fixed_cost REAL,
                base_total_cost REAL,
                cost_currency TEXT,
                pricing_rule_id TEXT,
                pricing_source TEXT,
                cost_status TEXT,
                group_binding_id TEXT,
                normalization_status TEXT,
                balance_scope TEXT,
                economic_context_json TEXT,
                created_at TEXT NOT NULL
            );
            CREATE UNIQUE INDEX idx_request_logs_request_id_unique
                ON request_logs(request_id)
                WHERE request_id IS NOT NULL;
            "#,
        )
        .expect("base request log schema");
}

fn insert_request_start(connection: &Connection, request_id: &str) {
    connection
        .execute(
            "INSERT INTO request_logs (
                id, request_id, started_at, method, path, model, stream, status,
                lifecycle_status, endpoint, fallback_count, created_at
             ) VALUES (?1, ?1, '1', 'POST', '/v1/chat/completions', NULL, 0,
                       'in_progress', 'admitted', '/v1/chat/completions', 0, '1')",
            [request_id],
        )
        .expect("request start");
}

fn insert_attempt_terminal(
    connection: &Connection,
    request_id: &str,
    ordinal: i64,
) -> rusqlite::Result<usize> {
    connection.execute(
        "INSERT INTO request_attempts (
            request_id, ordinal, station_id, station_key_id, endpoint_revision,
            started_at_ms, terminal_kind, failure_kind, failure_blame,
            retry_disposition, health_effect, health_cooldown_until_ms,
            public_code, sanitized_detail, output_committed, terminal_at_ms
         ) VALUES (?1, ?2, 'station-persist', 'key-persist', 1, 2, 'succeeded',
                   NULL, NULL, NULL, 'success', NULL, NULL, NULL, 1, 3)",
        params![request_id, ordinal],
    )
}

fn finish_request_once(connection: &Connection, request_id: &str, lifecycle_status: &str) -> usize {
    connection
        .execute(
            "UPDATE request_logs SET
                finished_at = '4',
                duration_ms = 3,
                status = 'success',
                lifecycle_status = ?2,
                terminal_kind = 'Completed',
                terminal_code = NULL,
                terminal_detail = NULL,
                protocol_completed = 1,
                delivery_terminal = 'BodyCompleted',
                selected_attempt_ordinal = 0,
                attempt_count = 1,
                fallback_count = 0,
                terminal_at_ms = 4
             WHERE request_id = ?1 AND terminal_at_ms IS NULL",
            params![request_id, lifecycle_status],
        )
        .expect("finish request")
}

fn temp_db_path(name: &str) -> PathBuf {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    let root = std::env::temp_dir().join(format!(
        "relay-pool-task18-persistence-{name}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).expect("temp db dir");
    root.join("relay-pool.sqlite")
}
