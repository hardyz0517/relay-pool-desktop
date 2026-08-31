use http::StatusCode;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.failure-target; owner=application/request_finalization; remove_when=canonical failure mapping drops downstream/protocol target scopes"
    )
)]
pub(crate) enum FailureTarget {
    Request,
    /// The failure is attributable to the Key selected for this attempt, but
    /// does not justify a broader credential, account, endpoint, or provider
    /// health verdict.
    CurrentKey,
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
    StationGroup {
        station_id: String,
        group_binding_id: String,
    },
    StationEndpoint {
        station_id: String,
        endpoint_revision: i64,
    },
    ProviderCapacity {
        domain_commitment: String,
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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.provider-protocol; owner=application/request_finalization; remove_when=provider error mapping drops non-emitted protocol variants"
    )
)]
pub(crate) enum ProviderProtocolKind {
    OpenAiChatCompletions,
    OpenAiResponses,
    OpenAiCompatible,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.local-adapter-component; owner=application/request_finalization; remove_when=failure attribution drops request/lifecycle components"
    )
)]
pub(crate) enum LocalAdapterComponent {
    RequestBody,
    ResponseTransform,
    Lifecycle,
    Invariant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.failure-class; owner=application/request_finalization; remove_when=canonical outcome taxonomy removes non-constructed failure classes"
    )
)]
pub(crate) enum FailureClass {
    ConfigRequired,
    PolicyRejected,
    EconomicsUnavailable,
    HealthUnavailable,
    Authentication,
    InsufficientBalance,
    RateLimited,
    QuotaExhausted,
    RuntimeConcurrencyLimited,
    ProviderCapacity,
    RelayServiceUnavailable,
    ModelUnavailable,
    CapabilityMismatch,
    BadRequest,
    ProviderRejectedRequest,
    Timeout,
    Transport,
    Upstream5xx,
    UpstreamOverloaded,
    MalformedResponse,
    StreamInterrupted,
    DownstreamDrop,
    NoAvailableKey,
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
    TryNextKey,
    StopRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceConfidence {
    Confirmed,
    Probable,
    Unknown,
    Conflicting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RequestAcceptance {
    RejectedBeforeAcceptance,
    AcceptedOrMayHaveBeenAccepted,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplaySafety {
    ReplaySafe,
    RequiresProviderIdempotency,
    NotReplayable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BillingState {
    BillingUncertain,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.health-effect; owner=application/request_finalization; remove_when=finalization effects drop reserved health transitions"
    )
)]
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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.capability-applicability; owner=application/request_finalization; remove_when=learning contract drops broader applicability states"
    )
)]
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
    pub(crate) confidence: EvidenceConfidence,
    pub(crate) request_acceptance: RequestAcceptance,
    pub(crate) replay_safety: ReplaySafety,
    pub(crate) billing: BillingState,
    pub(crate) classifier_profile_version: &'static str,
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
    UpstreamRequestRejected,
    Timeout,
    TransportFailure,
    UpstreamUnavailable,
    UpstreamOverloaded,
    MalformedResponse,
    StreamInterrupted,
    DownstreamDisconnected,
    NoAvailableKey,
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
            Self::ConfigRequired => "routing_configuration_required",
            Self::PolicyRejected => "route_policy_rejected",
            Self::EconomicsUnavailable => "route_economics_unavailable",
            Self::HealthUnavailable => "route_health_unavailable",
            Self::AuthenticationFailed => "upstream_authentication_failed",
            Self::InsufficientBalance => "upstream_insufficient_balance",
            Self::RateLimited => "upstream_rate_limited",
            Self::ModelUnavailable => "upstream_model_unavailable",
            Self::CapabilityMismatch => "upstream_capability_mismatch",
            Self::BadRequest => "request_bad_request",
            Self::UpstreamRequestRejected => "upstream_request_rejected",
            Self::Timeout => "upstream_timeout",
            Self::TransportFailure => "upstream_transport_failure",
            Self::UpstreamUnavailable => "upstream_unavailable",
            Self::UpstreamOverloaded => "upstream_overloaded",
            Self::MalformedResponse => "upstream_malformed_response",
            Self::StreamInterrupted => "upstream_stream_interrupted",
            Self::DownstreamDisconnected => "downstream_disconnected",
            Self::NoAvailableKey => "no_available_key",
            Self::CapacityExhausted => "route_capacity_exhausted",
            Self::CandidateLimitExceeded => "route_candidate_limit_exceeded",
            Self::FactsUnavailable => "route_facts_unavailable",
            Self::ConfigUnstable => "route_configuration_changed",
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
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "routing configuration is required",
        },
        FailureClass::PolicyRejected => PublicError {
            code: PublicErrorCode::PolicyRejected,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
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
        FailureClass::QuotaExhausted => PublicError {
            code: PublicErrorCode::RateLimited,
            http_status: StatusCode::TOO_MANY_REQUESTS,
            message: "upstream quota is exhausted",
        },
        FailureClass::RuntimeConcurrencyLimited => PublicError {
            code: PublicErrorCode::UpstreamOverloaded,
            http_status: StatusCode::TOO_MANY_REQUESTS,
            message: "upstream concurrency is limited",
        },
        FailureClass::ProviderCapacity => PublicError {
            code: PublicErrorCode::UpstreamOverloaded,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "upstream server is temporarily overloaded",
        },
        FailureClass::RelayServiceUnavailable => PublicError {
            code: PublicErrorCode::UpstreamUnavailable,
            http_status: StatusCode::BAD_GATEWAY,
            message: "upstream relay service is unavailable",
        },
        FailureClass::ModelUnavailable => PublicError {
            code: PublicErrorCode::ModelUnavailable,
            http_status: StatusCode::NOT_FOUND,
            message: "upstream model was not found",
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
        FailureClass::ProviderRejectedRequest => PublicError {
            code: PublicErrorCode::UpstreamRequestRejected,
            http_status: StatusCode::BAD_REQUEST,
            message: "upstream rejected the request",
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
        FailureClass::UpstreamOverloaded => PublicError {
            code: PublicErrorCode::UpstreamOverloaded,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "upstream is overloaded",
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
        FailureClass::NoAvailableKey => PublicError {
            code: PublicErrorCode::NoAvailableKey,
            http_status: StatusCode::SERVICE_UNAVAILABLE,
            message: "no available key",
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
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=request-finalization.provider-semantic-signal; owner=application/request_finalization; remove_when=canonical failure mapping drops transport/timeout signal classes"
    )
)]
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
    ConfirmedGroupSubscriptionInvalid {
        station_id: String,
        group_binding_id: String,
    },
    ConfirmedCapabilityMismatch {
        protocol: ProviderProtocolKind,
    },
    RateLimited {
        station_id: String,
        retry_after_ms: Option<i64>,
    },
    BadRequest,
    Overloaded,
    ProviderCapacity {
        domain_commitment: String,
        retry_after_ms: Option<i64>,
    },
    ServerError {
        station_id: String,
        endpoint_revision: i64,
    },
    GenericStatus {
        status: u16,
        confidence: EvidenceConfidence,
    },
    Transport {
        station_id: String,
        endpoint_revision: i64,
    },
    Timeout {
        station_id: String,
        endpoint_revision: i64,
    },
    MalformedResponse,
}

pub(crate) fn failure_from_provider_signal(
    signal: ProviderErrorSemanticSignal,
    applicability: CapabilityApplicabilitySet,
) -> CanonicalFailure {
    let mut confidence = EvidenceConfidence::Confirmed;
    let (target, class, retry, health, capability) = match signal {
        ProviderErrorSemanticSignal::ConfirmedAuthentication { station_key_id } => (
            FailureTarget::StationKeyCredential { station_key_id },
            FailureClass::Authentication,
            RetryDisposition::TryNextKey,
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
            // A confirmed balance exhaustion is the only provider business
            // error that must stop immediately. Retrying it on the same key
            // or silently failing over would only repeat a deterministic
            // account-level rejection.
            RetryDisposition::StopRequest,
            HealthEffect::HardFail,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ConfirmedGroupSubscriptionInvalid {
            station_id,
            group_binding_id,
        } => (
            FailureTarget::StationGroup {
                station_id,
                group_binding_id,
            },
            FailureClass::PolicyRejected,
            RetryDisposition::TryNextKey,
            HealthEffect::HardFail,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ConfirmedCapabilityMismatch { protocol } => (
            FailureTarget::ProviderProtocol { protocol },
            FailureClass::CapabilityMismatch,
            RetryDisposition::TryNextKey,
            HealthEffect::Neutral,
            CapabilityEffect::ConfirmUnsupportedProtocol { protocol },
        ),
        ProviderErrorSemanticSignal::RateLimited {
            station_id: _,
            retry_after_ms: _,
        } => (
            FailureTarget::CurrentKey,
            FailureClass::RateLimited,
            // 429 is an ordinary retryable failure. The execution layer first
            // retries the same key until its circuit threshold is reached,
            // then consumes one distinct-key failover slot.
            RetryDisposition::TryNextKey,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::BadRequest => (
            FailureTarget::Request,
            FailureClass::ProviderRejectedRequest,
            RetryDisposition::TryNextKey,
            HealthEffect::Neutral,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::Overloaded => (
            FailureTarget::CurrentKey,
            FailureClass::UpstreamOverloaded,
            RetryDisposition::TryNextKey,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ProviderCapacity {
            domain_commitment: _,
            retry_after_ms: _,
        } => (
            FailureTarget::CurrentKey,
            FailureClass::ProviderCapacity,
            // Capacity-domain routing is intentionally out of the v3
            // production policy. Treat this as a normal failure of the
            // selected key and let the single request retry loop choose the
            // next ranked candidate.
            RetryDisposition::TryNextKey,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::ServerError {
            station_id: _,
            endpoint_revision: _,
        } => (
            FailureTarget::CurrentKey,
            FailureClass::Upstream5xx,
            RetryDisposition::TryNextKey,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::GenericStatus {
            confidence: signal_confidence,
            ..
        } => {
            confidence = signal_confidence;
            (
                FailureTarget::Uncertain,
                FailureClass::Uncertain,
                RetryDisposition::TryNextKey,
                HealthEffect::Neutral,
                CapabilityEffect::Neutral,
            )
        }
        ProviderErrorSemanticSignal::Transport {
            station_id: _,
            endpoint_revision: _,
        } => (
            FailureTarget::CurrentKey,
            FailureClass::Transport,
            RetryDisposition::TryNextKey,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::Timeout {
            station_id: _,
            endpoint_revision: _,
        } => (
            FailureTarget::CurrentKey,
            FailureClass::Timeout,
            RetryDisposition::TryNextKey,
            HealthEffect::ObserveFailure,
            CapabilityEffect::Neutral,
        ),
        ProviderErrorSemanticSignal::MalformedResponse => (
            FailureTarget::ProviderProtocol {
                protocol: ProviderProtocolKind::Unknown,
            },
            FailureClass::MalformedResponse,
            RetryDisposition::TryNextKey,
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
        confidence,
        request_acceptance: default_request_acceptance(class),
        replay_safety: default_replay_safety(class),
        billing: BillingState::BillingUncertain,
        classifier_profile_version: "canonical-failure-v1",
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
        confidence: EvidenceConfidence::Confirmed,
        request_acceptance: default_request_acceptance(class),
        replay_safety: default_replay_safety(class),
        billing: BillingState::BillingUncertain,
        classifier_profile_version: "canonical-failure-v1",
    }
}

fn default_request_acceptance(class: FailureClass) -> RequestAcceptance {
    match class {
        FailureClass::ProviderCapacity
        | FailureClass::UpstreamOverloaded
        | FailureClass::ProviderRejectedRequest
        | FailureClass::PolicyRejected
        | FailureClass::Authentication
        | FailureClass::InsufficientBalance
        | FailureClass::RateLimited
        | FailureClass::QuotaExhausted
        | FailureClass::RuntimeConcurrencyLimited
        | FailureClass::ModelUnavailable
        | FailureClass::CapabilityMismatch
        | FailureClass::Uncertain
        // Planning/admission deadlines are exhausted before downstream
        // output is committed. Keep them out of the PossiblyAccepted bucket
        // so lifecycle and health evidence remain replay-safe.
        | FailureClass::Deadline => RequestAcceptance::RejectedBeforeAcceptance,
        FailureClass::Upstream5xx
        | FailureClass::Transport
        | FailureClass::Timeout
        | FailureClass::MalformedResponse
        | FailureClass::StreamInterrupted => RequestAcceptance::AcceptedOrMayHaveBeenAccepted,
        _ => RequestAcceptance::Unknown,
    }
}

fn default_replay_safety(class: FailureClass) -> ReplaySafety {
    match class {
        FailureClass::ProviderCapacity
        | FailureClass::UpstreamOverloaded
        | FailureClass::ProviderRejectedRequest
        | FailureClass::PolicyRejected
        | FailureClass::Authentication
        | FailureClass::InsufficientBalance
        | FailureClass::RateLimited
        | FailureClass::QuotaExhausted
        | FailureClass::RuntimeConcurrencyLimited
        | FailureClass::ModelUnavailable
        | FailureClass::CapabilityMismatch
        | FailureClass::Uncertain => ReplaySafety::ReplaySafe,
        FailureClass::Transport
        | FailureClass::Timeout
        | FailureClass::Upstream5xx
        | FailureClass::MalformedResponse
        | FailureClass::StreamInterrupted => ReplaySafety::RequiresProviderIdempotency,
        _ => ReplaySafety::NotReplayable,
    }
}
