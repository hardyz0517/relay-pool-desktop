//! Stable, scenario-level contracts for integration tests.
//!
//! Integration tests call these functions through the real library crate. The
//! private domain types intentionally stay private so tests cannot reconstruct
//! a second module graph with `#[path]` includes.

use bytes::Bytes;
use http::StatusCode;

use crate::{
    application::{
        request_finalization::failure::{
            failure_from_provider_signal, CapabilityApplicabilitySet, CapabilityEffect,
            FailureClass, FailureTarget, HealthEffect, ProviderErrorSemanticSignal,
            RetryDisposition as CanonicalRetryDisposition,
        },
        request_lifecycle::{
            attempt::{
                AttemptContext, AttemptFailureKind, AttemptLifecycle, AttemptTerminal,
                ClassifiedAttemptFailure, FailureBlame, HealthEffect as LifecycleHealthEffect,
                RetryDisposition,
            },
            delivery::DeliveryTerminal,
            request::{
                AttemptId, PendingFinalRequestRecord, RequestContextSnapshot, RequestLifecycle,
                RequestLogAnnotations, RequestTerminal,
            },
        },
        routing_engine::routing_failure::{
            classify_route_failure, RouteFailureInput, RouteFailureKind, RoutePlanningFailure,
        },
    },
    services::proxy::error::{FailureSource, ProxyFailure, ProxyFailureCode},
    services::proxy::protocol::{
        responses_sse::ResponsesSseMachine, ProtocolMachine, ProtocolTerminal,
    },
};

pub fn request_lifecycle_exactly_once() {
    let context = request_context("req-domain");
    let mut lifecycle = RequestLifecycle::new(context);
    let attempt_id = AttemptId::new("req-domain", 0);

    lifecycle.admit().expect("admit");
    lifecycle.start_routing().expect("routing");
    lifecycle.start_attempt(0).expect("attempt");
    lifecycle.commit(attempt_id.clone()).expect("commit");
    lifecycle
        .terminalize(
            RequestTerminal::Completed(
                crate::application::request_lifecycle::request::RequestCompletion {
                    protocol_completed: true,
                    attempt_id: Some(attempt_id.clone()),
                },
            ),
            DeliveryTerminal::BodyCompleted,
        )
        .expect("terminal");

    let terminal = lifecycle.terminal_record().expect("terminal record");
    assert_eq!(terminal.selected_attempt_id, Some(attempt_id.clone()));
    assert!(matches!(
        terminal.terminal.terminal,
        RequestTerminal::Completed(_)
    ));
    assert!(lifecycle
        .terminalize(
            RequestTerminal::Completed(
                crate::application::request_lifecycle::request::RequestCompletion {
                    protocol_completed: true,
                    attempt_id: Some(attempt_id),
                },
            ),
            DeliveryTerminal::BodyCompleted,
        )
        .is_err());
}

pub fn request_lifecycle_rejects_early_commit() {
    let mut lifecycle = RequestLifecycle::new(request_context("req-domain"));
    lifecycle.admit().expect("admit");
    lifecycle.start_routing().expect("routing");
    assert!(lifecycle.commit(AttemptId::new("req-domain", 0)).is_err());
}

pub fn attempt_lifecycle_keeps_retry_and_health_separate() {
    let mut lifecycle = AttemptLifecycle::new(attempt_context("req-domain", 1));
    lifecycle.observe_headers().expect("headers");
    lifecycle.begin_stream().expect("stream");
    lifecycle.commit().expect("commit");
    lifecycle
        .terminalize(AttemptTerminal::Failed(classified_failure(
            AttemptFailureKind::RateLimit,
            FailureBlame::Upstream,
            RetryDisposition::TryNextCandidate,
            LifecycleHealthEffect::Cooldown {
                retry_after_ms: Some(1_000),
            },
            "rate_limited",
        )))
        .expect("terminal");

    let terminal = lifecycle.terminal_record(true, 3).expect("terminal record");
    assert!(terminal.output_committed);
    assert!(matches!(
        terminal.terminal,
        AttemptTerminal::Failed(ClassifiedAttemptFailure {
            retry: RetryDisposition::TryNextCandidate,
            health: LifecycleHealthEffect::Cooldown {
                retry_after_ms: Some(1_000)
            },
            ..
        })
    ));
    assert!(lifecycle.terminalize(AttemptTerminal::Succeeded).is_err());
}

