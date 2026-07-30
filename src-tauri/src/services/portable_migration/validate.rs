use std::path::Path;

use sqlx::{sqlite::SqliteConnectOptions, ConnectOptions, Connection, Row, SqliteConnection};

use super::{catalog::CatalogError, transform::TransformError};

#[derive(Debug, thiserror::Error)]
pub(crate) enum PortableMigrationValidationError {
    #[error("portable migration SQLite database could not be opened")]
    OpenFailed,
    #[error("portable migration SQLite quick_check failed")]
    QuickCheckFailed,
    #[error("portable migration SQLite foreign key check failed")]
    ForeignKeyCheckFailed,
    #[error("portable migration schema is unsupported")]
    UnsupportedSchema,
    #[error("portable migration schema object is unsupported")]
    UnsupportedSchemaObject,
    #[error("portable migration schema catalog drift was detected")]
    CatalogDrift(#[from] CatalogError),
    #[error("portable migration row transform failed")]
    Transform(#[from] TransformError),
    #[error("portable migration SQL operation failed")]
    Sql,
    #[error("portable migration atomic publish failed")]
    AtomicPublish,
    #[error("portable migration SQLite sidecar is not empty in closed state")]
    SidecarNotEmpty,
}

impl From<sqlx::Error> for PortableMigrationValidationError {
    fn from(_: sqlx::Error) -> Self {
        Self::Sql
    }
}

pub(crate) type PortableValidationResult<T> = Result<T, PortableMigrationValidationError>;

pub(crate) async fn open_read_only_sqlite(
    path: &Path,
) -> PortableValidationResult<SqliteConnection> {
    if !path.is_file() {
        return Err(PortableMigrationValidationError::OpenFailed);
    }
    let options = SqliteConnectOptions::new()
        .filename(path)
        .read_only(true)
        .create_if_missing(false)
        .foreign_keys(true)
        .disable_statement_logging();
    let mut connection = SqliteConnection::connect_with(&options)
        .await
        .map_err(|_| PortableMigrationValidationError::OpenFailed)?;
    configure_untrusted_reader(&mut connection).await?;
    Ok(connection)
}

pub(crate) async fn configure_untrusted_reader(
    connection: &mut SqliteConnection,
) -> PortableValidationResult<()> {
    sqlx::query("PRAGMA query_only = ON")
        .execute(&mut *connection)
        .await?;
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&mut *connection)
        .await?;
    sqlx::query("PRAGMA trusted_schema = OFF")
        .execute(&mut *connection)
        .await?;
    Ok(())
}

pub(crate) async fn validate_quick_check(
    connection: &mut SqliteConnection,
) -> PortableValidationResult<()> {
    let row = sqlx::query("PRAGMA quick_check")
        .fetch_one(&mut *connection)
        .await?;
    let status: String = row.get(0);
    if status.eq_ignore_ascii_case("ok") {
        Ok(())
    } else {
        Err(PortableMigrationValidationError::QuickCheckFailed)
    }
}

pub(crate) async fn validate_foreign_keys(
    connection: &mut SqliteConnection,
) -> PortableValidationResult<()> {
    let row = sqlx::query("PRAGMA foreign_key_check")
        .fetch_optional(&mut *connection)
        .await?;
    if row.is_none() {
        Ok(())
    } else {
        Err(PortableMigrationValidationError::ForeignKeyCheckFailed)
    }
}

pub(crate) async fn validate_closed_sqlite_database(path: &Path) -> PortableValidationResult<()> {
    let mut connection = open_read_only_sqlite(path).await?;
    validate_quick_check(&mut connection).await?;
    validate_foreign_keys(&mut connection).await?;
    connection.close().await?;
    Ok(())
}

pub(crate) fn quote_identifier(identifier: &str) -> PortableValidationResult<String> {
    if identifier.is_empty() || identifier.bytes().any(|byte| byte == 0) {
        return Err(PortableMigrationValidationError::UnsupportedSchemaObject);
    }
    Ok(format!("\"{}\"", identifier.replace('"', "\"\"")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn closed_database_validation_rejects_missing_file() {
        let directory = tempfile::tempdir().expect("tempdir");
        let missing = directory.path().join("missing.sqlite");

        let error = validate_closed_sqlite_database(&missing)
            .await
            .expect_err("missing sqlite must fail");

        assert!(matches!(
            error,
            PortableMigrationValidationError::OpenFailed
        ));
    }
}
