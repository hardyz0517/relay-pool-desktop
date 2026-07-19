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
