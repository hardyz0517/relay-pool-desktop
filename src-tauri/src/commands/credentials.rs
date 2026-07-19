use serde::Serialize;

use crate::application::error::ApplicationError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CredentialCommandError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) fn credential_command_error(error: ApplicationError) -> CredentialCommandError {
    match error {
        ApplicationError::NotFound => CredentialCommandError {
            code: "not_found",
            message: "not found",
        },
        ApplicationError::ConstraintViolation | ApplicationError::Conflict => {
            CredentialCommandError {
                code: "conflict",
                message: "conflict",
            }
        }
        ApplicationError::SecretValidationFailed => CredentialCommandError {
            code: "secret_validation_failed",
            message: "secret validation failed",
        },
        ApplicationError::Busy => CredentialCommandError {
            code: "busy",
            message: "resource busy",
        },
        ApplicationError::Unavailable => CredentialCommandError {
            code: "unavailable",
            message: "persistence unavailable",
        },
        _ => CredentialCommandError {
            code: "internal",
            message: "internal failure",
        },
    }
}
