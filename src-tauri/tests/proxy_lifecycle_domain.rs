mod lifecycle {
    #[path = "../../src/services/proxy/lifecycle/attempt.rs"]
    pub mod attempt;
    #[path = "../../src/services/proxy/lifecycle/delivery.rs"]
    pub mod delivery;
    #[path = "../../src/services/proxy/lifecycle/request.rs"]
    pub mod request;
}

use lifecycle::{
    attempt::{
        AttemptContext, AttemptFailureKind, AttemptLifecycle, AttemptTerminal,
        ClassifiedAttemptFailure, FailureBlame, HealthEffect, RetryDisposition,
    },
    delivery::DeliveryTerminal,
    request::{AttemptId, RequestContextSnapshot, RequestLifecycle, RequestTerminal},
};

fn request_context() -> RequestContextSnapshot {
    RequestContextSnapshot {
        request_id: "req-domain".to_string(),
        method: "POST".to_string(),
        local_path: "/v1/responses".to_string(),
        endpoint: "responses".to_string(),
        received_at_ms: 1,
    }
}

fn attempt_context(ordinal: u16) -> AttemptContext {
    AttemptContext {
        attempt_id: AttemptId::new("req-domain", ordinal),
        station_id: "station-a".to_string(),
        station_key_id: "key-a".to_string(),
        endpoint_revision: 7,
        started_at_ms: 2,
    }
}

#[test]
fn request_lifecycle_allows_one_committed_terminal_record() {
    let mut lifecycle = RequestLifecycle::new(request_context());
    let attempt_id = AttemptId::new("req-domain", 0);

    lifecycle.admit().expect("admit");
    lifecycle.start_routing().expect("routing");
    lifecycle.start_attempt(0).expect("attempt");
    lifecycle.commit(attempt_id.clone()).expect("commit");
    lifecycle
        .terminalize(
            RequestTerminal::Completed(lifecycle::request::RequestCompletion {
                protocol_completed: true,
                attempt_id: Some(attempt_id.clone()),
            }),
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
            RequestTerminal::Completed(lifecycle::request::RequestCompletion {
                protocol_completed: true,
                attempt_id: Some(attempt_id),
            }),
            DeliveryTerminal::BodyCompleted,
        )
        .is_err());
}

#[test]
fn request_lifecycle_rejects_commit_before_attempt_start() {
    let mut lifecycle = RequestLifecycle::new(request_context());
    lifecycle.admit().expect("admit");
    lifecycle.start_routing().expect("routing");

    assert!(lifecycle.commit(AttemptId::new("req-domain", 0)).is_err());
}

#[test]
fn attempt_lifecycle_separates_retry_from_health_effect() {
    let mut lifecycle = AttemptLifecycle::new(attempt_context(1));
    lifecycle.observe_headers().expect("headers");
    lifecycle.begin_stream().expect("stream");
    lifecycle.commit().expect("commit");
    lifecycle
        .terminalize(AttemptTerminal::Failed(ClassifiedAttemptFailure {
            kind: AttemptFailureKind::RateLimit,
            blame: FailureBlame::Upstream,
            retry: RetryDisposition::TryNextCandidate,
            health: HealthEffect::Cooldown {
                retry_after_ms: Some(1_000),
            },
            public_code: "rate_limited".to_string(),
            sanitized_detail: Some("retry later".to_string()),
        }))
        .expect("terminal");

    let terminal = lifecycle.terminal_record(true, 3).expect("terminal record");
    assert!(terminal.output_committed);
    assert!(matches!(
        terminal.terminal,
        AttemptTerminal::Failed(ClassifiedAttemptFailure {
            retry: RetryDisposition::TryNextCandidate,
            health: HealthEffect::Cooldown {
                retry_after_ms: Some(1_000)
            },
            ..
        })
    ));
    assert!(lifecycle.terminalize(AttemptTerminal::Succeeded).is_err());
}
