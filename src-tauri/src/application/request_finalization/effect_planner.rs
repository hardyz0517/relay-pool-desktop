#[cfg(test)]
use crate::application::request_finalization::outcome::{AttemptOutcome, UpstreamProtocolOutcome};
#[cfg(test)]
use crate::application::request_lifecycle::request::AttemptId;
use crate::application::{
    request_finalization::failure::{
        CanonicalFailure, CapabilityEffect, FailureClass, FailureTarget, HealthEffect,
        PublicErrorCode, RetryDisposition,
    },
    request_lifecycle::attempt::{
        project_retry_disposition, AttemptFailureKind, ClassifiedAttemptFailure,
        DurableCapabilityEffect, FailureBlame, HealthEffect as LifecycleHealthEffect,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FailureEffectPlan {
    pub(crate) target: FailureTarget,
    pub(crate) class: FailureClass,
    pub(crate) retry: RetryDisposition,
    pub(crate) health: HealthEffect,
    pub(crate) capability: CapabilityEffect,
    pub(crate) public_code: PublicErrorCode,
}

pub(crate) fn plan_failure_effects(failure: &CanonicalFailure) -> FailureEffectPlan {
    FailureEffectPlan {
        target: failure.target.clone(),
        class: failure.class,
        retry: failure.retry,
        health: failure.health,
        capability: failure.capability.clone(),
        public_code: failure.public.code,
    }
}

pub(crate) fn classified_attempt_failure_from_canonical(
    failure: &CanonicalFailure,
) -> ClassifiedAttemptFailure {
    plan_failure_effects(failure).into_classified_attempt_failure(failure)
}

impl FailureEffectPlan {
    fn into_classified_attempt_failure(
        self,
        failure: &CanonicalFailure,
    ) -> ClassifiedAttemptFailure {
        let evidence_code = self.public_code.as_str().to_string();
        let durable_capability = match &self.capability {
            CapabilityEffect::ConfirmUnsupportedModel {
                station_key_id,
                model,
            } => Some(DurableCapabilityEffect::ConfirmUnsupportedModel {
                station_key_id: station_key_id.clone(),
                model: model.clone(),
                evidence_code: evidence_code.clone(),
                classifier_profile_version: failure.classifier_profile_version.to_string(),
            }),
            CapabilityEffect::Neutral | CapabilityEffect::ConfirmUnsupportedProtocol { .. } => None,
        };
        ClassifiedAttemptFailure {
            kind: attempt_kind_for_class(self.class),
            blame: blame_for_target(&self.target),
            retry: project_retry_disposition(self.retry),
            health: durable_capability
                .map(LifecycleHealthEffect::Capability)
                .unwrap_or_else(|| {
                    if matches!(
                        self.class,
                        FailureClass::ProviderCapacity | FailureClass::RuntimeConcurrencyLimited
                    ) {
                        LifecycleHealthEffect::Neutral
                    } else {
                        lifecycle_health(self.health)
                    }
                }),
            public_code: evidence_code,
            sanitized_detail: Some(failure.public.message.to_string()),
        }
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptEffectPlan {
    pub(crate) attempt_id: AttemptId,
    pub(crate) failure: Option<FailureEffectPlan>,
    pub(crate) output_committed: bool,
}

#[cfg(test)]
pub(crate) fn plan_attempt_outcome_effects(outcome: &AttemptOutcome) -> AttemptEffectPlan {
    match &outcome.protocol {
        UpstreamProtocolOutcome::Succeeded { output_committed } => AttemptEffectPlan {
            attempt_id: outcome.attempt_id.clone(),
            failure: None,
            output_committed: *output_committed,
        },
        UpstreamProtocolOutcome::Failed {
            failure,
            output_committed,
        } => AttemptEffectPlan {
            attempt_id: outcome.attempt_id.clone(),
            failure: Some(plan_failure_effects(failure)),
            output_committed: *output_committed,
        },
        UpstreamProtocolOutcome::NotStarted { .. }
        | UpstreamProtocolOutcome::Interrupted { .. } => AttemptEffectPlan {
            attempt_id: outcome.attempt_id.clone(),
            failure: None,
            output_committed: false,
        },
    }
}

fn attempt_kind_for_class(class: FailureClass) -> AttemptFailureKind {
    match class {
        FailureClass::Authentication => AttemptFailureKind::Authentication,
        FailureClass::InsufficientBalance => AttemptFailureKind::Balance,
        FailureClass::RateLimited | FailureClass::QuotaExhausted => AttemptFailureKind::RateLimit,
        FailureClass::Transport => AttemptFailureKind::Connect,
        FailureClass::Timeout | FailureClass::Deadline => AttemptFailureKind::Timeout,
        FailureClass::CapabilityMismatch
        | FailureClass::ModelUnavailable
        | FailureClass::PolicyRejected
        | FailureClass::EconomicsUnavailable
        | FailureClass::HealthUnavailable
        | FailureClass::CandidateLimit => AttemptFailureKind::CapabilityMismatch,
        FailureClass::BadRequest
        | FailureClass::ProviderRejectedRequest
        | FailureClass::ConfigRequired => AttemptFailureKind::BadRequest,
        FailureClass::MalformedResponse => AttemptFailureKind::MalformedResponse,
        FailureClass::StreamInterrupted => AttemptFailureKind::StreamInterrupted,
        FailureClass::DownstreamDrop => AttemptFailureKind::DownstreamDrop,
        FailureClass::NoAvailableKey => AttemptFailureKind::LocalAdapter,
        FailureClass::Upstream5xx
        | FailureClass::UpstreamOverloaded
        | FailureClass::ProviderCapacity
        | FailureClass::RuntimeConcurrencyLimited
        | FailureClass::RelayServiceUnavailable
        | FailureClass::CapacityExhausted
        | FailureClass::FactsUnavailable
        | FailureClass::ConfigUnstable
        | FailureClass::Lifecycle
        | FailureClass::Invariant
        | FailureClass::Uncertain => AttemptFailureKind::HttpStatus,
    }
}

fn blame_for_target(target: &FailureTarget) -> FailureBlame {
    match target {
        FailureTarget::Downstream => FailureBlame::Downstream,
        FailureTarget::LocalAdapter { .. } | FailureTarget::Request | FailureTarget::Uncertain => {
            FailureBlame::LocalAdapter
        }
        FailureTarget::CurrentKey
        | FailureTarget::ModelOnKey { .. }
        | FailureTarget::StationKeyCredential { .. }
        | FailureTarget::StationAccount { .. }
        | FailureTarget::StationGroup { .. }
        | FailureTarget::StationEndpoint { .. }
        | FailureTarget::ProviderProtocol { .. } => FailureBlame::Upstream,
    }
}

fn lifecycle_health(health: HealthEffect) -> LifecycleHealthEffect {
    match health {
        HealthEffect::Success => LifecycleHealthEffect::Success,
        HealthEffect::ObserveFailure => LifecycleHealthEffect::ObserveFailure,
        HealthEffect::Cooldown { retry_after_ms } => {
            LifecycleHealthEffect::Cooldown { retry_after_ms }
        }
        HealthEffect::HardFail => LifecycleHealthEffect::HardFail,
        HealthEffect::Neutral => LifecycleHealthEffect::Neutral,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::request_finalization::failure::{
        failure_from_provider_signal, planning_failure, CapabilityApplicabilitySet,
        ProviderErrorSemanticSignal,
    };

    #[test]
    fn group_subscription_signal_preserves_diagnostics_without_a_scoped_verdict() {
        let failure = failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedGroupSubscriptionInvalid {
                station_id: "station-test".to_string(),
                group_binding_id: "binding-test".to_string(),
            },
            CapabilityApplicabilitySet::ConfirmedModelCatalog,
        );
        let classified = classified_attempt_failure_from_canonical(&failure);
        assert_eq!(
            classified.retry,
            crate::application::request_lifecycle::attempt::RetryDisposition::TryNextCandidate
        );
        assert_eq!(classified.health, LifecycleHealthEffect::HardFail);
        assert_eq!(classified.public_code, "route_policy_rejected");
    }

    #[test]
    fn overloaded_provider_signal_is_an_ordinary_current_key_failure() {
        let failure = failure_from_provider_signal(
            ProviderErrorSemanticSignal::Overloaded,
            CapabilityApplicabilitySet::UnknownModelCatalog,
        );
        let classified = classified_attempt_failure_from_canonical(&failure);
        assert_eq!(classified.blame, FailureBlame::Upstream);
        assert_eq!(
            classified.retry,
            crate::application::request_lifecycle::attempt::RetryDisposition::TryNextCandidate
        );
        assert!(matches!(
            classified.health,
            LifecycleHealthEffect::ObserveFailure
        ));
    }

    #[test]
    fn no_available_key_is_a_local_terminal_failure() {
        let failure = planning_failure(
            FailureClass::NoAvailableKey,
            FailureTarget::Request,
            crate::application::request_finalization::failure::RetryDisposition::StopRequest,
        );

        let classified = classified_attempt_failure_from_canonical(&failure);

        assert_eq!(classified.kind, AttemptFailureKind::LocalAdapter);
        assert_eq!(classified.blame, FailureBlame::LocalAdapter);
        assert_eq!(
            classified.retry,
            crate::application::request_lifecycle::attempt::RetryDisposition::StopRequest
        );
    }
}
