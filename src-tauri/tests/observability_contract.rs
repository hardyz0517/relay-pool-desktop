#[path = "../src/observability/decision_trace.rs"]
mod decision_trace;
#[path = "../src/observability/metrics.rs"]
mod metrics;
#[path = "../src/observability/runtime/subject.rs"]
mod subject;

use decision_trace::{
    DecisionTraceBuilder, DecisionTraceEvent, DecisionTraceEventKind, DecisionTraceRing,
    MAX_TRACE_EVENTS_PER_REQUEST, TRACE_RING_MAX_RETAINED_BYTES, TRACE_RING_MAX_TRACES,
};
use metrics::{ClassificationMetricLabel, LocalMetricBuffer, MetricEvent, MetricKind, MetricLabel};
use subject::{RedactedResourceId, StableEventCode};

#[test]
fn metrics_contract_uses_bounded_low_cardinality_events() {
    let mut buffer = LocalMetricBuffer::new(1).expect("metric buffer");
    buffer.record(
        MetricEvent::new(
            MetricKind::Classification,
            42,
            vec![MetricLabel::Classification(
                ClassificationMetricLabel::AttemptStart,
            )],
        )
        .expect("metric event"),
    );
    buffer.record(
        MetricEvent::new(
            MetricKind::Classification,
            1,
            vec![MetricLabel::Classification(
                ClassificationMetricLabel::RequestTerminal,
            )],
        )
        .expect("metric event"),
    );

    let snapshot = buffer.snapshot();
    assert_eq!(snapshot.dropped, 1);
    assert_eq!(snapshot.events.len(), 1);
    assert_eq!(snapshot.events[0].kind, MetricKind::Classification);
}

#[test]
fn metrics_contract_is_bounded_and_expires_old_samples() {
    let mut buffer = LocalMetricBuffer::with_ttl(2, 10).expect("metric buffer");
    buffer.record(
        MetricEvent::new_at(
            MetricKind::Classification,
            1,
            vec![MetricLabel::Classification(
                ClassificationMetricLabel::AttemptStart,
            )],
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
fn runtime_subject_contract_exposes_only_stable_redacted_fields() {
    let code = StableEventCode::new("operation.result_unknown").expect("stable event code");
    assert_eq!(code.as_str(), "operation.result_unknown");

    let resource = RedactedResourceId::from_raw(
        "operation",
        "https://user:pass@example.test/v1/chat?token=sk-secret#fragment",
    )
    .expect("hashed resource");
    assert!(resource.as_str().starts_with("res_"));
    let debug = format!("{resource:?}");
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
fn runtime_subject_contract_rejects_unstable_or_secret_codes() {
    for code in [
        "Authorization",
        "C:/local-fixture/relay-pool.db",
        "https://example.test/path?token=secret",
        "prompt=response",
    ] {
        assert!(StableEventCode::new(code).is_err());
    }
}

#[test]
fn decision_trace_profile_freezes_attempt_event_and_ring_ceilings() {
    assert_eq!(TRACE_RING_MAX_TRACES, 512);
    assert_eq!(TRACE_RING_MAX_RETAINED_BYTES, 16 * 1024 * 1024);
    assert_eq!(MAX_TRACE_EVENTS_PER_REQUEST, 64);
    assert!(DecisionTraceEvent::new(
        DecisionTraceEventKind::AttemptStart,
        "attempt_start",
        0,
        None,
    )
    .is_ok());
    assert!(DecisionTraceEvent::new(
        DecisionTraceEventKind::AttemptStart,
        "https://example.test/v1?token=sk-secret",
        0,
        None,
    )
    .is_err());
}

#[test]
fn decision_trace_ring_is_bounded_and_evicts_oldest_complete_trace() {
    let mut ring = DecisionTraceRing::with_limits(2, 4096).expect("ring");
    for index in 0..3 {
        let mut builder = DecisionTraceBuilder::new(&format!("req-{index}")).expect("builder");
        builder
            .record(
                DecisionTraceEvent::new(
                    DecisionTraceEventKind::AttemptStart,
                    "attempt_start",
                    index,
                    None,
                )
                .expect("event"),
            )
            .expect("record");
        ring.push(builder.finish());
    }
    assert_eq!(ring.len(), 2);
    assert_eq!(ring.dropped_traces(), 1);
    assert_eq!(ring.traces().next().unwrap().request_id, "req-1");
}

#[test]
fn decision_trace_truncation_appends_exactly_one_marker() {
    let mut builder = DecisionTraceBuilder::new("req-truncated").expect("builder");
    for _ in 0..MAX_TRACE_EVENTS_PER_REQUEST {
        builder
            .record(
                DecisionTraceEvent::new(
                    DecisionTraceEventKind::AttemptStart,
                    "attempt_start",
                    0,
                    None,
                )
                .expect("event"),
            )
            .expect("within cap");
    }
    assert!(builder
        .record(
            DecisionTraceEvent::new(
                DecisionTraceEventKind::CanonicalFailure,
                "canonical_failure",
                99,
                None,
            )
            .expect("event"),
        )
        .is_err());
    let trace = builder.finish();
    assert!(trace.trace_truncated);
    assert_eq!(
        trace
            .events
            .iter()
            .filter(|event| event.kind == DecisionTraceEventKind::TraceTruncated)
            .count(),
        1
    );
}
