#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibilityDecisionCode {
    #[cfg(test)]
    Writable,
    #[cfg(test)]
    InspectionOnly,
    GenerationMismatch,
    ReaderTooOld,
    #[cfg(test)]
    WriterTooOld,
    MetadataMismatch,
}

impl CompatibilityDecisionCode {
    #[cfg(test)]
    #[allow(
        dead_code,
        reason = "the runtime compatibility code is asserted by its dedicated integration target"
    )]
    pub(crate) fn as_code(self) -> &'static str {
        match self {
            Self::Writable => "writable",
            Self::InspectionOnly => "inspection_only",
            Self::GenerationMismatch => "generation_mismatch",
            Self::ReaderTooOld => "reader_too_old",
            Self::WriterTooOld => "writer_too_old",
            Self::MetadataMismatch => "metadata_mismatch",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum PersistenceError {
    #[error("database file is missing")]
    MissingDatabase,
    #[error("schema incompatible")]
    IncompatibleSchema {
        writable: bool,
        code: CompatibilityDecisionCode,
    },
    #[error("schema metadata missing")]
    MissingCompatibilityMetadata,
    #[error("schema metadata invalid")]
    InvalidCompatibilityMetadata,
    #[error("migration metadata missing")]
    MissingMigrationMetadata,
    #[error("runtime is not accepting new persistence work")]
    RuntimeUnavailable,
    #[error("persistence session is already closed")]
    SessionClosed,
    #[error("persistence invariant violated")]
    InvariantViolation(String),
    #[error("record not found")]
    NotFound,
    #[error("constraint violation")]
    ConstraintViolation,
    #[error("stale endpoint revision")]
    #[allow(
        dead_code,
        reason = "constructed by station persistence; source-included runtime tests compile without that store"
    )]
    StaleRevision,
    #[error("commit outcome is unknown")]
    #[allow(
        dead_code,
        reason = "reserved for an indeterminate SQLite commit result at the persistence boundary"
    )]
    CommitOutcomeUnknown,
    #[error("I/O failure")]
    IoFailed { kind: std::io::ErrorKind },
    #[error("backup verification failed")]
    BackupVerificationFailed,
    #[error("database operation failed")]
    DatabaseFailed,
    #[error("migration failed")]
    MigrationFailed,
}

impl From<sqlx::Error> for PersistenceError {
    fn from(error: sqlx::Error) -> Self {
        match error {
            sqlx::Error::RowNotFound => Self::NotFound,
            sqlx::Error::Database(database)
                if database.is_unique_violation() || database.is_foreign_key_violation() =>
            {
                Self::ConstraintViolation
            }
            _ => Self::DatabaseFailed,
        }
    }
}

impl From<sqlx::migrate::MigrateError> for PersistenceError {
    fn from(_: sqlx::migrate::MigrateError) -> Self {
        Self::MigrationFailed
    }
}

impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::IoFailed { kind: error.kind() }
    }
}

#[cfg(test)]
mod tests {
    use super::PersistenceError;

    #[test]
    fn sqlx_row_not_found_is_normalized_inside_persistence() {
        assert!(matches!(
            PersistenceError::from(sqlx::Error::RowNotFound),
            PersistenceError::NotFound
        ));
    }
}
