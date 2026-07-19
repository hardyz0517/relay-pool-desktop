#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibilityDecisionCode {
    Writable,
    InspectionOnly,
    GenerationMismatch,
    ReaderTooOld,
    WriterTooOld,
    MetadataMismatch,
}

impl CompatibilityDecisionCode {
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
    #[error("commit outcome is unknown")]
    CommitOutcomeUnknown,
    #[error("I/O failure")]
    IoFailed { kind: std::io::ErrorKind },
    #[error("backup verification failed")]
    BackupVerificationFailed,
    #[error("SQLx failure")]
    Sqlx(#[from] sqlx::Error),
    #[error("migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl From<std::io::Error> for PersistenceError {
    fn from(error: std::io::Error) -> Self {
        Self::IoFailed { kind: error.kind() }
    }
}
