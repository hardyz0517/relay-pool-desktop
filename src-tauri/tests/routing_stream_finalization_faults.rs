#![allow(dead_code, unfulfilled_lint_expectations)]

use bytes::Bytes;
use http::{HeaderMap, StatusCode};

mod application {
    #[path = "../../src/application/request_lifecycle/mod.rs"]
    pub(crate) mod request_lifecycle;
}

mod services {
    pub(crate) mod proxy {
        #[path = "../../../src/services/proxy/protocol/mod.rs"]
        pub(crate) mod protocol;
    }
}

use application::request_lifecycle::{
    attempt::{
        AttemptContext, AttemptFailureKind, AttemptLifecycle, AttemptTerminal,
        ClassifiedAttemptFailure, FailureBlame, HealthEffect, RetryDisposition,
    },
    delivery::DeliveryTerminal,
    request::{
        AttemptId, PendingFinalRequestRecord, RequestContextSnapshot, RequestLogAnnotations,
        RequestTerminal,
    },
};
use services::proxy::protocol::{
    responses_sse::ResponsesSseMachine, ProtocolMachine, ProtocolProgress, ProtocolTerminal,
};

#[test]
fn incomplete_stream_eof_is_attempt_failure_and_not_request_success() {
    let mut protocol = ResponsesSseMachine::new();
    protocol
        .observe_headers(StatusCode::OK, &HeaderMap::new())
        .expect("headers");
    assert_eq!(
        protocol
            .observe_chunk(&Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\"}\n\n"
            ))
            .expect("delta"),
        ProtocolProgress::Observed
    );
    assert_eq!(
        protocol.finish_eof().expect("eof"),
        ProtocolTerminal::Incomplete
    );

    let context = request_context("stream-incomplete");
    let mut attempt = committed_attempt(&context);
    let terminal = attempt
        .terminalize(AttemptTerminal::Failed(classified_failure(
            AttemptFailureKind::MalformedResponse,
            FailureBlame::Upstream,
            RetryDisposition::StopRequest,
            HealthEffect::ObserveFailure,
            "upstream_stream_incomplete",
        )))
        .expect("attempt terminal");
    let attempt_record = attempt
        .terminal_record(true, context.received_at_ms + 10)
        .expect("attempt record");
    assert!(matches!(terminal, AttemptTerminal::Failed(_)));
    assert!(attempt_record.output_committed);

    let request = pending_record(context).fail(
        "upstream_stream_incomplete",
        Some("protocol terminal was not observed before EOF".to_string()),
        DeliveryTerminal::BodyCompleted,
    );
    assert!(matches!(
        request.terminal.terminal,
        RequestTerminal::Failed(_)
    ));
    assert!(
        !matches!(request.terminal.terminal, RequestTerminal::Completed(_)),
        "transport EOF without protocol terminal must not become request success"
    );
}

#[test]
fn downstream_drop_after_commit_records_failed_attempt_before_interrupted_request() {
    let context = request_context("stream-downstream-drop");
    let mut attempt = committed_attempt(&context);
    attempt
        .terminalize(AttemptTerminal::Failed(classified_failure(
            AttemptFailureKind::DownstreamDrop,
            FailureBlame::Downstream,
            RetryDisposition::StopRequest,
            HealthEffect::Neutral,
            "downstream_disconnected",
        )))
        .expect("attempt terminal");
    let attempt_record = attempt
        .terminal_record(true, context.received_at_ms + 10)
        .expect("attempt record");
    assert!(matches!(
        attempt_record.terminal,
        AttemptTerminal::Failed(ref failure)
            if failure.kind == AttemptFailureKind::DownstreamDrop
                && failure.blame == FailureBlame::Downstream
    ));
    assert!(
        attempt_record.output_committed,
        "downstream drop after a delivered chunk remains post-commit"
    );

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

fn committed_attempt(context: &RequestContextSnapshot) -> AttemptLifecycle {
    let mut attempt = AttemptLifecycle::new(AttemptContext {
        attempt_id: AttemptId::new(context.request_id.clone(), 0),
        station_id: "station-test".to_string(),
        station_key_id: "key-test".to_string(),
        endpoint_revision: 1,
        started_at_ms: context.received_at_ms,
    });
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

fn pending_record(context: RequestContextSnapshot) -> PendingFinalRequestRecord {
    PendingFinalRequestRecord::new(
        context.clone(),
        Some(AttemptId::new(context.request_id, 0)),
        1,
        0,
        RequestLogAnnotations::default(),
    )
}

fn classified_failure(
    kind: AttemptFailureKind,
    blame: FailureBlame,
    retry: RetryDisposition,
    health: HealthEffect,
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
