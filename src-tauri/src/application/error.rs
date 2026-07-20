#[derive(Debug, thiserror::Error)]
pub(crate) enum ApplicationError {
    #[error("persistence unavailable")]
    Unavailable,
    #[error("installation already running")]
    InstallationAlreadyRunning,
    #[error("resource busy")]
    Busy,
    #[error("not found")]
    NotFound,
    #[error("conflict")]
    Conflict,
    #[error("stale revision")]
    StaleRevision,
    #[error("constraint violation")]
    ConstraintViolation,
    #[error("migration failed")]
    MigrationFailed,
    #[error("unsupported legacy schema")]
    UnsupportedLegacySchema,
    #[error("integrity failed")]
    IntegrityFailed,
    #[error("secret validation failed")]
    SecretValidationFailed,
    #[error("I/O failed")]
    IoFailed,
    #[error("cancelled")]
    Cancelled,
    #[error("schema incompatible")]
    IncompatibleSchema,
    #[error("commit outcome unknown")]
    CommitOutcomeUnknown,
    #[error("recovery precondition changed")]
    RecoveryPreconditionChanged,
    #[error("internal failure")]
    Internal,
}

impl From<crate::persistence::error::PersistenceError> for ApplicationError {
    fn from(error: crate::persistence::error::PersistenceError) -> Self {
        use crate::persistence::error::PersistenceError;

        match error {
            PersistenceError::RuntimeUnavailable => Self::Unavailable,
            PersistenceError::IncompatibleSchema { .. } => Self::IncompatibleSchema,
            PersistenceError::MissingDatabase => Self::Unavailable,
            PersistenceError::MissingCompatibilityMetadata
            | PersistenceError::InvalidCompatibilityMetadata
            | PersistenceError::MissingMigrationMetadata => Self::IncompatibleSchema,
            PersistenceError::Migration(_) => Self::MigrationFailed,
            PersistenceError::IoFailed { .. } => Self::IoFailed,
            PersistenceError::SessionClosed => Self::Internal,
            PersistenceError::InvariantViolation(_) => Self::Internal,
            PersistenceError::ConstraintViolation => Self::ConstraintViolation,
            PersistenceError::StaleRevision => Self::StaleRevision,
            PersistenceError::CommitOutcomeUnknown => Self::CommitOutcomeUnknown,
            PersistenceError::BackupVerificationFailed => Self::IntegrityFailed,
            PersistenceError::Sqlx(sqlx::Error::RowNotFound) => Self::NotFound,
            PersistenceError::Sqlx(sqlx::Error::Database(database))
                if database.is_unique_violation() || database.is_foreign_key_violation() =>
            {
                Self::ConstraintViolation
            }
            PersistenceError::Sqlx(_) => Self::Internal,
        }
    }
}
