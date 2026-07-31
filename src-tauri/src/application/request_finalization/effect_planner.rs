#![allow(dead_code)]

use crate::application::{
    request_finalization::failure::{
        CanonicalFailure, CapabilityEffect, FailureClass, FailureTarget, HealthEffect,
        PublicErrorCode, RetryDisposition,
    },
    request_finalization::outcome::{AttemptOutcome, UpstreamProtocolOutcome},
    request_lifecycle::{
        attempt::{
            AttemptFailureKind, ClassifiedAttemptFailure, FailureBlame,
            HealthEffect as LifecycleHealthEffect, RetryDisposition as LifecycleRetryDisposition,
        },
        request::AttemptId,
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
    ClassifiedAttemptFailure {
        kind: attempt_kind_for_class(failure.class),
        blame: blame_for_target(&failure.target),
        retry: lifecycle_retry(failure.retry),
        health: lifecycle_health(failure.health),
        public_code: failure.public.code.as_str().to_string(),
        sanitized_detail: Some(failure.public.message.to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptEffectPlan {
    pub(crate) attempt_id: AttemptId,
    pub(crate) failure: Option<FailureEffectPlan>,
    pub(crate) output_committed: bool,
}

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
        FailureClass::RateLimited => AttemptFailureKind::RateLimit,
        FailureClass::Transport => AttemptFailureKind::Connect,
        FailureClass::Timeout | FailureClass::Deadline => AttemptFailureKind::Timeout,
        FailureClass::CapabilityMismatch
        | FailureClass::ModelUnavailable
        | FailureClass::PolicyRejected
        | FailureClass::EconomicsUnavailable
        | FailureClass::HealthUnavailable
        | FailureClass::CandidateLimit => AttemptFailureKind::CapabilityMismatch,
        FailureClass::BadRequest | FailureClass::ConfigRequired => AttemptFailureKind::BadRequest,
        FailureClass::MalformedResponse => AttemptFailureKind::MalformedResponse,
        FailureClass::StreamInterrupted => AttemptFailureKind::StreamInterrupted,
        FailureClass::DownstreamDrop => AttemptFailureKind::DownstreamDrop,
        FailureClass::Upstream5xx
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
        FailureTarget::ModelOnKey { .. }
        | FailureTarget::StationKeyCredential { .. }
        | FailureTarget::StationAccount { .. }
        | FailureTarget::StationEndpoint { .. }
        | FailureTarget::ProviderProtocol { .. } => FailureBlame::Upstream,
    }
}

fn lifecycle_retry(retry: RetryDisposition) -> LifecycleRetryDisposition {
    match retry {
        RetryDisposition::TryNextCandidate | RetryDisposition::WaitThenReplan => {
            LifecycleRetryDisposition::TryNextCandidate
        }
        RetryDisposition::StopRequest => LifecycleRetryDisposition::StopRequest,
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
