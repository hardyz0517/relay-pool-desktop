use serde::{Deserialize, Serialize};

use crate::application::error::ApplicationError;
use crate::observability::correlation;

pub const MAX_MESSAGE_BYTES: usize = 512;
pub const MAX_FIELD_ERRORS: usize = 16;
pub const MAX_FIELD_BYTES: usize = 64;
pub const MAX_FIELD_MESSAGE_BYTES: usize = 256;
pub const MAX_RESOURCE_BYTES: usize = 128;
pub const MAX_REVISION_BYTES: usize = 128;
pub const MAX_PROVIDER_BYTES: usize = 64;
pub const MAX_CORRELATION_ID_BYTES: usize = 32;
pub const MAX_RETRY_AFTER_MS: u64 = 300_000;

pub const COMMAND_ERROR_TYPESCRIPT: &str = r#"export type CommandErrorCode =
  | "invalid_input"
  | "not_found"
  | "conflict"
  | "permission_denied"
  | "runtime_unavailable"
  | "data_store_unavailable"
  | "external_unavailable"
  | "timeout"
  | "overloaded"
  | "unsupported"
  | "internal";

export type PublicFieldError = {
  field: string;
  code: string;
  message: string;
};

export type PublicErrorDetails =
  | { kind: "validation"; fields: PublicFieldError[] }
  | { kind: "conflict"; resource: string; currentRevision: string | null }
  | { kind: "retry"; retryAfterMs: number | null }
  | { kind: "external"; provider: string | null; upstreamStatus: number | null };

