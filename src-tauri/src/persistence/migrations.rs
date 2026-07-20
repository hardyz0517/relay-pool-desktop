use rusqlite::Connection;

use super::error::PersistenceError;

pub(crate) fn apply_migrations(connection: &Connection) -> Result<(), PersistenceError> {
    apply_request_lifecycle_migration(connection)?;
    Ok(())
}

fn apply_request_lifecycle_migration(connection: &Connection) -> Result<(), PersistenceError> {
    let mut statement = connection.prepare("PRAGMA table_info(request_logs)")?;
    let columns = statement
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;

    for statement in request_log_add_column_statements(&columns) {
        connection.execute(statement, [])?;
    }

    connection.execute_batch(include_str!("migrations/0005_request_logs.sql"))?;
    rebuild_request_attempts_table_if_needed(connection)?;
    Ok(())
}

fn request_log_add_column_statements(existing_columns: &[String]) -> Vec<&'static str> {
    let mut statements = Vec::new();
    let has = |name: &str| existing_columns.iter().any(|column| column == name);
    if !has("terminal_kind") {
        statements.push("ALTER TABLE request_logs ADD COLUMN terminal_kind TEXT");
    }
    if !has("terminal_code") {
        statements.push("ALTER TABLE request_logs ADD COLUMN terminal_code TEXT");
    }
    if !has("terminal_detail") {
        statements.push("ALTER TABLE request_logs ADD COLUMN terminal_detail TEXT");
    }
    if !has("protocol_completed") {
        statements.push("ALTER TABLE request_logs ADD COLUMN protocol_completed INTEGER");
    }
    if !has("delivery_terminal") {
        statements.push("ALTER TABLE request_logs ADD COLUMN delivery_terminal TEXT");
    }
    if !has("selected_attempt_ordinal") {
        statements.push("ALTER TABLE request_logs ADD COLUMN selected_attempt_ordinal INTEGER");
    }
    if !has("terminal_at_ms") {
        statements.push("ALTER TABLE request_logs ADD COLUMN terminal_at_ms INTEGER");
    }
    if !has("endpoint") {
        statements.push("ALTER TABLE request_logs ADD COLUMN endpoint TEXT");
    }
    statements
}

fn rebuild_request_attempts_table_if_needed(
    connection: &Connection,
) -> Result<(), PersistenceError> {
    let table_exists: bool = connection.query_row(
        "SELECT EXISTS(
            SELECT 1 FROM sqlite_master
             WHERE type = 'table' AND name = 'request_attempts'
        )",
        [],
        |row| row.get(0),
    )?;
    if !table_exists || request_attempts_references_request_log_id(connection)? {
        return Ok(());
    }

    connection.execute_batch(
        r#"
        PRAGMA foreign_keys = OFF;

        CREATE TABLE request_attempts_v2_rebuild (
            request_id TEXT NOT NULL,
            ordinal INTEGER NOT NULL,
            station_id TEXT NOT NULL,
            station_key_id TEXT NOT NULL,
            endpoint_revision INTEGER NOT NULL,
            started_at_ms INTEGER NOT NULL,
            terminal_kind TEXT NOT NULL,
            failure_kind TEXT,
            failure_blame TEXT,
            retry_disposition TEXT,
            health_effect TEXT NOT NULL,
            health_cooldown_until_ms INTEGER,
            public_code TEXT,
            sanitized_detail TEXT,
            output_committed INTEGER NOT NULL,
            terminal_at_ms INTEGER NOT NULL,
            PRIMARY KEY (request_id, ordinal),
            FOREIGN KEY(request_id) REFERENCES request_logs(id) ON DELETE CASCADE
        );

        INSERT OR IGNORE INTO request_attempts_v2_rebuild (
            request_id, ordinal, station_id, station_key_id, endpoint_revision,
            started_at_ms, terminal_kind, failure_kind, failure_blame,
            retry_disposition, health_effect, health_cooldown_until_ms,
            public_code, sanitized_detail, output_committed, terminal_at_ms
        )
        SELECT
            request_id, ordinal, station_id, station_key_id, endpoint_revision,
            started_at_ms, terminal_kind, failure_kind, failure_blame,
            retry_disposition, health_effect, health_cooldown_until_ms,
            public_code, sanitized_detail, output_committed, terminal_at_ms
        FROM request_attempts;

        DROP TABLE request_attempts;
        ALTER TABLE request_attempts_v2_rebuild RENAME TO request_attempts;

        CREATE INDEX IF NOT EXISTS idx_request_attempts_station_key_terminal
            ON request_attempts(station_key_id, terminal_at_ms DESC);

        PRAGMA foreign_keys = ON;
        "#,
    )?;
    Ok(())
}

fn request_attempts_references_request_log_id(
    connection: &Connection,
) -> Result<bool, PersistenceError> {
    let mut statement = connection.prepare("PRAGMA foreign_key_list(request_attempts)")?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .iter()
        .any(|(table, from, to)| table == "request_logs" && from == "request_id" && to == "id"))
}
