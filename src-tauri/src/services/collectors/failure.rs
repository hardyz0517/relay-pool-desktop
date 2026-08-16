use std::time::Duration;

use crate::services::collectors::evidence::{redact_text, EndpointRole, EvidenceSet};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriverFailure {
    pub kind: DriverFailureKind,
    pub retry: RetryDisposition,
    pub auth_effect: AuthEffect,
    pub endpoint: Option<FailedEndpoint>,
    pub evidence: EvidenceSet,
    pub sanitized_detail: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverFailureKind {
    Unsupported,
    InvalidRequest,
    AuthRejected,
    BrowserContextRequired,
    RateLimited,
    Timeout,
    BudgetExhausted,
    Cancelled,
    ResultUnknown,
    Transport,
    MalformedPayload,
    ProviderUnavailable,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryDisposition {
    Never,
    After(Duration),
    WithinBudget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthEffect {
    None,
    RefreshSession,
    Reauthorize,
    InvalidateCredential,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FailedEndpoint {
    pub role: EndpointRole,
    pub status_code: Option<u16>,
}

impl DriverFailure {
    pub fn unsupported(detail: impl Into<String>) -> Self {
        Self {
            kind: DriverFailureKind::Unsupported,
            retry: RetryDisposition::Never,
            auth_effect: AuthEffect::None,
            endpoint: None,
            evidence: EvidenceSet::empty(),
            sanitized_detail: Some(redact_text(&detail.into())),
        }
    }

    pub fn auth_rejected(endpoint: FailedEndpoint, detail: impl Into<String>) -> Self {
        Self {
            kind: DriverFailureKind::AuthRejected,
            retry: RetryDisposition::Never,
            auth_effect: AuthEffect::InvalidateCredential,
            endpoint: Some(endpoint),
            evidence: EvidenceSet::empty(),
            sanitized_detail: Some(redact_text(&detail.into())),
        }
    }

    pub fn reauthorization_required(endpoint: FailedEndpoint, detail: impl Into<String>) -> Self {
        Self {
            kind: DriverFailureKind::AuthRejected,
            retry: RetryDisposition::Never,
            auth_effect: AuthEffect::Reauthorize,
            endpoint: Some(endpoint),
            evidence: EvidenceSet::empty(),
            sanitized_detail: Some(redact_text(&detail.into())),
        }
    }

    pub fn with_evidence(mut self, evidence: EvidenceSet) -> Self {
        self.evidence = evidence;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsupported_failure_is_typed_not_message_driven() {
        let failure = DriverFailure::unsupported("provider does not support remote keys");

        assert_eq!(failure.kind, DriverFailureKind::Unsupported);
        assert_eq!(failure.retry, RetryDisposition::Never);
        assert_eq!(failure.auth_effect, AuthEffect::None);
        assert!(failure.endpoint.is_none());
    }

    #[test]
    fn auth_failure_carries_decision_fields() {
        let failure = DriverFailure::auth_rejected(
            FailedEndpoint {
                role: EndpointRole::Authorization,
                status_code: Some(401),
            },
            "Authorization: Bearer sk-p8-secret-plaintext-canary was rejected",
        );

        assert_eq!(failure.kind, DriverFailureKind::AuthRejected);
        assert_eq!(failure.retry, RetryDisposition::Never);
        assert_eq!(failure.auth_effect, AuthEffect::InvalidateCredential);
        assert_eq!(
            failure.endpoint,
            Some(FailedEndpoint {
                role: EndpointRole::Authorization,
                status_code: Some(401)
            })
        );
        assert!(!failure
            .sanitized_detail
            .as_deref()
            .unwrap_or_default()
            .contains("sk-p8-secret-plaintext-canary"));
    }

    #[test]
    fn reauthorization_failure_is_an_auth_failure_with_a_recovery_action() {
        let failure = DriverFailure::reauthorization_required(
            FailedEndpoint {
                role: EndpointRole::Authorization,
                status_code: Some(401),
            },
            "saved session expired",
        );

        assert_eq!(failure.kind, DriverFailureKind::AuthRejected);
        assert_eq!(failure.retry, RetryDisposition::Never);
        assert_eq!(failure.auth_effect, AuthEffect::Reauthorize);
    }
}
