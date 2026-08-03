mod application {
    pub(crate) mod request_finalization {
        pub(crate) mod failure {
            pub(crate) use crate::failure::*;
        }
        pub(crate) mod effect_planner {
            #[allow(unused_imports)]
            pub(crate) use crate::effect_planner::*;
        }
        pub(crate) mod outcome {
            #[allow(unused_imports)]
            pub(crate) use crate::outcome::*;
        }
    }

    pub(crate) mod request_lifecycle {
        pub(crate) mod delivery {
            #[allow(unused_imports)]
            pub(crate) use crate::delivery::*;
        }
        pub(crate) mod request {
            #[allow(unused_imports)]
            pub(crate) use crate::request::*;
        }
        pub(crate) mod attempt {
            pub(crate) use crate::attempt::*;
        }
    }
}

mod services {
    pub(crate) mod secrets {
        pub(crate) mod mask {
            pub(crate) fn redact_text(value: &str) -> String {
                value.replace("sk-contract-secret", "[REDACTED]")
            }
        }
    }
}

#[path = "../src/application/request_lifecycle/attempt.rs"]
mod attempt;
#[path = "../src/application/request_lifecycle/delivery.rs"]
mod delivery;
#[path = "../src/application/request_finalization/effect_planner.rs"]
mod effect_planner;
#[path = "../src/application/request_finalization/failure.rs"]
mod failure;
#[path = "../src/application/request_finalization/outcome.rs"]
mod outcome;
#[path = "../src/services/proxy/error.rs"]
mod proxy_error;
#[path = "../src/application/request_lifecycle/request.rs"]
mod request;
#[path = "../src/application/routing_engine/routing_failure.rs"]
mod routing_failure;

use attempt::{AttemptFailureKind, FailureBlame, HealthEffect as LifecycleHealthEffect};
use effect_planner::{classified_attempt_failure_from_canonical, plan_failure_effects};
use failure::{
    failure_from_provider_signal, planning_failure, CapabilityApplicabilitySet, CapabilityEffect,
    FailureClass, FailureTarget, HealthEffect, LocalAdapterComponent, ProviderErrorSemanticSignal,
    ProviderProtocolKind, RetryDisposition,
};
use http::StatusCode;
use proxy_error::{FailureSource, ProxyFailure, ProxyFailureCode};
use routing_failure::{classify_route_failure, RouteFailureInput, RouteFailureKind};

#[test]
fn failure_target_class_effect_types_cover_task_18_contract_surface() {
    let targets = vec![
        FailureTarget::Request,
        FailureTarget::ModelOnKey {
            station_key_id: "key-a".to_string(),
            model: "gpt-x".to_string(),
        },
        FailureTarget::StationKeyCredential {
            station_key_id: "key-a".to_string(),
        },
        FailureTarget::StationAccount {
            station_id: "station-a".to_string(),
        },
        FailureTarget::StationEndpoint {
            station_id: "station-a".to_string(),
            endpoint_revision: 7,
        },
        FailureTarget::ProviderProtocol {
            protocol: ProviderProtocolKind::OpenAiResponses,
        },
        FailureTarget::LocalAdapter {
            component: LocalAdapterComponent::ResponseTransform,
        },
        FailureTarget::Downstream,
        FailureTarget::Uncertain,
    ];
    assert_eq!(targets.len(), 9);

    let classes = all_failure_classes();
    let public_codes = classes
        .iter()
        .map(|class| failure::public_error_for_class(*class).code.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        public_codes.len(),
        classes.len(),
        "each canonical class has a stable public code"
    );
}

#[test]
fn generic_403_and_404_are_uncertain_neutral_without_adapter_signal() {
    for status in [403, 404] {
        let failure = classify_route_failure(RouteFailureInput::http_status(status, false));
        assert_eq!(failure.kind, RouteFailureKind::Uncertain);
        assert!(!failure.retryable_before_output);
    }
}

#[test]
fn adapter_confirmed_auth_and_model_not_found_are_typed_not_string_derived() {
    let auth = failure_from_provider_signal(
        ProviderErrorSemanticSignal::ConfirmedAuthentication {
            station_key_id: "key-a".to_string(),
        },
        CapabilityApplicabilitySet::UnknownModelCatalog,
    );
    assert_eq!(
        auth.target,
        FailureTarget::StationKeyCredential {
            station_key_id: "key-a".to_string()
        }
    );
    assert_eq!(auth.class, FailureClass::Authentication);
    assert_eq!(auth.health, HealthEffect::HardFail);
    assert_eq!(auth.capability, CapabilityEffect::Neutral);

    let model = failure_from_provider_signal(
        ProviderErrorSemanticSignal::ConfirmedModelNotFound {
            station_key_id: "key-a".to_string(),
            model: "gpt-x".to_string(),
        },
        CapabilityApplicabilitySet::ConfirmedModelCatalog,
    );
    assert_eq!(model.class, FailureClass::ModelUnavailable);
    assert_eq!(model.health, HealthEffect::Neutral);
    assert!(matches!(
        model.capability,
        CapabilityEffect::ConfirmUnsupportedModel { .. }
    ));
}