pub fn incomplete_stream_is_not_request_success() {
    let mut protocol = ResponsesSseMachine::new();
    let progress = protocol
        .observe_chunk(&Bytes::from_static(
            b"data: {\"type\":\"response.output_text.delta\"}\n\n",
        ))
        .expect("delta");
    assert_eq!(progress.terminal(), None);
    assert_eq!(
        protocol.finish_eof().expect("eof"),
        ProtocolTerminal::Incomplete
    );

    let context = request_context("stream-incomplete");
    let mut attempt = committed_attempt(&context);
    attempt
        .terminalize(AttemptTerminal::Failed(classified_failure(
            AttemptFailureKind::MalformedResponse,
            FailureBlame::Upstream,
            RetryDisposition::StopRequest,
            LifecycleHealthEffect::ObserveFailure,
            "upstream_stream_incomplete",
        )))
        .expect("attempt terminal");
    assert!(
        attempt
            .terminal_record(true, context.received_at_ms + 10)
            .expect("attempt record")
            .output_committed
    );

    let request = pending_record(context).fail(
        "upstream_stream_incomplete",
        Some("protocol terminal was not observed before EOF".to_string()),
        DeliveryTerminal::BodyCompleted,
    );
    assert!(matches!(
        request.terminal.terminal,
        RequestTerminal::Failed(_)
    ));
}

pub fn downstream_drop_is_interrupted_after_failed_attempt() {
    let context = request_context("stream-downstream-drop");
    let mut attempt = committed_attempt(&context);
    attempt
        .terminalize(AttemptTerminal::Failed(classified_failure(
            AttemptFailureKind::DownstreamDrop,
            FailureBlame::Downstream,
            RetryDisposition::StopRequest,
            LifecycleHealthEffect::Neutral,
            "downstream_disconnected",
        )))
        .expect("attempt terminal");
    let attempt_record = attempt
        .terminal_record(true, context.received_at_ms + 10)
        .expect("attempt record");
    assert!(attempt_record.output_committed);
    assert!(matches!(
        attempt_record.terminal,
        AttemptTerminal::Failed(ref failure)
            if failure.kind == AttemptFailureKind::DownstreamDrop
                && failure.blame == FailureBlame::Downstream
    ));

    let request = pending_record(context).interrupt(
        DeliveryTerminal::DownstreamDropped,
        Some("downstream dropped before upstream EOF".to_string()),
    );
    assert!(matches!(
        request.terminal.terminal,
        RequestTerminal::Interrupted(_)
    ));
    assert_eq!(
        request.terminal.delivery,
        DeliveryTerminal::DownstreamDropped
    );
}

pub fn routing_failure_semantics() {
    for status in [403, 404] {
        let failure = classify_route_failure(RouteFailureInput::http_status(status, false));
        assert_eq!(failure.kind, RouteFailureKind::Uncertain);
        assert!(!failure.retryable_before_output);
    }

    let auth = failure_from_provider_signal(
        ProviderErrorSemanticSignal::ConfirmedAuthentication {
            station_key_id: "key-a".to_string(),
        },
        CapabilityApplicabilitySet::UnknownModelCatalog,
    );
    assert_eq!(auth.class, FailureClass::Authentication);
    assert!(matches!(
        auth.target,
        FailureTarget::StationKeyCredential { .. }
    ));
    assert_eq!(auth.health, HealthEffect::HardFail);

    for applicability in [
        CapabilityApplicabilitySet::UnknownModelCatalog,
        CapabilityApplicabilitySet::PositiveCapabilityEvidence,
        CapabilityApplicabilitySet::LoadEvidenceGap,
    ] {
        let model = failure_from_provider_signal(
            ProviderErrorSemanticSignal::ConfirmedModelNotFound {
                station_key_id: "key-a".to_string(),
                model: "gpt-x".to_string(),
            },
            applicability,
        );
        assert_eq!(model.class, FailureClass::Uncertain);
        assert_eq!(model.capability, CapabilityEffect::Neutral);
    }

    let rate_limited = failure_from_provider_signal(
        ProviderErrorSemanticSignal::RateLimited {
            station_id: "station-a".to_string(),
            retry_after_ms: Some(30_000),
        },
        CapabilityApplicabilitySet::ConfirmedModelCatalog,
    );
    assert_eq!(rate_limited.retry, CanonicalRetryDisposition::TryNextKey);
    assert_eq!(rate_limited.health, HealthEffect::ObserveFailure);

    let rejected = failure_from_provider_signal(
        ProviderErrorSemanticSignal::BadRequest,
        CapabilityApplicabilitySet::UnknownModelCatalog,
    );
    assert_eq!(rejected.public.http_status, StatusCode::BAD_REQUEST);
    assert_eq!(rejected.public.code.as_str(), "upstream_request_rejected");
}