export type CommandError = {
  code: CommandErrorCode;
  message: string;
  retryable: boolean;
  details: PublicErrorDetails | null;
  correlationId: string | null;
};"#;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: CommandErrorCode,
    pub message: String,
    pub retryable: bool,
    pub details: Option<PublicErrorDetails>,
    pub correlation_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandErrorCode {
    InvalidInput,
    NotFound,
    Conflict,
    PermissionDenied,
    RuntimeUnavailable,
    DataStoreUnavailable,
    ExternalUnavailable,
    Timeout,
    Overloaded,
    Unsupported,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PublicFieldError {
    pub field: String,
    pub code: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PublicErrorDetails {
    Validation {
        fields: Vec<PublicFieldError>,
    },
    Conflict {
        resource: String,
        current_revision: Option<String>,
    },
    Retry {
        retry_after_ms: Option<u64>,
    },
    External {
        provider: Option<String>,
        upstream_status: Option<u16>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandErrorInvariant {
    Oversized,
    Sensitive,
    InvalidDetails,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DriverFailure {
    Authentication,
    Unsupported,
    ExternalUnavailable {
        provider: Option<String>,
        upstream_status: Option<u16>,
    },
    Internal,
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OutboundFailure {
    Timeout {
        retry_after_ms: Option<u64>,
    },
    Overloaded {
        retry_after_ms: Option<u64>,
    },
    ExternalUnavailable {
        provider: Option<String>,
        upstream_status: Option<u16>,
    },
    Internal,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WorkFailure {
    Timeout,
    Overloaded,
    ResultUnknown,
    Internal,
}

impl CommandError {
    pub fn try_new(
        code: CommandErrorCode,
        message: impl Into<String>,
        retryable: bool,
        details: Option<PublicErrorDetails>,
        correlation_id: Option<String>,
    ) -> Result<Self, CommandErrorInvariant> {
        let message = message.into();
        validate_message(&message)?;
        if matches!(code, CommandErrorCode::Internal) && retryable {
            return Err(CommandErrorInvariant::InvalidDetails);
        }
        validate_details(details.as_ref(), retryable)?;
        let correlation_id = normalize_correlation_id(correlation_id)?;
        Ok(Self {
            code,
            message,
            retryable,
            details,
            correlation_id,
        })
    }

    pub fn internal(correlation_id: Option<String>) -> Self {
        let correlation_id = correlation_id.or_else(current_correlation_id);
        Self::try_new(
            CommandErrorCode::Internal,
            "The desktop operation failed.",
            false,
            None,
            correlation_id,
        )
        .expect("internal error envelope is a compile-time safe constant")
    }

    pub fn from_application(error: ApplicationError) -> Self {
        if matches!(
            &error,
            ApplicationError::IoFailed | ApplicationError::Internal
        ) {
            return Self::internal(None);
        }
        let (code, message, details) = match error {
            ApplicationError::Unavailable => (
                CommandErrorCode::RuntimeUnavailable,
                "The desktop runtime is unavailable.",
                None,
            ),
            ApplicationError::NotFound => (
                CommandErrorCode::NotFound,
                "The requested resource was not found.",
                None,
            ),
            ApplicationError::StaleRevision => (
                CommandErrorCode::Conflict,
                "The resource changed before this operation completed.",
                Some(PublicErrorDetails::Conflict {
                    resource: "resource".into(),
                    current_revision: None,
                }),
            ),
            ApplicationError::ConstraintViolation => (
                CommandErrorCode::Conflict,
                "The operation conflicts with existing data.",
                None,
            ),
            ApplicationError::MigrationFailed | ApplicationError::IncompatibleSchema => (
                CommandErrorCode::DataStoreUnavailable,
                "The local data store is unavailable.",
                None,
            ),
            ApplicationError::IntegrityFailed => (
                CommandErrorCode::DataStoreUnavailable,
                "The local data store failed an integrity check.",
                None,
            ),
            ApplicationError::SecretValidationFailed => (
                CommandErrorCode::InvalidInput,
                "The supplied credential could not be validated.",
                None,
            ),
            ApplicationError::IoFailed | ApplicationError::Internal => unreachable!(),
            ApplicationError::CommitOutcomeUnknown => (
                CommandErrorCode::Conflict,
                "The operation outcome could not be confirmed.",
                None,
            ),
        };
        Self::try_new(code, message, false, details, current_correlation_id())
            .expect("mapped application error must satisfy public envelope invariants")
    }

    pub(crate) fn from_driver(error: DriverFailure) -> Self {
        let (code, message, retryable, details) = match error {
            DriverFailure::Authentication => (
                CommandErrorCode::PermissionDenied,
                "The provider rejected the current credentials.",
                false,
                None,
            ),
            DriverFailure::Unsupported => (
                CommandErrorCode::Unsupported,
                "The provider does not support this operation.",
                false,
                None,
            ),
            DriverFailure::ExternalUnavailable {
                provider,
                upstream_status,
            } => (
                CommandErrorCode::ExternalUnavailable,
                "The external provider is unavailable.",
                true,
                Some(PublicErrorDetails::External {
                    provider,
                    upstream_status,
                }),
            ),
            DriverFailure::Internal => return Self::internal(None),
        };
        Self::try_new(code, message, retryable, details, current_correlation_id())
            .unwrap_or_else(|_| Self::internal(None))
    }

    pub(crate) fn from_outbound(error: OutboundFailure) -> Self {
        let (code, message, details) = match error {
            OutboundFailure::Timeout { retry_after_ms } => (
                CommandErrorCode::Timeout,
                "The external request timed out.",
                Some(PublicErrorDetails::Retry { retry_after_ms }),
            ),
            OutboundFailure::Overloaded { retry_after_ms } => (
                CommandErrorCode::Overloaded,
                "The external request capacity is full.",
                Some(PublicErrorDetails::Retry { retry_after_ms }),
            ),
            OutboundFailure::ExternalUnavailable {
                provider,
                upstream_status,
            } => (
                CommandErrorCode::ExternalUnavailable,
                "The external provider is unavailable.",
                Some(PublicErrorDetails::External {
                    provider,
                    upstream_status,
                }),
            ),
            OutboundFailure::Internal => return Self::internal(None),
        };
        Self::try_new(code, message, true, details, current_correlation_id())
            .unwrap_or_else(|_| Self::internal(None))
    }

    pub(crate) fn from_work(error: WorkFailure) -> Self {
        let (code, message, retryable) = match error {
            WorkFailure::Timeout => (CommandErrorCode::Timeout, "The operation timed out.", true),
            WorkFailure::Overloaded => (
                CommandErrorCode::Overloaded,
                "The operation capacity is full.",
                true,
            ),
            WorkFailure::ResultUnknown => (
                CommandErrorCode::Conflict,
                "The operation outcome could not be confirmed.",
                false,
            ),
            WorkFailure::Internal => return Self::internal(None),
        };
        Self::try_new(code, message, retryable, None, current_correlation_id())
            .expect("work failure mapping is a bounded public contract")
    }
}

impl From<String> for CommandError {
    fn from(_: String) -> Self {
        Self::internal(None)
    }
}

impl From<&str> for CommandError {
    fn from(_: &str) -> Self {
        Self::internal(None)
    }
}

fn validate_message(value: &str) -> Result<(), CommandErrorInvariant> {
    validate_text(value, MAX_MESSAGE_BYTES, true)
}

fn validate_text(
    value: &str,
    max_bytes: usize,
    reject_sensitive: bool,
) -> Result<(), CommandErrorInvariant> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(CommandErrorInvariant::Oversized);
    }
    if reject_sensitive && contains_sensitive_text(value) {
        return Err(CommandErrorInvariant::Sensitive);
    }
    Ok(())
}

fn contains_sensitive_text(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    if [
        "://",
        "?",
        "#",
        "cookie",
        "authorization",
        "bearer ",
        "token",
        "secret",
        "api_key",
        "apikey",
        ".sqlite",
        "\\",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return true;
    }

    value.split_ascii_whitespace().any(looks_like_absolute_path)
}

fn looks_like_absolute_path(value: &str) -> bool {
    let value = value.trim_matches(|character: char| {
        matches!(
            character,
            '(' | ')' | '[' | ']' | '{' | '}' | ',' | ';' | ':' | '"' | '\''
        )
    });
    value.starts_with('/')
        || value.starts_with("\\")
        || (value.len() >= 3
            && value.as_bytes()[0].is_ascii_alphabetic()
            && value.as_bytes()[1] == b':'
            && matches!(value.as_bytes()[2], b'/' | b'\\'))
}

fn validate_details(
    details: Option<&PublicErrorDetails>,
    retryable: bool,
) -> Result<(), CommandErrorInvariant> {
    match details {
        None => Ok(()),
        Some(PublicErrorDetails::Validation { fields }) => {
            if fields.is_empty() || fields.len() > MAX_FIELD_ERRORS {
                return Err(CommandErrorInvariant::Oversized);
            }
            for field in fields {
                validate_text(&field.field, MAX_FIELD_BYTES, true)?;
                validate_text(&field.code, MAX_FIELD_BYTES, true)?;
                validate_text(&field.message, MAX_FIELD_MESSAGE_BYTES, true)?;
            }
            Ok(())
        }
        Some(PublicErrorDetails::Conflict {
            resource,
            current_revision,
        }) => {
            validate_text(resource, MAX_RESOURCE_BYTES, true)?;
            if let Some(revision) = current_revision {
                validate_text(revision, MAX_REVISION_BYTES, true)?;
            }
            Ok(())
        }
        Some(PublicErrorDetails::Retry { retry_after_ms }) => {
            if !retryable {
                return Err(CommandErrorInvariant::InvalidDetails);
            }
            if retry_after_ms.is_some_and(|value| value > MAX_RETRY_AFTER_MS) {
                return Err(CommandErrorInvariant::Oversized);
            }
            Ok(())
        }
        Some(PublicErrorDetails::External {
            provider,
            upstream_status,
        }) => {
            if let Some(provider) = provider {
                validate_text(provider, MAX_PROVIDER_BYTES, true)?;
            }
            if upstream_status.is_some_and(|status| !(100..=599).contains(&status)) {
                return Err(CommandErrorInvariant::InvalidDetails);
            }
            Ok(())
        }
    }
}

fn normalize_correlation_id(
    value: Option<String>,
) -> Result<Option<String>, CommandErrorInvariant> {
    let Some(value) = value else { return Ok(None) };
    if value.is_empty()
        || value.len() > MAX_CORRELATION_ID_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(CommandErrorInvariant::Sensitive);
    }
    Ok(Some(value))
}

fn current_correlation_id() -> Option<String> {
    correlation::current().map(|value| value.as_str().to_string())
}

pub(crate) fn command_application_error(error: ApplicationError) -> CommandError {
    CommandError::from_application(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::error::PersistenceError;

    #[test]
    fn serializes_closed_error_envelope_with_stable_names() {
        let error = CommandError::try_new(
            CommandErrorCode::InvalidInput,
            "The input is invalid.",
            false,
            Some(PublicErrorDetails::Validation {
                fields: vec![PublicFieldError {
                    field: "name".into(),
                    code: "required".into(),
                    message: "Name is required.".into(),
                }],
            }),
            Some("corr_123".into()),
        )
        .expect("valid envelope");
        let value = serde_json::to_value(error).expect("serialize");
        assert_eq!(value["code"], "invalid_input");
        assert_eq!(value["correlationId"], "corr_123");
        assert_eq!(value["details"]["kind"], "validation");
    }

    #[test]
    fn rejects_secret_url_path_and_unbounded_input() {
        let oversized = "x".repeat(MAX_MESSAGE_BYTES + 1);
        for message in [
            "request https://example.test/v1?token=secret",
            "cookie=abc",
            "failed at /home/user/private.db",
            "failed at C:/Users/private/data.db",
            oversized.as_str(),
        ] {
            assert!(
                CommandError::try_new(CommandErrorCode::Internal, message, false, None, None,)
                    .is_err()
            );
        }
    }

    #[test]
    fn retry_details_require_retryable_and_have_a_bounded_delay() {
        let detail = Some(PublicErrorDetails::Retry {
            retry_after_ms: Some(MAX_RETRY_AFTER_MS + 1),
        });
        assert!(
            CommandError::try_new(CommandErrorCode::Timeout, "Timed out.", true, detail, None)
                .is_err()
        );
        assert!(CommandError::try_new(
            CommandErrorCode::Timeout,
            "Timed out.",
            false,
            Some(PublicErrorDetails::Retry {
                retry_after_ms: Some(10),
            }),
            None,
        )
        .is_err());
    }

    #[test]
    fn internal_application_errors_are_not_retryable_or_verbose() {
        let error =
            command_application_error(ApplicationError::from(PersistenceError::DatabaseFailed));
        assert_eq!(error.code, CommandErrorCode::Internal);
        assert!(!error.retryable);
        assert!(!error.message.contains("DatabaseFailed"));
    }

    #[test]
    fn internal_errors_cannot_be_marked_retryable() {
        assert!(CommandError::try_new(
            CommandErrorCode::Internal,
            "The desktop operation failed.",
            true,
            None,
            None,
        )
        .is_err());
    }

    #[tokio::test]
    async fn mapped_errors_preserve_the_current_command_correlation() {
        let error = correlation::in_command_scope("fixture_command", async {
            command_application_error(ApplicationError::NotFound)
        })
        .await;

        assert_eq!(
            error.correlation_id.as_deref().map(str::len),
            Some(MAX_CORRELATION_ID_BYTES)
        );
    }

    #[test]
    fn application_driver_outbound_and_work_failures_map_one_way() {
        assert_eq!(
            CommandError::from_application(ApplicationError::NotFound).code,
            CommandErrorCode::NotFound
        );
        let driver = CommandError::from_driver(DriverFailure::Authentication);
        assert_eq!(driver.code, CommandErrorCode::PermissionDenied);
        assert!(!driver.retryable);
        let outbound = CommandError::from_outbound(OutboundFailure::Timeout {
            retry_after_ms: Some(250),
        });
        assert_eq!(outbound.code, CommandErrorCode::Timeout);
        assert!(outbound.retryable);
        assert!(matches!(
            outbound.details,
            Some(PublicErrorDetails::Retry {
                retry_after_ms: Some(250)
            })
        ));
        let work = CommandError::from_work(WorkFailure::ResultUnknown);
        assert_eq!(work.code, CommandErrorCode::Conflict);
        assert!(!work.retryable);
    }

    #[test]
    fn unsafe_temporary_mapping_details_fail_closed() {
        let error = CommandError::from_driver(DriverFailure::ExternalUnavailable {
            provider: Some("https://provider.test/?token=secret".into()),
            upstream_status: Some(200),
        });
        assert_eq!(error.code, CommandErrorCode::Internal);
        assert!(!error.retryable);
        assert!(error.details.is_none());
    }
}
