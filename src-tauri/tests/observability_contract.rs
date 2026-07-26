#[path = "../src/observability/metrics.rs"]
mod metrics;
#[path = "../src/observability/redaction.rs"]
mod redaction;

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
