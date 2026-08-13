use http::StatusCode;
use serde_json::Value;

use super::request_send::RequestSendPhase;

use crate::application::request_finalization::failure::{
    BillingState, CanonicalFailure, EvidenceConfidence, FailureClass, FailureTarget, PublicError,
    PublicErrorCode, ReplaySafety, RequestAcceptance, RetryDisposition,
};
use crate::application::request_lifecycle::request::RequestRoutingOutcomeFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailureSource {
    Local,
    Routing,
    Upstream,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "reserved by the downstream-disconnect failure contract"
        )
    )]
    Downstream,
    Internal,
}

impl FailureSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Routing => "routing",
            Self::Upstream => "upstream",
            Self::Downstream => "downstream",
            Self::Internal => "internal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryClass {
    Never,
    BeforeOutput,
    AfterCommitStop,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyFailureCode {
    LocalProxyBusy,
    LocalProxyMemoryBusy,
    #[expect(
        dead_code,
        reason = "contract=local-proxy.request-header-failure; owner=services/proxy; remove_when=proxy header validation drops reserved failure variant"
    )]
    RequestHeaderTimeout,
    #[expect(
        dead_code,
        reason = "contract=local-proxy.request-header-failure; owner=services/proxy; remove_when=proxy header validation drops reserved failure variant"
    )]
    RequestHeaderTooLarge,
    RequestBodyTimeout,
    RequestBodyTooLarge,
    RequestBodyInvalid,
    LocalAuthMissing,
    LocalAuthInvalid,
    RouteNoCandidate,
    RouteWaitTimeout,
    RouteConfigRequired,
    RoutePolicyRejected,
    RouteEconomicsUnavailable,
    RouteHealthUnavailable,
    RouteCapacityExhausted,
    RouteCandidateLimitExceeded,
    RouteFactsUnavailable,
    RouteConfigUnstable,
    RouteLifecycleUnavailable,
    RouteDeadlineExceeded,
    RouteInvariantViolation,
    UpstreamConnectFailed,
    UpstreamFirstByteTimeout,
    UpstreamAuthenticationFailed,
    UpstreamInsufficientBalance,
    UpstreamRateLimited,
    UpstreamModelUnavailable,
    UpstreamCapabilityMismatch,
    UpstreamRequestRejected,
    UpstreamUnavailable,
    UpstreamOverloaded,
    UpstreamMalformedResponse,
    UpstreamUncertain,
    UpstreamStreamFailed,
    #[cfg_attr(
        not(test),
        allow(
            dead_code,
            reason = "reserved by the downstream-disconnect failure contract"
        )
    )]
    DownstreamDisconnected,
    ResponsesChatFallbackIncompatible,
    #[expect(
        dead_code,
        reason = "contract=local-proxy.application-update-admission; owner=services/proxy; remove_when=proxy update admission drops reserved failure variant"
    )]
    ApplicationUpdateInProgress,
    InternalProxyError,
}

