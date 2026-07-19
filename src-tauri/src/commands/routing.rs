use serde::Serialize;

use crate::application::error::ApplicationError;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RoutingCommandError {
    pub(crate) code: &'static str,
    pub(crate) message: &'static str,
}

pub(crate) fn routing_command_error(error: ApplicationError) -> RoutingCommandError {
    match error {
        ApplicationError::NotFound => RoutingCommandError {
            code: "not_found",
            message: "not found",
        },
        ApplicationError::ConstraintViolation | ApplicationError::Conflict => RoutingCommandError {
            code: "conflict",
            message: "conflict",
        },
        ApplicationError::Busy => RoutingCommandError {
            code: "busy",
            message: "resource busy",
        },
        ApplicationError::Unavailable => RoutingCommandError {
            code: "unavailable",
            message: "persistence unavailable",
        },
        _ => RoutingCommandError {
            code: "internal",
            message: "internal failure",
        },
    }
}
