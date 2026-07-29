#[path = "../src/observability/events.rs"]
mod events;
#[path = "../src/observability/metrics.rs"]
mod metrics;
#[path = "../src/observability/redaction.rs"]
mod redaction;

use events::{StructuredEvent, StructuredEventError, StructuredEventKind, StructuredEventResult};
use metrics::{LocalMetricBuffer, MetricEvent, MetricKind, MetricLabel, MetricOutcome};
use redaction::{redact_text_preview, redact_url_preview};

#[test]
fn redaction_contract_removes_secret_canaries_and_unbounded_url_parts() {
    assert_eq!(
        redact_text_preview("cookie=session; api_key=sk-secret"),
        "[REDACTED]"
    );
    assert_eq!(
        redact_url_preview("https://user:pass@example.test/v1/chat?api_key=sk-secret#fragment"),
        "https://example.test/path-redacted"
    );
}

#[test]
fn metrics_contract_uses_bounded_low_cardinality_events() {
    let mut buffer = LocalMetricBuffer::new(1).expect("metric buffer");
    buffer.record(
        MetricEvent::new(
            MetricKind::CommandLatency,
            42,
            vec![
                MetricLabel::Command("get_settings"),
                MetricLabel::Outcome(MetricOutcome::Ok),
            ],
        )
        .expect("metric event"),
    );
    buffer.record(
        MetricEvent::new(
            MetricKind::TaskShutdownTimeout,
            1,
            vec![MetricLabel::Task("channel-monitor-runner")],
        )
        .expect("metric event"),
    );

    let snapshot = buffer.snapshot();
    assert_eq!(snapshot.dropped, 1);
    assert_eq!(snapshot.events.len(), 1);
    assert_eq!(snapshot.events[0].kind, MetricKind::TaskShutdownTimeout);
}

#[test]
fn metrics_contract_freezes_stage4_kind_coverage_and_gc() {
    for required in [
        MetricKind::CommandLatency,
        MetricKind::CommandError,
        MetricKind::WorkspaceLatency,
        MetricKind::WorkspacePayloadBytes,
        MetricKind::WorkspaceIpcCount,
        MetricKind::TaskStatus,
        MetricKind::TaskBackoff,
        MetricKind::TaskShutdownTimeout,
        MetricKind::OperationTerminal,
        MetricKind::OperationCancelLatency,
        MetricKind::BlockingSaturation,
        MetricKind::BlockingOrphan,
        MetricKind::CollectorFailure,
        MetricKind::HiddenQueryStart,
        MetricKind::BindingDrift,
    ] {
        assert!(MetricKind::stage4_required().contains(&required));
    }

    let mut buffer = LocalMetricBuffer::with_ttl(2, 10).expect("metric buffer");
    buffer.record(
        MetricEvent::new_at(
            MetricKind::CommandLatency,
            1,
            vec![MetricLabel::Command("get_settings")],
            100,
        )
        .expect("metric event"),
    );
    buffer.collect_garbage_at(111);

    let snapshot = buffer.snapshot();
    assert_eq!(snapshot.dropped, 1);
    assert!(snapshot.events.is_empty());
}

#[test]
fn metrics_contract_rejects_secret_or_url_labels() {
    assert!(MetricEvent::new(
        MetricKind::WorkspaceLatency,
        1,
        vec![MetricLabel::WorkKind(
            "https://example.test/path?token=secret"
        )],
    )
    .is_err());
}

#[test]
fn structured_event_contract_exposes_only_stable_redacted_fields() {
    let event = StructuredEvent::new(
        "operation.result_unknown",
        StructuredEventKind::Operation,
        250,
        StructuredEventResult::Error,
        Some((
            "operation",
            "https://user:pass@example.test/v1/chat?token=sk-secret#fragment",
        )),
    )
    .expect("stable structured event");

    assert_eq!(event.code.as_str(), "operation.result_unknown");
    assert_eq!(event.duration_ms, 250);
    assert_eq!(event.result, StructuredEventResult::Error);
    let debug = format!("{event:?}");

    for forbidden in [
        "example.test",
        "fragment",
        "pass",
        "sk-secret",
        "token",
        "/v1/chat",
    ] {
        assert!(!debug.contains(forbidden), "{forbidden}");
    }
}

#[test]
fn structured_event_contract_rejects_unstable_or_secret_codes() {
    for code in [
        "Authorization",
        "C:/local-fixture/relay-pool.db",
        "https://example.test/path?token=secret",
        "prompt=response",
    ] {
        assert_eq!(
            StructuredEvent::new(
                code,
                StructuredEventKind::IpcCommand,
                1,
                StructuredEventResult::Error,
                None,
            ),
            Err(StructuredEventError::InvalidStableCode)
        );
    }
}