impl ProxyFailureCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::LocalProxyBusy => "local_proxy_busy",
            Self::LocalProxyMemoryBusy => "local_proxy_memory_busy",
            Self::RequestHeaderTimeout => "request_header_timeout",
            Self::RequestHeaderTooLarge => "request_header_too_large",
            Self::RequestBodyTimeout => "request_body_timeout",
            Self::RequestBodyTooLarge => "request_body_too_large",
            Self::RequestBodyInvalid => "request_body_invalid",
            Self::LocalAuthMissing => "local_auth_missing",
            Self::LocalAuthInvalid => "local_auth_invalid",
            Self::RouteNoCandidate => "route_no_candidate",
            Self::RouteWaitTimeout => "route_wait_timeout",
            Self::RouteConfigRequired => "routing_configuration_required",
            Self::RoutePolicyRejected => "route_policy_rejected",
            Self::RouteEconomicsUnavailable => "route_economics_unavailable",
            Self::RouteHealthUnavailable => "route_health_unavailable",
            Self::RouteCapacityExhausted => "route_capacity_exhausted",
            Self::RouteCandidateLimitExceeded => "route_candidate_limit_exceeded",
            Self::RouteFactsUnavailable => "route_facts_unavailable",
            Self::RouteConfigUnstable => "route_configuration_changed",
            Self::RouteLifecycleUnavailable => "route_lifecycle_unavailable",
            Self::RouteDeadlineExceeded => "route_deadline_exceeded",
            Self::RouteInvariantViolation => "route_invariant_violation",
            Self::UpstreamConnectFailed => "upstream_connect_failed",
            Self::UpstreamFirstByteTimeout => "upstream_first_byte_timeout",
            Self::UpstreamAuthenticationFailed => "upstream_authentication_failed",
            Self::UpstreamInsufficientBalance => "upstream_insufficient_balance",
            Self::UpstreamRateLimited => "upstream_rate_limited",
            Self::UpstreamModelUnavailable => "upstream_model_unavailable",
            Self::UpstreamCapabilityMismatch => "upstream_capability_mismatch",
            Self::UpstreamRequestRejected => "upstream_request_rejected",
            Self::UpstreamUnavailable => "upstream_unavailable",
            Self::UpstreamOverloaded => "upstream_overloaded",
            Self::UpstreamMalformedResponse => "upstream_malformed_response",
            Self::UpstreamUncertain => "upstream_uncertain",
            Self::UpstreamStreamFailed => "upstream_stream_failed",
            Self::DownstreamDisconnected => "downstream_disconnected",
            Self::ResponsesChatFallbackIncompatible => "responses_chat_fallback_incompatible",
            Self::ApplicationUpdateInProgress => "application_update_in_progress",
            Self::InternalProxyError => "internal_proxy_error",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyFailure {
    pub code: ProxyFailureCode,
    pub source: FailureSource,
    pub retry_class: RetryClass,
    pub http_status: StatusCode,
    pub public_message: String,
    pub retry_after_ms: Option<i64>,
    pub internal_detail: Option<String>,
    pub(crate) request_send_phase: RequestSendPhase,
    canonical: Option<Box<CanonicalFailure>>,
    context: Option<Box<ProxyFailureContext>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProxyFailureContext {
    pub(crate) candidate_id: Option<String>,
    pub(crate) candidate_station_id: Option<String>,
    pub(crate) candidate_upstream_base_url: Option<String>,
    pub(crate) attempt_count: Option<i64>,
    pub(crate) route_policy: Option<String>,
}

impl ProxyFailure {
    pub fn new(
        code: ProxyFailureCode,
        source: FailureSource,
        retry_class: RetryClass,
        http_status: StatusCode,
        public_message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            source,
            retry_class,
            http_status,
            public_message: public_message.into(),
            retry_after_ms: None,
            internal_detail: None,
            request_send_phase: RequestSendPhase::Unknown,
            canonical: None,
            context: None,
        }
    }

    pub(crate) fn context_mut(&mut self) -> &mut ProxyFailureContext {
        self.context
            .get_or_insert_with(|| Box::new(ProxyFailureContext::default()))
    }

    pub(crate) fn candidate_id(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.candidate_id.as_deref())
    }

    pub(crate) fn candidate_station_id(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.candidate_station_id.as_deref())
    }

    pub(crate) fn candidate_upstream_base_url(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.candidate_upstream_base_url.as_deref())
    }

    pub(crate) fn attempt_count(&self) -> Option<i64> {
        self.context
            .as_deref()
            .and_then(|context| context.attempt_count)
    }

    pub(crate) fn route_policy(&self) -> Option<&str> {
        self.context
            .as_deref()
            .and_then(|context| context.route_policy.as_deref())
    }

    pub fn into_response(self) -> (StatusCode, Value) {
        // Upstream failures must speak stable OpenAI-compatible semantics to
        // Codex/OpenAI SDK clients (Task 7 public adapter). Local, routing,
        // internal and downstream failures keep the stable local envelope so
        // the desktop tool's own consumers can distinguish auth, body and
        // routing conditions without re-deriving them from an upstream code.
        if self.source == FailureSource::Upstream {
            let public = adapt_proxy_failure(&self);
            return (public.status, public.into_json());
        }
        let message = crate::services::secrets::mask::redact_text(&self.public_message);
        (
            self.http_status,
            serde_json::json!({
                "error": {
                    "message": message,
                    "type": "relay_pool_error",
                    "param": Value::Null,
                    "code": self.code.as_str(),
                }
            }),
        )
    }

    pub(crate) fn from_public_error(error: PublicError) -> Self {
        Self::new(
            proxy_failure_code_for_public_error(error.code),
            failure_source_for_public_error(error.code),
            retry_class_for_public_error(error.code),
            error.http_status,
            error.message,
        )
    }

    /// Preserve the complete canonical outcome for every downstream consumer.
    /// Public HTTP fields are a projection only and must never become an input
    /// to retry, health, capability, or lifecycle decisions.
    pub(crate) fn from_canonical(canonical: CanonicalFailure) -> Self {
        let mut failure = Self::from_public_error(canonical.public.clone());
        failure.retry_after_ms = match canonical.health {
            crate::application::request_finalization::failure::HealthEffect::Cooldown {
                retry_after_ms,
            } => retry_after_ms,
            _ => None,
        };
        failure.canonical = Some(Box::new(canonical));
        failure
    }

    pub(crate) fn canonical(&self) -> Option<&CanonicalFailure> {
        self.canonical.as_deref()
    }

    pub(crate) fn with_request_send_phase(mut self, phase: RequestSendPhase) -> Self {
        self.request_send_phase = phase;
        self
    }

    /// Creates the closed terminal facts while the canonical failure and its
    /// transport-owned phase are still available. Persistence must not recreate
    /// these facts from the public error projection.
    pub(crate) fn routing_outcome_facts(&self) -> Option<RequestRoutingOutcomeFacts> {
        let canonical = self.canonical()?;
        let (failure_domain_commitment_version, failure_domain_commitment_digest) = match &canonical
            .target
        {
            FailureTarget::ProviderCapacity { domain_commitment } => {
                let commitment = crate::application::routing_engine::failure_domains::CapacityDomainCommitment::from_canonical(domain_commitment)?;
                (
                    Some(i64::from(commitment.schema_version)),
                    Some(commitment.digest_hex),
                )
            }
            _ => (None, None),
        };
        Some(RequestRoutingOutcomeFacts {
            classification: canonical_classification(canonical.class).to_string(),
            confidence: confidence_label(canonical.confidence).to_string(),
            evidence_source: evidence_source_label(canonical.class).to_string(),
            request_accepted: acceptance_label(canonical.request_acceptance).to_string(),
            send_phase: send_phase_label(self.request_send_phase).to_string(),
            replay_disposition: replay_label(canonical.replay_safety).to_string(),
            billing_state: billing_label(canonical.billing).to_string(),
            retry_disposition: retry_label(canonical.retry).to_string(),
            effect_summary: effect_summary(canonical).to_string(),
            failure_domain_commitment_version,
            failure_domain_commitment_digest,
        })
    }
}