pub fn route_planning_failures_keep_stable_public_mappings() {
    let failures = [
        (
            RoutePlanningFailure::HealthUnavailable,
            "route_health_unavailable",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            RoutePlanningFailure::CapacityExhausted,
            "route_capacity_exhausted",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            RoutePlanningFailure::CandidateLimitExceeded {
                actual: 1_025,
                limit: 1_024,
            },
            "route_candidate_limit_exceeded",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            RoutePlanningFailure::ConfigUnstable,
            "route_configuration_changed",
            StatusCode::SERVICE_UNAVAILABLE,
        ),
        (
            RoutePlanningFailure::DeadlineExceeded,
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

    let invariant = RoutePlanningFailure::InvariantViolation {
        code: "route_invariant_violation",
    };
    assert_eq!(invariant.stable_code(), "route_invariant_violation");
    let proxy = ProxyFailure::from_public_error(invariant.into_canonical().public);
    assert_eq!(proxy.code, ProxyFailureCode::RouteInvariantViolation);
    assert_eq!(proxy.source, FailureSource::Internal);
    assert_eq!(proxy.http_status, StatusCode::INTERNAL_SERVER_ERROR);
}

fn committed_attempt(context: &RequestContextSnapshot) -> AttemptLifecycle {
    let mut attempt = AttemptLifecycle::new(attempt_context(&context.request_id, 0));
    attempt.observe_headers().expect("headers");
    attempt.begin_stream().expect("stream");
    attempt.commit().expect("commit");
    attempt
}

fn request_context(request_id: &str) -> RequestContextSnapshot {
    RequestContextSnapshot {
        request_id: request_id.to_string(),
        method: "POST".to_string(),
        local_path: "/v1/responses".to_string(),
        endpoint: "/v1/responses".to_string(),
        received_at_ms: 1_000,
    }
}

fn attempt_context(request_id: &str, ordinal: u16) -> AttemptContext {
    AttemptContext {
        attempt_id: AttemptId::new(request_id, ordinal),
        station_id: "station-a".to_string(),
        station_key_id: "key-a".to_string(),
        endpoint_revision: 7,
        credential_revision: 1,
        account_revision: 1,
        group_binding_id: None,
        group_revision: None,
        resolved_upstream_model: None,
        comparability_key: None,
        model_alias_revision: 1,
        started_at_ms: 2,
    }
}

fn pending_record(context: RequestContextSnapshot) -> PendingFinalRequestRecord {
    PendingFinalRequestRecord::new(
        context.clone(),
        Some(AttemptId::new(context.request_id.clone(), 0)),
        1,
        0,
        RequestLogAnnotations::default(),
    )
}

fn classified_failure(
    kind: AttemptFailureKind,
    blame: FailureBlame,
    retry: RetryDisposition,
    health: LifecycleHealthEffect,
    public_code: &str,
) -> ClassifiedAttemptFailure {
    ClassifiedAttemptFailure {
        kind,
        blame,
        retry,
        health,
        public_code: public_code.to_string(),
        sanitized_detail: None,
    }
}