#[test]
fn capability_applicability_blocks_model_404_learning_for_unknown_positive_or_gap() {
    for applicability in [
        CapabilityApplicabilitySet::UnknownModelCatalog,
        CapabilityApplicabilitySet::PositiveCapabilityEvidence,
        CapabilityApplicabilitySet::LoadEvidenceGap,
    ] {
        let failure = failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedModelNotFound {
                station_key_id: "key-a".to_string(),
                model: "gpt-x".to_string(),
            },
            applicability,
        );
        assert_eq!(failure.class, FailureClass::Uncertain);
        assert_eq!(failure.health, HealthEffect::Neutral);
        assert_eq!(failure.capability, CapabilityEffect::Neutral);
    }
}

#[test]
fn effect_planner_keeps_retry_health_and_capability_axes_separate() {
    let failure = failure_from_provider_signal(
        ProviderErrorSemanticSignal::RateLimited {
            station_id: "station-a".to_string(),
            retry_after_ms: Some(30_000),
        },
        CapabilityApplicabilitySet::ConfirmedModelCatalog,
    );
    let effects = plan_failure_effects(&failure);

    assert_eq!(effects.retry, RetryDisposition::WaitThenReplan);
    assert_eq!(
        effects.health,
        HealthEffect::Cooldown {
            retry_after_ms: Some(30_000)
        }
    );
    assert_eq!(effects.capability, CapabilityEffect::Neutral);

    let attempt = classified_attempt_failure_from_canonical(&failure);
    assert_eq!(attempt.kind, AttemptFailureKind::RateLimit);
    assert_eq!(attempt.blame, FailureBlame::Upstream);
    assert!(matches!(
        attempt.health,
        LifecycleHealthEffect::Cooldown {
            retry_after_ms: Some(30_000)
        }
    ));
}

#[test]
fn route_planning_failures_have_stable_codes_and_public_proxy_mapping() {
    let failures = [
        (
            routing_failure::RoutePlanningFailure::HealthUnavailable,
            "route_health_unavailable",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            routing_failure::RoutePlanningFailure::CapacityExhausted,
            "route_capacity_exhausted",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            routing_failure::RoutePlanningFailure::CandidateLimitExceeded {
                actual: 1025,
                limit: 1024,
            },
            "route_candidate_limit_exceeded",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            routing_failure::RoutePlanningFailure::ConfigUnstable,
            "route_configuration_changed",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            routing_failure::RoutePlanningFailure::DeadlineExceeded,
            "route_deadline_exceeded",
            StatusCode::GATEWAY_TIMEOUT,
        ),
    ];
    for (failure, stable_code, http_status) in failures {
        assert_eq!(failure.stable_code(), stable_code);
        let canonical = failure.into_canonical();
        let proxy = ProxyFailure::from_public_error(canonical.public.clone());
        assert_eq!(proxy.code.as_str(), canonical.public.code.as_str());
        assert_eq!(proxy.http_status, http_status);
    }

    let invariant = planning_failure(
        FailureClass::Invariant,
        FailureTarget::LocalAdapter {
            component: LocalAdapterComponent::Invariant,
        },
        RetryDisposition::StopRequest,
    );
    let proxy = ProxyFailure::from_public_error(invariant.public);
    assert_eq!(proxy.code, ProxyFailureCode::RouteInvariantViolation);
    assert_eq!(proxy.source, FailureSource::Internal);
    assert_eq!(proxy.http_status, StatusCode::INTERNAL_SERVER_ERROR);
}

fn all_failure_classes() -> Vec<FailureClass> {
    vec![
        FailureClass::ConfigRequired,
        FailureClass::PolicyRejected,
        FailureClass::EconomicsUnavailable,
        FailureClass::HealthUnavailable,
        FailureClass::Authentication,
        FailureClass::InsufficientBalance,
        FailureClass::RateLimited,
        FailureClass::ModelUnavailable,
        FailureClass::CapabilityMismatch,
        FailureClass::BadRequest,
        FailureClass::Timeout,
        FailureClass::Transport,
        FailureClass::Upstream5xx,
        FailureClass::MalformedResponse,
        FailureClass::StreamInterrupted,
        FailureClass::DownstreamDrop,
        FailureClass::CapacityExhausted,
        FailureClass::CandidateLimit,
        FailureClass::FactsUnavailable,
        FailureClass::ConfigUnstable,
        FailureClass::Lifecycle,
        FailureClass::Deadline,
        FailureClass::Invariant,
        FailureClass::Uncertain,
    ]
}