fn canonical_classification(class: FailureClass) -> &'static str {
    match class {
        FailureClass::Authentication => "authentication",
        FailureClass::InsufficientBalance => "balance",
        FailureClass::RateLimited | FailureClass::QuotaExhausted => "rate_limit",
        FailureClass::ProviderCapacity | FailureClass::CapacityExhausted => "capacity",
        FailureClass::ModelUnavailable => "model_not_found",
        FailureClass::Transport => "transport",
        FailureClass::Timeout | FailureClass::Deadline => "timeout",
        FailureClass::MalformedResponse | FailureClass::StreamInterrupted => "protocol",
        FailureClass::DownstreamDrop => "downstream",
        FailureClass::Upstream5xx
        | FailureClass::UpstreamOverloaded
        | FailureClass::RelayServiceUnavailable => "server_error",
        FailureClass::ConfigRequired
        | FailureClass::PolicyRejected
        | FailureClass::EconomicsUnavailable
        | FailureClass::HealthUnavailable
        | FailureClass::RuntimeConcurrencyLimited
        | FailureClass::CapabilityMismatch
        | FailureClass::BadRequest
        | FailureClass::ProviderRejectedRequest
        | FailureClass::CandidateLimit
        | FailureClass::FactsUnavailable
        | FailureClass::ConfigUnstable
        | FailureClass::Lifecycle
        | FailureClass::Invariant => "local",
        FailureClass::Uncertain => "generic",
    }
}

