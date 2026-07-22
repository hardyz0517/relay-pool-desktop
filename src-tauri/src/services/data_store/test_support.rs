use std::path::Path;

use sqlx::{
    sqlite::{SqliteConnectOptions, SqliteJournalMode},
    ConnectOptions, Connection, SqliteConnection,
};

pub(crate) fn open_database(path: &Path) -> SqliteConnection {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("database parent");
    }
    tauri::async_runtime::block_on(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .connect(),
    )
    .expect("open test database")
}

pub(crate) fn execute_batch(connection: &mut SqliteConnection, statements: &str) {
    tauri::async_runtime::block_on(sqlx::raw_sql(statements).execute(connection))
        .expect("execute test fixture");
}

pub(crate) fn execute_with_text(connection: &mut SqliteConnection, statement: &str, value: &str) {
    tauri::async_runtime::block_on(sqlx::query(statement).bind(value).execute(connection))
        .expect("execute test fixture with text");
}

pub(crate) fn query_i64(path: &Path, statement: &str) -> i64 {
    let mut connection = tauri::async_runtime::block_on(
        SqliteConnectOptions::new()
            .filename(path)
            .read_only(true)
            .create_if_missing(false)
            .connect(),
    )
    .expect("open read-only test database");
    tauri::async_runtime::block_on(async move {
        let value = sqlx::query_scalar(statement)
            .fetch_one(&mut connection)
            .await?;
        connection.close().await?;
        Ok::<i64, sqlx::Error>(value)
    })
    .expect("query test fixture")
}

pub(crate) fn close_database(connection: SqliteConnection) {
    tauri::async_runtime::block_on(connection.close()).expect("close test database");
}

pub(crate) fn open_existing_database(path: &Path) -> SqliteConnection {
    tauri::async_runtime::block_on(
        SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(false)
            .connect(),
    )
    .expect("open existing test database")
}
