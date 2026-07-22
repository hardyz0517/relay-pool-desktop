#[derive(Debug, thiserror::Error)]
pub(crate) enum ApplicationError {
    #[error("persistence unavailable")]
    Unavailable,
    #[error("not found")]
    NotFound,
    #[error("stale revision")]
    StaleRevision,
    #[error("constraint violation")]
    ConstraintViolation,
    #[error("migration failed")]
    MigrationFailed,
    #[error("integrity failed")]
    IntegrityFailed,
    #[error("secret validation failed")]
    SecretValidationFailed,
    #[error("I/O failed")]
    IoFailed,
    #[error("schema incompatible")]
    IncompatibleSchema,
    #[error("commit outcome unknown")]
    CommitOutcomeUnknown,
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
            PersistenceError::MigrationFailed => Self::MigrationFailed,
            PersistenceError::IoFailed { .. } => Self::IoFailed,
            PersistenceError::SessionClosed => Self::Internal,
            PersistenceError::InvariantViolation(_) => Self::Internal,
            PersistenceError::NotFound => Self::NotFound,
            PersistenceError::ConstraintViolation => Self::ConstraintViolation,
            PersistenceError::StaleRevision => Self::StaleRevision,
            PersistenceError::CommitOutcomeUnknown => Self::CommitOutcomeUnknown,
            PersistenceError::BackupVerificationFailed => Self::IntegrityFailed,
            PersistenceError::DatabaseFailed => Self::Internal,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ApplicationError;
    use crate::persistence::error::PersistenceError;

    #[test]
    fn persistence_errors_map_to_stable_application_categories() {
        assert!(matches!(
            ApplicationError::from(PersistenceError::NotFound),
            ApplicationError::NotFound
        ));
        assert!(matches!(
            ApplicationError::from(PersistenceError::ConstraintViolation),
            ApplicationError::ConstraintViolation
        ));
        assert!(matches!(
            ApplicationError::from(PersistenceError::DatabaseFailed),
            ApplicationError::Internal
        ));
        assert!(matches!(
            ApplicationError::from(PersistenceError::MigrationFailed),
            ApplicationError::MigrationFailed
        ));
    }
}