fn confidence_label(confidence: EvidenceConfidence) -> &'static str {
    match confidence {
        EvidenceConfidence::Confirmed => "confirmed",
        EvidenceConfidence::Probable => "probable",
        EvidenceConfidence::Unknown => "unknown",
        EvidenceConfidence::Conflicting => "conflicting",
    }
}

fn evidence_source_label(class: FailureClass) -> &'static str {
    match class {
        FailureClass::Transport => "transport",
        FailureClass::Timeout | FailureClass::Deadline => "timeout",
        FailureClass::StreamInterrupted | FailureClass::MalformedResponse => "sse_event",
        FailureClass::DownstreamDrop => "downstream",
        FailureClass::ConfigRequired
        | FailureClass::PolicyRejected
        | FailureClass::EconomicsUnavailable
        | FailureClass::HealthUnavailable
        | FailureClass::RuntimeConcurrencyLimited
        | FailureClass::CandidateLimit
        | FailureClass::FactsUnavailable
        | FailureClass::ConfigUnstable
        | FailureClass::Lifecycle
        | FailureClass::Invariant => "local",
        _ => "error_envelope",
    }
}

fn acceptance_label(acceptance: RequestAcceptance) -> &'static str {
    match acceptance {
        RequestAcceptance::RejectedBeforeAcceptance => "not_accepted",
        RequestAcceptance::AcceptedOrMayHaveBeenAccepted => "accepted",
        RequestAcceptance::Unknown => "unknown",
    }
}

fn send_phase_label(phase: RequestSendPhase) -> &'static str {
    match phase {
        RequestSendPhase::NotConnected => "not_connected",
        RequestSendPhase::ResponseStarted => "response_started",
        RequestSendPhase::Unknown => "unknown",
        #[cfg(test)]
        RequestSendPhase::ConnectedNoHeaders
        | RequestSendPhase::HeadersSent
        | RequestSendPhase::BodyPartiallySent
        | RequestSendPhase::BodyFullySent => "unknown",
    }
}

fn replay_label(replay: ReplaySafety) -> &'static str {
    match replay {
        ReplaySafety::ReplaySafe => "not_applicable",
        ReplaySafety::RequiresProviderIdempotency | ReplaySafety::NotReplayable => {
            "stopped_uncertain"
        }
    }
}

fn billing_label(billing: BillingState) -> &'static str {
    match billing {
        BillingState::BillingUncertain => "possibly_billed",
    }
}

fn retry_label(retry: RetryDisposition) -> &'static str {
    match retry {
        RetryDisposition::RetrySameTarget => "same_target_exhausted",
        RetryDisposition::TryDifferentFailureDomain
        | RetryDisposition::WaitThenReplan
        | RetryDisposition::StopRequest => "fail_closed",
    }
}

fn effect_summary(canonical: &CanonicalFailure) -> &'static str {
    if matches!(
        canonical.health,
        crate::application::request_finalization::failure::HealthEffect::Neutral
    ) && matches!(
        canonical.capability,
        crate::application::request_finalization::failure::CapabilityEffect::Neutral
    ) {
        "neutral"
    } else {
        "health_or_capability_applied"
    }
}

