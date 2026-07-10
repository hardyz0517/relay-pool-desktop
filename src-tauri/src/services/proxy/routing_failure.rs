#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteFailureKind {
    AuthError,
    InsufficientBalance,
    RateLimited,
    ModelUnavailable,
    CapabilityMismatch,
    BadRequest,
    TemporaryNetwork,
    Upstream5xx,
    Timeout,
    StreamInterrupted,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteFailureAction {
    HardFail,
    Cooldown,
    Observe,
    IgnoreForKeyHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RouteFailureScope {
    KeyHealth,
    RequestOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteFailureInput {
    pub http_status: Option<u16>,
    pub output_started: bool,
    pub transport_error: bool,
    pub timeout: bool,
    pub retry_after_ms: Option<i64>,
}

impl RouteFailureInput {
    pub fn timeout(output_started: bool) -> Self {
        Self {
            http_status: None,
            output_started,
            transport_error: true,
            timeout: true,
            retry_after_ms: None,
        }
    }

    pub fn http_status(status: u16, output_started: bool) -> Self {
        Self {
            http_status: Some(status),
            output_started,
            transport_error: false,
            timeout: false,
            retry_after_ms: None,
        }
    }

    pub fn http_status_with_retry_after(
        status: u16,
        output_started: bool,
        retry_after_ms: Option<i64>,
    ) -> Self {
        Self {
            http_status: Some(status),
            output_started,
            transport_error: false,
            timeout: false,
            retry_after_ms,
        }
    }

    pub fn transport_error(output_started: bool, error_summary: &str) -> Self {
        let lower = error_summary.to_ascii_lowercase();
        Self {
            http_status: None,
            output_started,
            transport_error: true,
            timeout: lower.contains("timeout")
                || lower.contains("timed out")
                || lower.contains("deadline"),
            retry_after_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedRouteFailure {
    pub kind: RouteFailureKind,
    pub action: RouteFailureAction,
    pub scope: RouteFailureScope,
    pub retryable_before_output: bool,
    pub retry_after_ms: Option<i64>,
}

impl ClassifiedRouteFailure {
    pub fn timeout_observe() -> Self {
        Self {
            kind: RouteFailureKind::Timeout,
            action: RouteFailureAction::Observe,
            scope: RouteFailureScope::KeyHealth,
            retryable_before_output: true,
            retry_after_ms: None,
        }
    }

    fn request_only(kind: RouteFailureKind) -> Self {
        Self {
            kind,
            action: RouteFailureAction::IgnoreForKeyHealth,
            scope: RouteFailureScope::RequestOnly,
            retryable_before_output: false,
            retry_after_ms: None,
        }
    }

    fn key_health(
        kind: RouteFailureKind,
        action: RouteFailureAction,
        retryable_before_output: bool,
        retry_after_ms: Option<i64>,
    ) -> Self {
        Self {
            kind,
            action,
            scope: RouteFailureScope::KeyHealth,
            retryable_before_output,
            retry_after_ms,
        }
    }
}

pub fn classify_route_failure(input: RouteFailureInput) -> ClassifiedRouteFailure {
    if input.output_started && input.transport_error {
        return ClassifiedRouteFailure::request_only(RouteFailureKind::StreamInterrupted);
    }

    match input.http_status {
        Some(401 | 403) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::AuthError,
            RouteFailureAction::HardFail,
            false,
            None,
        ),
        Some(402) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::InsufficientBalance,
            RouteFailureAction::HardFail,
            false,
            None,
        ),
        Some(429) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::RateLimited,
            RouteFailureAction::Cooldown,
            true,
            input.retry_after_ms,
        ),
        Some(404) => ClassifiedRouteFailure::request_only(RouteFailureKind::ModelUnavailable),
        Some(405 | 501) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::CapabilityMismatch,
            RouteFailureAction::Observe,
            true,
            None,
        ),
        Some(400) => ClassifiedRouteFailure::request_only(RouteFailureKind::BadRequest),
        Some(500..=599) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::Upstream5xx,
            RouteFailureAction::Observe,
            true,
            None,
        ),
        _ if input.timeout => ClassifiedRouteFailure::timeout_observe(),
        _ if input.transport_error => ClassifiedRouteFailure::key_health(
            RouteFailureKind::TemporaryNetwork,
            RouteFailureAction::Observe,
            !input.output_started,
            None,
        ),
        _ => ClassifiedRouteFailure::key_health(
            RouteFailureKind::Unknown,
            RouteFailureAction::Observe,
            !input.output_started,
            None,
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_treats_single_timeout_as_observe() {
        let failure = classify_route_failure(RouteFailureInput::timeout(false));

        assert_eq!(failure.kind, RouteFailureKind::Timeout);
        assert_eq!(failure.action, RouteFailureAction::Observe);
        assert!(failure.retryable_before_output);
    }

    #[test]
    fn classifier_ignores_client_bad_request_for_key_health() {
        let failure = classify_route_failure(RouteFailureInput::http_status(400, false));

        assert_eq!(failure.kind, RouteFailureKind::BadRequest);
        assert_eq!(failure.action, RouteFailureAction::IgnoreForKeyHealth);
        assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
    }

    #[test]
    fn classifier_treats_model_not_found_as_request_only() {
        let failure = classify_route_failure(RouteFailureInput::http_status(404, false));

        assert_eq!(failure.kind, RouteFailureKind::ModelUnavailable);
        assert_eq!(failure.action, RouteFailureAction::IgnoreForKeyHealth);
        assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
    }
}
