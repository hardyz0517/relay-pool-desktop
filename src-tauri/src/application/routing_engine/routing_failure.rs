#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RouteFailureKind {
    AuthError,
    InsufficientBalance,
    RateLimited,
    CapabilityMismatch,
    BadRequest,
    TemporaryNetwork,
    Upstream5xx,
    Timeout,
    StreamInterrupted,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RouteFailureAction {
    HardFail,
    Cooldown,
    Observe,
    IgnoreForKeyHealth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RouteFailureScope {
    KeyHealth,
    RequestOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RouteFailureInput {
    pub(crate) http_status: Option<u16>,
    pub(crate) output_started: bool,
    pub(crate) transport_error: bool,
    pub(crate) timeout: bool,
    pub(crate) retry_after_ms: Option<i64>,
}

impl RouteFailureInput {
    #[cfg(test)]
    pub(crate) fn timeout(output_started: bool) -> Self {
        Self {
            http_status: None,
            output_started,
            transport_error: true,
            timeout: true,
            retry_after_ms: None,
        }
    }

    pub(crate) fn http_status(status: u16, output_started: bool) -> Self {
        Self {
            http_status: Some(status),
            output_started,
            transport_error: false,
            timeout: false,
            retry_after_ms: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassifiedRouteFailure {
    pub(crate) kind: RouteFailureKind,
    pub(crate) action: RouteFailureAction,
    pub(crate) scope: RouteFailureScope,
    pub(crate) retryable_before_output: bool,
    pub(crate) retry_after_ms: Option<i64>,
}

impl ClassifiedRouteFailure {
    pub(crate) fn timeout_observe() -> Self {
        Self {
            kind: RouteFailureKind::Timeout,
            action: RouteFailureAction::Observe,
            scope: RouteFailureScope::KeyHealth,
            retryable_before_output: true,
            retry_after_ms: None,
        }
    }

    fn request_only(kind: RouteFailureKind, retryable_before_output: bool) -> Self {
        Self {
            kind,
            action: RouteFailureAction::IgnoreForKeyHealth,
            scope: RouteFailureScope::RequestOnly,
            retryable_before_output,
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

pub(crate) fn classify_route_failure(input: RouteFailureInput) -> ClassifiedRouteFailure {
    if input.output_started && input.transport_error {
        return ClassifiedRouteFailure::request_only(RouteFailureKind::StreamInterrupted, false);
    }

    match input.http_status {
        Some(401) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::AuthError,
            RouteFailureAction::HardFail,
            !input.output_started,
            None,
        ),
        Some(408 | 425) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::TemporaryNetwork,
            RouteFailureAction::Observe,
            !input.output_started,
            input.retry_after_ms,
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
        Some(403 | 404) => ClassifiedRouteFailure::request_only(RouteFailureKind::Uncertain, false),
        Some(405 | 501) => ClassifiedRouteFailure::key_health(
            RouteFailureKind::CapabilityMismatch,
            RouteFailureAction::Observe,
            true,
            None,
        ),
        Some(400 | 409 | 422) => {
            ClassifiedRouteFailure::request_only(RouteFailureKind::BadRequest, false)
        }
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
            RouteFailureKind::Uncertain,
            RouteFailureAction::IgnoreForKeyHealth,
            !input.output_started,
            None,
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RoutePlanningFailure {
    HealthUnavailable,
    CapacityExhausted,
    #[cfg(test)]
    CandidateLimitExceeded { actual: usize, limit: usize },
    ConfigUnstable,
    DeadlineExceeded,
    InvariantViolation { code: &'static str },
}

impl RoutePlanningFailure {
    pub(crate) fn into_canonical(
        self,
    ) -> crate::application::request_finalization::failure::CanonicalFailure {
        use crate::application::request_finalization::failure::{
            planning_failure, FailureClass, FailureTarget, LocalAdapterComponent, RetryDisposition,
        };
        match self {
            Self::HealthUnavailable => planning_failure(
                FailureClass::HealthUnavailable,
                FailureTarget::Request,
                RetryDisposition::TryNextCandidate,
            ),
            Self::CapacityExhausted => planning_failure(
                FailureClass::CapacityExhausted,
                FailureTarget::Request,
                RetryDisposition::WaitThenReplan,
            ),
            #[cfg(test)]
            Self::CandidateLimitExceeded { .. } => planning_failure(
                FailureClass::CandidateLimit,
                FailureTarget::Request,
                RetryDisposition::StopRequest,
            ),
            Self::ConfigUnstable => planning_failure(
                FailureClass::ConfigUnstable,
                FailureTarget::Request,
                RetryDisposition::TryNextCandidate,
            ),
            Self::DeadlineExceeded => planning_failure(
                FailureClass::Deadline,
                FailureTarget::Request,
                RetryDisposition::StopRequest,
            ),
            Self::InvariantViolation { .. } => planning_failure(
                FailureClass::Invariant,
                FailureTarget::LocalAdapter {
                    component: LocalAdapterComponent::Invariant,
                },
                RetryDisposition::StopRequest,
            ),
        }
    }

    pub(crate) fn stable_code(&self) -> &'static str {
        match self {
            Self::HealthUnavailable => "route_health_unavailable",
            Self::CapacityExhausted => "route_capacity_exhausted",
            #[cfg(test)]
            Self::CandidateLimitExceeded { actual, limit } => {
                if actual > limit { "route_candidate_limit_exceeded" } else { "route_candidate_limit_invalid" }
            }
            Self::ConfigUnstable => "route_configuration_changed",
            Self::DeadlineExceeded => "route_deadline_exceeded",
            Self::InvariantViolation { code } => code,
        }
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
    fn generic_not_found_is_uncertain_request_only() {
        let failure = classify_route_failure(RouteFailureInput::http_status(404, false));

        assert_eq!(failure.kind, RouteFailureKind::Uncertain);
        assert_eq!(failure.action, RouteFailureAction::IgnoreForKeyHealth);
        assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
    }

    #[test]
    fn model_not_found_is_uncertain_without_adapter_signal() {
        let failure = classify_route_failure(RouteFailureInput::http_status(404, false));

        assert_eq!(failure.kind, RouteFailureKind::Uncertain);
        assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
        assert!(!failure.retryable_before_output);
    }

    #[test]
    fn classifier_retries_candidate_auth_and_temporary_status_before_output() {
        for status in [401, 408, 425, 429, 500] {
            let failure = classify_route_failure(RouteFailureInput::http_status(status, false));

            assert!(failure.retryable_before_output, "status {status}");
        }
    }

    #[test]
    fn generic_forbidden_is_uncertain_and_neutral() {
        let failure = classify_route_failure(RouteFailureInput::http_status(403, false));

        assert_eq!(failure.kind, RouteFailureKind::Uncertain);
        assert_eq!(failure.action, RouteFailureAction::IgnoreForKeyHealth);
        assert_eq!(failure.scope, RouteFailureScope::RequestOnly);
        assert!(!failure.retryable_before_output);
    }

    #[test]
    fn adapter_confirmed_model_not_found_can_update_capability_only_when_applicable() {
        use crate::application::request_finalization::failure::{
            failure_from_provider_signal, CapabilityApplicabilitySet, CapabilityEffect,
            FailureClass, HealthEffect, ProviderErrorSemanticSignal,
        };

        let confirmed = failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedModelNotFound {
                station_key_id: "key-a".to_string(),
                model: "gpt-x".to_string(),
            },
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
        );
        assert_eq!(confirmed.class, FailureClass::ModelUnavailable);
        assert_eq!(confirmed.health, HealthEffect::Neutral);
        assert!(matches!(
            confirmed.capability,
            CapabilityEffect::ConfirmUnsupportedModel { .. }
        ));

        let blocked = failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedModelNotFound {
                station_key_id: "key-a".to_string(),
                model: "gpt-x".to_string(),
            },
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        assert_eq!(blocked.class, FailureClass::Uncertain);
        assert_eq!(blocked.capability, CapabilityEffect::Neutral);
    }

    #[test]
    fn classifier_stops_conflict_and_validation_statuses() {
        for status in [400, 409, 422] {
            let failure = classify_route_failure(RouteFailureInput::http_status(status, false));

            assert_eq!(failure.kind, RouteFailureKind::BadRequest);
            assert!(!failure.retryable_before_output, "status {status}");
        }
    }
}