fn proxy_failure_code_for_public_error(code: PublicErrorCode) -> ProxyFailureCode {
    match code {
        PublicErrorCode::ConfigRequired => ProxyFailureCode::RouteConfigRequired,
        PublicErrorCode::PolicyRejected => ProxyFailureCode::RoutePolicyRejected,
        PublicErrorCode::EconomicsUnavailable => ProxyFailureCode::RouteEconomicsUnavailable,
        PublicErrorCode::HealthUnavailable => ProxyFailureCode::RouteHealthUnavailable,
        PublicErrorCode::AuthenticationFailed => ProxyFailureCode::UpstreamAuthenticationFailed,
        PublicErrorCode::InsufficientBalance => ProxyFailureCode::UpstreamInsufficientBalance,
        PublicErrorCode::RateLimited => ProxyFailureCode::UpstreamRateLimited,
        PublicErrorCode::ModelUnavailable => ProxyFailureCode::UpstreamModelUnavailable,
        PublicErrorCode::CapabilityMismatch => ProxyFailureCode::UpstreamCapabilityMismatch,
        PublicErrorCode::UpstreamRequestRejected => ProxyFailureCode::UpstreamRequestRejected,
        PublicErrorCode::UpstreamUnavailable => ProxyFailureCode::UpstreamUnavailable,
        PublicErrorCode::UpstreamOverloaded => ProxyFailureCode::UpstreamOverloaded,
        PublicErrorCode::UpstreamUncertain => ProxyFailureCode::UpstreamUncertain,
        PublicErrorCode::BadRequest => ProxyFailureCode::RequestBodyInvalid,
        PublicErrorCode::Timeout => ProxyFailureCode::UpstreamFirstByteTimeout,
        PublicErrorCode::TransportFailure => ProxyFailureCode::UpstreamConnectFailed,
        PublicErrorCode::MalformedResponse => ProxyFailureCode::UpstreamMalformedResponse,
        PublicErrorCode::StreamInterrupted => ProxyFailureCode::UpstreamStreamFailed,
        PublicErrorCode::DownstreamDisconnected => ProxyFailureCode::DownstreamDisconnected,
        PublicErrorCode::CapacityExhausted => ProxyFailureCode::RouteCapacityExhausted,
        PublicErrorCode::CandidateLimitExceeded => ProxyFailureCode::RouteCandidateLimitExceeded,
        PublicErrorCode::FactsUnavailable => ProxyFailureCode::RouteFactsUnavailable,
        PublicErrorCode::ConfigUnstable => ProxyFailureCode::RouteConfigUnstable,
        PublicErrorCode::LifecycleUnavailable => ProxyFailureCode::RouteLifecycleUnavailable,
        PublicErrorCode::DeadlineExceeded => ProxyFailureCode::RouteDeadlineExceeded,
        PublicErrorCode::InvariantViolation => ProxyFailureCode::RouteInvariantViolation,
    }
}

fn failure_source_for_public_error(code: PublicErrorCode) -> FailureSource {
    match code {
        PublicErrorCode::AuthenticationFailed
        | PublicErrorCode::InsufficientBalance
        | PublicErrorCode::RateLimited
        | PublicErrorCode::ModelUnavailable
        | PublicErrorCode::CapabilityMismatch
        | PublicErrorCode::UpstreamRequestRejected
        | PublicErrorCode::Timeout
        | PublicErrorCode::TransportFailure
        | PublicErrorCode::UpstreamUnavailable
        | PublicErrorCode::UpstreamOverloaded
        | PublicErrorCode::MalformedResponse
        | PublicErrorCode::StreamInterrupted
        | PublicErrorCode::UpstreamUncertain => FailureSource::Upstream,
        PublicErrorCode::DownstreamDisconnected => FailureSource::Downstream,
        PublicErrorCode::InvariantViolation => FailureSource::Internal,
        PublicErrorCode::ConfigRequired
        | PublicErrorCode::PolicyRejected
        | PublicErrorCode::EconomicsUnavailable
        | PublicErrorCode::HealthUnavailable
        | PublicErrorCode::BadRequest
        | PublicErrorCode::CapacityExhausted
        | PublicErrorCode::CandidateLimitExceeded
        | PublicErrorCode::FactsUnavailable
        | PublicErrorCode::ConfigUnstable
        | PublicErrorCode::LifecycleUnavailable
        | PublicErrorCode::DeadlineExceeded => FailureSource::Routing,
    }
}

