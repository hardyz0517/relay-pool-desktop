use std::fmt::{self, Display, Formatter};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PersistenceError {
    Unavailable(String),
    Draining,
    Busy,
    Locked,
    ConstraintViolation(String),
    StaleRevision,
    CommitOutcomeUnknown(String),
    InvariantViolation(String),
    Database(String),
}

impl PersistenceError {
    pub(crate) fn unavailable(message: impl Into<String>) -> Self {
        Self::Unavailable(message.into())
    }

    pub(crate) fn invariant(message: impl Into<String>) -> Self {
        Self::InvariantViolation(message.into())
    }
}

impl Display for PersistenceError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unavailable(message) => write!(f, "persistence unavailable: {message}"),
            Self::Draining => f.write_str("persistence is draining"),
            Self::Busy => f.write_str("database busy"),
            Self::Locked => f.write_str("database locked"),
            Self::ConstraintViolation(message) => {
                write!(f, "constraint violation: {message}")
            }
            Self::StaleRevision => f.write_str("stale endpoint revision"),
            Self::CommitOutcomeUnknown(message) => write!(f, "commit outcome unknown: {message}"),
            Self::InvariantViolation(message) => {
                write!(f, "persistence invariant violated: {message}")
            }
            Self::Database(message) => write!(f, "database operation failed: {message}"),
        }
    }
}

impl std::error::Error for PersistenceError {}

impl From<rusqlite::Error> for PersistenceError {
    fn from(error: rusqlite::Error) -> Self {
        use rusqlite::ErrorCode;

        match error {
            rusqlite::Error::SqliteFailure(failure, detail) => match failure.code {
                ErrorCode::DatabaseBusy => Self::Busy,
                ErrorCode::DatabaseLocked => Self::Locked,
                ErrorCode::ConstraintViolation => {
                    Self::ConstraintViolation(detail.unwrap_or_else(|| failure.to_string()))
                }
                _ => Self::Database(detail.unwrap_or_else(|| failure.to_string())),
            },
            rusqlite::Error::QueryReturnedNoRows => {
                Self::InvariantViolation("expected row was missing".to_string())
            }
            other => Self::Database(other.to_string()),
        }
    }
}
