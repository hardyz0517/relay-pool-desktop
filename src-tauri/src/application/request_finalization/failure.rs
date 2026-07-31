#![allow(dead_code)]

use http::StatusCode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FailureTarget {
    Request,
    ModelOnKey {
        station_key_id: String,
        model: String,
    },
    StationKeyCredential {
        station_key_id: String,
    },
    StationAccount {
        station_id: String,
    },
    StationEndpoint {
        station_id: String,
        endpoint_revision: i64,
    },
    ProviderProtocol {
        protocol: ProviderProtocolKind,
    },
    LocalAdapter {
        component: LocalAdapterComponent,
    },
    Downstream,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderProtocolKind {
    OpenAiChatCompletions,
    OpenAiResponses,
    OpenAiCompatible,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocalAdapterComponent {
    RequestBody,
    ResponseTransform,
    Lifecycle,
    Invariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FailureClass {
    ConfigRequired,
    PolicyRejected,
    EconomicsUnavailable,
    HealthUnavailable,
    Authentication,
    InsufficientBalance,
    RateLimited,
    ModelUnavailable,
    CapabilityMismatch,
    BadRequest,
    Timeout,
    Transport,
    Upstream5xx,
    MalformedResponse,
    StreamInterrupted,
    DownstreamDrop,
    CapacityExhausted,
    CandidateLimit,
    FactsUnavailable,
    ConfigUnstable,
    Lifecycle,
    Deadline,
    Invariant,
    Uncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RetryDisposition {
    TryNextCandidate,
    WaitThenReplan,
    StopRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HealthEffect {
    Success,
    ObserveFailure,
    Cooldown { retry_after_ms: Option<i64> },
    HardFail,
    Neutral,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CapabilityEffect {
    Neutral,
    ConfirmUnsupportedModel {
        station_key_id: String,
        model: String,
    },
    ConfirmUnsupportedProtocol {
        protocol: ProviderProtocolKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CapabilityApplicabilitySet {
    ConfirmedModelCatalog,
    UnknownModelCatalog,
    PositiveCapabilityEvidence,
    LoadEvidenceGap,
    RequestPolicyOnly,
}

impl CapabilityApplicabilitySet {
    pub(crate) fn permits_model_not_found_learning(self) -> bool {
        matches!(self, Self::ConfirmedModelCatalog)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CanonicalFailure {
    pub(crate) target: FailureTarget,
    pub(crate) class: FailureClass,
    pub(crate) retry: RetryDisposition,
    pub(crate) health: HealthEffect,
    pub(crate) capability: CapabilityEffect,
    pub(crate) public: PublicError,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublicError {
    pub(crate) code: PublicErrorCode,
    pub(crate) http_status: StatusCode,
    pub(crate) message: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PublicErrorCode {
    ConfigRequired,
    PolicyRejected,
    EconomicsUnavailable,
    HealthUnavailable,
    AuthenticationFailed,
    InsufficientBalance,
    RateLimited,
    ModelUnavailable,
    CapabilityMismatch,
    BadRequest,
    Timeout,
    TransportFailure,
    UpstreamUnavailable,
    MalformedResponse,
    StreamInterrupted,
    DownstreamDisconnected,
    CapacityExhausted,
    CandidateLimitExceeded,
    FactsUnavailable,
    ConfigUnstable,
    LifecycleUnavailable,
    DeadlineExceeded,
    InvariantViolation,
    UpstreamUncertain,
}

impl PublicErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ConfigRequired => "route_config_required",
            Self::PolicyRejected => "route_policy_rejected",
            Self::EconomicsUnavailable => "route_economics_unavailable",
            Self::HealthUnavailable => "route_health_unavailable",
            Self::AuthenticationFailed => "upstream_authentication_failed",
            Self::InsufficientBalance => "upstream_insufficient_balance",
            Self::RateLimited => "upstream_rate_limited",
            Self::ModelUnavailable => "upstream_model_unavailable",
            Self::CapabilityMismatch => "upstream_capability_mismatch",
            Self::BadRequest => "request_bad_request",
            Self::Timeout => "upstream_timeout",
            Self::TransportFailure => "upstream_transport_failure",
            Self::UpstreamUnavailable => "upstream_unavailable",
            Self::MalformedResponse => "upstream_malformed_response",
            Self::StreamInterrupted => "upstream_stream_interrupted",
            Self::DownstreamDisconnected => "downstream_disconnected",
            Self::CapacityExhausted => "route_capacity_exhausted",
            Self::CandidateLimitExceeded => "route_candidate_limit_exceeded",
            Self::FactsUnavailable => "route_facts_unavailable",
            Self::ConfigUnstable => "route_config_unstable",
            Self::LifecycleUnavailable => "route_lifecycle_unavailable",
            Self::DeadlineExceeded => "route_deadline_exceeded",
            Self::InvariantViolation => "route_invariant_violation",
            Self::UpstreamUncertain => "upstream_uncertain",
        }
    }
}

pub(crate) fn public_error_for_class(class: FailureClass) -> PublicError {
    match class {
        FailureClass::ConfigRequired => PublicError {
            code: PublicErrorCode::ConfigRequired,
            http_status: StatusCode::PRECONDITION_REQUIRED,
            message: "routing configuration is required",
        },
        FailureClass::PolicyRejected => PublicError {
            code: PublicErrorCode::PolicyRejected,
            http_status: StatusCode::BAD_REQUEST,
            message: "request is rejected by routing policy",
        },
        FailureClass::EconomicsUnavailable => PublicError {
            code: PublicErrorCode::EconomicsUnavailable,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing economics are unavailable",
        },
        FailureClass::HealthUnavailable => PublicError {
            code: PublicErrorCode::HealthUnavailable,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing health is unavailable",
        },
        FailureClass::Authentication => PublicError {
            code: PublicErrorCode::AuthenticationFailed,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream authentication failed",
        },
        FailureClass::InsufficientBalance => PublicError {
            code: PublicErrorCode::InsufficientBalance,
            http_status: StatusCode::PAYMENT_REQUIRED,
            message: "upstream balance is insufficient",
        },
        FailureClass::RateLimited => PublicError {
            code: PublicErrorCode::RateLimited,
            http_status: StatusCode::TOO_MANY_REQUESTS,
            message: "upstream is rate limited",
        },
        FailureClass::ModelUnavailable => PublicError {
            code: PublicErrorCode::ModelUnavailable,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream model is unavailable",
        },
        FailureClass::CapabilityMismatch => PublicError {
            code: PublicErrorCode::CapabilityMismatch,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream capability is not compatible",
        },
        FailureClass::BadRequest => PublicError {
            code: PublicErrorCode::BadRequest,
            http_status: StatusCode::BAD_REQUEST,
            message: "request body is invalid",
        },
        FailureClass::Timeout => PublicError {
            code: PublicErrorCode::Timeout,
            http_status: StatusCode::GATEWAY_TIMEOUT,
            message: "upstream timed out",
        },
        FailureClass::Transport => PublicError {
            code: PublicErrorCode::TransportFailure,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream transport failed",
        },
        FailureClass::Upstream5xx => PublicError {
            code: PublicErrorCode::UpstreamUnavailable,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream is unavailable",
        },
        FailureClass::MalformedResponse => PublicError {
            code: PublicErrorCode::MalformedResponse,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream response is malformed",
        },
        FailureClass::StreamInterrupted => PublicError {
            code: PublicErrorCode::StreamInterrupted,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream stream was interrupted",
        },
        FailureClass::DownstreamDrop => PublicError {
            code: PublicErrorCode::DownstreamDisconnected,
            http_status: StatusCode::BAD_GATEWAY,
            message: "downstream disconnected",
        },
        FailureClass::CapacityExhausted => PublicError {
            code: PublicErrorCode::CapacityExhausted,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing capacity is exhausted",
        },
        FailureClass::CandidateLimit => PublicError {
            code: PublicErrorCode::CandidateLimitExceeded,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing candidate limit is exceeded",
        },
        FailureClass::FactsUnavailable => PublicError {
            code: PublicErrorCode::FactsUnavailable,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing facts are unavailable",
        },
        FailureClass::ConfigUnstable => PublicError {
            code: PublicErrorCode::ConfigUnstable,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing configuration changed during planning",
        },
        FailureClass::Lifecycle => PublicError {
            code: PublicErrorCode::LifecycleUnavailable,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "request lifecycle writer is unavailable",
        },
        FailureClass::Deadline => PublicError {
            code: PublicErrorCode::DeadlineExceeded,
            http_status: StatusCode::GATEWAY_TIMEOUT,
            message: "routing deadline was exceeded",
        },
        FailureClass::Invariant => PublicError {
            code: PublicErrorCode::InvariantViolation,
            http_status: StatusCode::INTERNAL_SERVER_ERROR,
            message: "routing invariant was violated",
        },
        FailureClass::Uncertain => PublicError {
            code: PublicErrorCode::UpstreamUncertain,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream failure is uncertain",
        },
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProviderErrorSemanticSignal {
    ConfirmedAuthentication {
        station_key_id: String,
    },
    ConfirmedModelNotFound {
        station_key_id: String,
        model: String,
    },
    ConfirmedInsufficientBalance {
        station_id: String,
    },
    ConfirmedCapabilityMismatch {
        protocol: ProviderProtocolKind,
    },
    RateLimited {
        station_id: String,
        retry_after_ms: Option<i64>,
    },
    BadRequest,
    ServerError {
        station_id: String,
        endpoint_revision: i64,
    },
    GenericStatus {
        status: u16,
    },
    Transport,
    Timeout,
    MalformedResponse,
}

pub(crate) fn failure_from_provider_signal(
    signal: ProviderErrorSemanticSignal,
    applicability: CapabilityApplicabilitySet,
) -> CanonicalFailure {
    let (target, class, retry, health, capability) = match signal {
        ProviderErrorSemanticSignal::ConfirmedAuthentication { station_key_id } => (
            FailureTarget::StationKeyCredential { station_key_id },
            FailureClass::Authentication,
            RetryDisposition::TryNextCandidate,
            HealthEffect::HardFail,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ConfirmedModelNotFound {
            station_key_id,
            model,
        } if applicability.permits_model_not_found_learning() => (
            FailureTarget::ModelOnKey {
                station_key_id: station_key_id.clone(),
                model: model.clone(),
            },
            FailureClass::ModelUnavailable,
            RetryDisposition::StopRequest,
            HealthEffect::Neutral,
            CapabilityEffect::ConfirmUnsupportedModel {
                station_key_id,
                model,
            },
        ),
        ProviderErrorSemanticSignal::ConfirmedModelNotFound { .. } => (
            FailureTarget::Uncertain,
            FailureClass::Uncertain,
            RetryDisposition::StopRequest,
            HealthEffect::Neutral,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ConfirmedInsufficientBalance { station_id } => (
            FailureTarget::StationAccount { station_id },
            FailureClass::InsufficientBalance,
            RetryDisposition::TryNextCandidate,
            HealthEffect::HardFail,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ConfirmedCapabilityMismatch { protocol } => (
            FailureTarget::ProviderProtocol { protocol },
            FailureClass::CapabilityMismatch,
            RetryDisposition::TryNextCandidate,
            HealthEffect::Neutral,
            CapabilityEffect::ConfirmUnsupportedProtocol { protocol },
        ),
        ProviderErrorSemanticSignal::RateLimited {
            station_id,
            retry_after_ms,
        } => (
            FailureTarget::StationAccount { station_id },
            FailureClass::RateLimited,
            RetryDisposition::WaitThenReplan,
            HealthEffect::Cooldown { retry_after_ms },
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::BadRequest => (
            FailureTarget::Request,
            FailureClass::BadRequest,
            RetryDisposition::StopRequest,
            HealthEffect::Neutral,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ServerError {
            station_id,
            endpoint_revision,
        } => (
            FailureTarget::StationEndpoint {
                station_id,
                endpoint_revision,
            },
            FailureClass::Upstream5xx,
            RetryDisposition::TryNextCandidate,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::GenericStatus { .. } => (
            FailureTarget::Uncertain,
            FailureClass::Uncertain,
            RetryDisposition::StopRequest,
            HealthEffect::Neutral,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::Transport => (
            FailureTarget::StationEndpoint {
                station_id: String::new(),
                endpoint_revision: 0,
            },
            FailureClass::Transport,
            RetryDisposition::TryNextCandidate,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::Timeout => (
            FailureTarget::StationEndpoint {
                station_id: String::new(),
                endpoint_revision: 0,
            },
            FailureClass::Timeout,
            RetryDisposition::TryNextCandidate,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::MalformedResponse => (
            FailureTarget::ProviderProtocol {
                protocol: ProviderProtocolKind::Unknown,
            },
            FailureClass::MalformedResponse,
            RetryDisposition::StopRequest,
            HealthEffect::Neutral,
            CapabilityEffect::Neutral,
        ),
    };
    CanonicalFailure {
        target,
        class,
        retry,
        health,
        capability,
        public: public_error_for_class(class),
    }
}

pub(crate) fn planning_failure(
    class: FailureClass,
    target: FailureTarget,
    retry: RetryDisposition,
) -> CanonicalFailure {
    CanonicalFailure {
        target,
        class,
        retry,
        health: HealthEffect::Neutral,
        capability: CapabilityEffect::Neutral,
        public: public_error_for_class(class),
    }
}