fn retry_class_for_public_error(code: PublicErrorCode) -> RetryClass {
    match code {
        PublicErrorCode::RateLimited
        | PublicErrorCode::Timeout
        | PublicErrorCode::TransportFailure
        | PublicErrorCode::UpstreamUnavailable
        | PublicErrorCode::UpstreamOverloaded
        | PublicErrorCode::StreamInterrupted
        | PublicErrorCode::CapacityExhausted
        | PublicErrorCode::EconomicsUnavailable
        | PublicErrorCode::HealthUnavailable
        | PublicErrorCode::FactsUnavailable
        | PublicErrorCode::ConfigUnstable
        | PublicErrorCode::LifecycleUnavailable
        | PublicErrorCode::DeadlineExceeded => RetryClass::BeforeOutput,
        PublicErrorCode::ConfigRequired
        | PublicErrorCode::PolicyRejected
        | PublicErrorCode::AuthenticationFailed
        | PublicErrorCode::InsufficientBalance
        | PublicErrorCode::ModelUnavailable
        | PublicErrorCode::CapabilityMismatch
        | PublicErrorCode::UpstreamRequestRejected
        | PublicErrorCode::BadRequest
        | PublicErrorCode::MalformedResponse
        | PublicErrorCode::DownstreamDisconnected
        | PublicErrorCode::CandidateLimitExceeded
        | PublicErrorCode::InvariantViolation
        | PublicErrorCode::UpstreamUncertain => RetryClass::Never,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiPublicError {
    pub(crate) status: StatusCode,
    pub(crate) error_type: &'static str,
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

impl OpenAiPublicError {
    pub(crate) fn into_json(self) -> Value {
        serde_json::json!({
            "error": {
                "message": self.message,
                "type": self.error_type,
                "param": Value::Null,
                "code": self.code,
            }
        })
    }
}

pub(crate) fn adapt_proxy_failure(failure: &ProxyFailure) -> OpenAiPublicError {
    let message = crate::services::secrets::mask::redact_text(&failure.public_message);
    let (status, error_type, code) = match failure.code {
        ProxyFailureCode::LocalAuthMissing | ProxyFailureCode::LocalAuthInvalid => (
            StatusCode::UNAUTHORIZED,
            "authentication_error",
            "invalid_api_key",
        ),
        ProxyFailureCode::UpstreamAuthenticationFailed => {
            (StatusCode::BAD_GATEWAY, "server_error", "server_error")
        }
        ProxyFailureCode::UpstreamRateLimited => (
            StatusCode::TOO_MANY_REQUESTS,
            "rate_limit_error",
            "rate_limit_exceeded",
        ),
        ProxyFailureCode::UpstreamModelUnavailable => (
            StatusCode::NOT_FOUND,
            "invalid_request_error",
            "model_not_found",
        ),
        ProxyFailureCode::RequestBodyInvalid
        | ProxyFailureCode::RequestBodyTooLarge
        | ProxyFailureCode::RequestBodyTimeout
        | ProxyFailureCode::UpstreamRequestRejected => (
            StatusCode::BAD_REQUEST,
            "invalid_request_error",
            "invalid_request_error",
        ),
        ProxyFailureCode::UpstreamOverloaded => (
            StatusCode::SERVICE_UNAVAILABLE,
            "server_error",
            "server_error",
        ),
        ProxyFailureCode::UpstreamUnavailable
        | ProxyFailureCode::UpstreamConnectFailed
        | ProxyFailureCode::UpstreamFirstByteTimeout
        | ProxyFailureCode::UpstreamMalformedResponse
        | ProxyFailureCode::UpstreamUncertain
        | ProxyFailureCode::UpstreamStreamFailed => (
            if failure.http_status == StatusCode::SERVICE_UNAVAILABLE {
                StatusCode::SERVICE_UNAVAILABLE
            } else {
                StatusCode::BAD_GATEWAY
            },
            "server_error",
            "server_error",
        ),
        _ if matches!(failure.source, FailureSource::Upstream) => {
            (failure.http_status, "server_error", "server_error")
        }
        _ => (
            failure.http_status,
            "relay_pool_error",
            failure.code.as_str(),
        ),
    };
    OpenAiPublicError {
        status,
        error_type,
        code,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::request_finalization::failure::{
        failure_from_provider_signal, CapabilityApplicabilitySet, ProviderErrorSemanticSignal,
    };
    use crate::services::proxy::error::{FailureSource, RetryClass};

    fn failure(code: ProxyFailureCode, status: StatusCode) -> ProxyFailure {
        ProxyFailure::new(
            code,
            FailureSource::Upstream,
            RetryClass::Never,
            status,
            "safe",
        )
    }

    #[test]
    fn capacity_and_upstream_auth_use_retry_compatible_server_error() {
        let capacity = adapt_proxy_failure(&failure(
            ProxyFailureCode::UpstreamOverloaded,
            StatusCode::SERVICE_UNAVAILABLE,
        ));
        assert_eq!(
            (capacity.status, capacity.error_type, capacity.code),
            (
                StatusCode::SERVICE_UNAVAILABLE,
                "server_error",
                "server_error"
            )
        );
        let auth = adapt_proxy_failure(&failure(
            ProxyFailureCode::UpstreamAuthenticationFailed,
            StatusCode::UNAUTHORIZED,
        ));
        assert_eq!(
            (auth.status, auth.error_type, auth.code),
            (StatusCode::BAD_GATEWAY, "server_error", "server_error")
        );
    }

    #[test]
    fn local_auth_remains_a_real_authentication_error() {
        let mut local = failure(ProxyFailureCode::LocalAuthInvalid, StatusCode::UNAUTHORIZED);
        local.source = FailureSource::Local;
        let public = adapt_proxy_failure(&local);
        assert_eq!(
            (public.error_type, public.code),
            ("authentication_error", "invalid_api_key")
        );
    }

    #[test]
    fn public_http_contract_is_golden() {
        let cases = [
            (
                ProxyFailureCode::UpstreamOverloaded,
                StatusCode::SERVICE_UNAVAILABLE,
                503,
                "server_error",
                "server_error",
            ),
            (
                ProxyFailureCode::UpstreamUnavailable,
                StatusCode::BAD_GATEWAY,
                502,
                "server_error",
                "server_error",
            ),
            (
                ProxyFailureCode::UpstreamRateLimited,
                StatusCode::TOO_MANY_REQUESTS,
                429,
                "rate_limit_error",
                "rate_limit_exceeded",
            ),
            (
                ProxyFailureCode::UpstreamModelUnavailable,
                StatusCode::NOT_FOUND,
                404,
                "invalid_request_error",
                "model_not_found",
            ),
            (
                ProxyFailureCode::UpstreamRequestRejected,
                StatusCode::BAD_REQUEST,
                400,
                "invalid_request_error",
                "invalid_request_error",
            ),
        ];
        for (failure_code, source_status, status, error_type, code) in cases {
            let public = adapt_proxy_failure(&failure(failure_code, source_status));
            assert_eq!(
                (public.status.as_u16(), public.error_type, public.code),
                (status, error_type, code)
            );
        }
    }

    #[test]
    fn canonical_failure_outcome_preserves_classification_and_transport_phase() {
        let digest = "b".repeat(64);
        let failure = ProxyFailure::from_canonical(failure_from_provider_signal(
            ProviderErrorSemanticSignal::ProviderCapacity {
                domain_commitment: format!("v1:{digest}"),
                retry_after_ms: None,
            },
            CapabilityApplicabilitySet::UnknownModelCatalog,
        ))
        .with_request_send_phase(RequestSendPhase::ResponseStarted);

        let facts = failure.routing_outcome_facts().expect("canonical facts");
        assert_eq!(facts.classification, "capacity");
        assert_eq!(facts.confidence, "confirmed");
        assert_eq!(facts.send_phase, "response_started");
        assert_eq!(facts.failure_domain_commitment_version, Some(1));
        assert_eq!(
            facts.failure_domain_commitment_digest.as_deref(),
            Some(digest.as_str())
        );
    }
}
