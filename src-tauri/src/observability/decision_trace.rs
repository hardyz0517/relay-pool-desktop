// DecisionTraceProfileV1 freezes every hard limit for per-request routing
// decision traces. The profile is owned by this single module; DTOs, stores
// and Execution must not scatter the constants.
//
// Traces are an in-memory bounded ring only. They never enter persistence,
// IPC or metric labels, so request ids and redacted resource hashes are safe.
use std::collections::VecDeque;

#[cfg(not(test))]
use super::runtime::subject::is_stable_token;
#[cfg(test)]
use super::subject::is_stable_token;

pub(crate) const DECISION_TRACE_PROFILE_VERSION: &str = "DecisionTraceProfileV1";
pub(crate) const MAX_OUTBOUND_ATTEMPTS_PER_TRACE: usize = 4;
pub(crate) const MAX_TRACE_EVENTS_PER_REQUEST: usize = 64;
pub(crate) const MAX_TRACE_FIELD_BYTES: usize = 512;
pub(crate) const MAX_SERIALIZED_TRACE_BYTES: usize = 32 * 1024;
pub(crate) const TRACE_RING_MAX_TRACES: usize = 512;
pub(crate) const TRACE_RING_MAX_RETAINED_BYTES: usize = 16 * 1024 * 1024;

const TRACE_EVENT_ENVELOPE_OVERHEAD_BYTES: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecisionTraceEventKind {
    AttemptStart,
    CanonicalFailure,
    SameTargetRetry,
    SameDomainFallbackSuppressed,
    CrossDomainFallback,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=decision-trace.committed-stop; owner=observability/decision_trace; remove_when=committed stream finalization surfaces no-retry as a trace event"
        )
    )]
    CommittedStop,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=decision-trace.sse-precommit-error; owner=observability/decision_trace; remove_when=SSE bootstrap surfaces precommit protocol terminals as trace events"
        )
    )]
    SseErrorBeforeSemanticCommit,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=decision-trace.saturation; owner=observability/decision_trace; remove_when=retry admission and diagnostic memory saturation are surfaced as trace events"
        )
    )]
    Saturation,
    FailClosed,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=decision-trace.profile-mismatch; owner=observability/decision_trace; remove_when=profile version mismatch is surfaced as a trace event"
        )
    )]
    ProfileVersionMismatch,
    TraceTruncated,
    RequestTerminal,
}

impl DecisionTraceEventKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::AttemptStart => "attempt_start",
            Self::CanonicalFailure => "canonical_failure",
            Self::SameTargetRetry => "same_target_retry",
            Self::SameDomainFallbackSuppressed => "same_domain_fallback_suppressed",
            Self::CrossDomainFallback => "cross_domain_fallback",
            Self::CommittedStop => "committed_stop",
            Self::SseErrorBeforeSemanticCommit => "sse_error_before_semantic_commit",
            Self::Saturation => "saturation",
            Self::FailClosed => "fail_closed",
            Self::ProfileVersionMismatch => "profile_version_mismatch",
            Self::TraceTruncated => "trace_truncated",
            Self::RequestTerminal => "request_terminal",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionTraceEvent {
    pub(crate) kind: DecisionTraceEventKind,
    pub(crate) code: String,
    pub(crate) ordinal: u32,
    pub(crate) detail: Option<String>,
}

impl DecisionTraceEvent {
    pub(crate) fn new(
        kind: DecisionTraceEventKind,
        code: &str,
        ordinal: u32,
        detail: Option<&str>,
    ) -> Result<Self, DecisionTraceError> {
        if !is_stable_token(code) {
            return Err(DecisionTraceError::InvalidStableCode);
        }
        let detail = match detail {
            Some(value) if is_trace_detail(value) => Some(value.to_string()),
            Some(_) => return Err(DecisionTraceError::InvalidStableCode),
            None => None,
        };
        Ok(Self {
            kind,
            code: code.to_string(),
            ordinal,
            detail,
        })
    }

    fn serialized_estimate(&self) -> usize {
        self.code.len()
            + self.detail.as_deref().map_or(0, str::len)
            + self.kind.as_str().len()
            + TRACE_EVENT_ENVELOPE_OVERHEAD_BYTES
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestDecisionTraceV1 {
    pub(crate) request_id: String,
    pub(crate) profile_version: &'static str,
    pub(crate) events: Vec<DecisionTraceEvent>,
    pub(crate) trace_truncated: bool,
    pub(crate) serialized_bytes_estimate: usize,
}

impl RequestDecisionTraceV1 {
    pub(crate) fn retained_bytes(&self) -> usize {
        self.serialized_bytes_estimate
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DecisionTraceError {
    InvalidStableCode,
    TooManyEvents,
    SerializedTooLarge,
    RequestIdTooLong,
}

/// Trace detail fields are bounded to the profile field cap and must stay
/// free of secrets, URLs, queries and paths. Codes themselves keep the
/// tighter stable-code budget from `events`.
fn is_trace_detail(value: &str) -> bool {
    if value.is_empty() || value.len() > MAX_TRACE_FIELD_BYTES {
        return false;
    }
    if !value.bytes().all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    }) {
        return false;
    }
    let lower = value.to_ascii_lowercase();
    !value.contains("://")
        && !value.contains('?')
        && !value.contains('=')
        && !value.contains('\\')
        && !value.contains('/')
        && !lower.contains("authorization")
        && !lower.contains("bearer")
        && !lower.contains("cookie")
        && !lower.contains("password")
        && !lower.contains("sk-")
        && !lower.contains("token")
}

/// Request-scoped builder enforces the per-request profile. After the event
/// cap is reached it appends exactly one `trace_truncated` marker (if it
/// fits) and then rejects further events.
pub(crate) struct DecisionTraceBuilder {
    request_id: String,
    events: Vec<DecisionTraceEvent>,
    trace_truncated: bool,
    stopped: bool,
    serialized_bytes_estimate: usize,
}

impl DecisionTraceBuilder {
    pub(crate) fn new(request_id: &str) -> Result<Self, DecisionTraceError> {
        if request_id.len() > MAX_TRACE_FIELD_BYTES {
            return Err(DecisionTraceError::RequestIdTooLong);
        }
        Ok(Self {
            request_id: request_id.to_string(),
            events: Vec::new(),
            trace_truncated: false,
            stopped: false,
            serialized_bytes_estimate: 0,
        })
    }

    pub(crate) fn record(&mut self, event: DecisionTraceEvent) -> Result<(), DecisionTraceError> {
        if self.stopped {
            return Err(DecisionTraceError::TooManyEvents);
        }
        if self.events.len() >= MAX_TRACE_EVENTS_PER_REQUEST {
            self.mark_truncated()?;
            self.stopped = true;
            return Err(DecisionTraceError::TooManyEvents);
        }
        if event.kind == DecisionTraceEventKind::AttemptStart
            && event.ordinal >= MAX_OUTBOUND_ATTEMPTS_PER_TRACE as u32
        {
            self.mark_truncated()?;
            self.stopped = true;
            return Err(DecisionTraceError::TooManyEvents);
        }
        let event_bytes = event.serialized_estimate();
        if self
            .serialized_bytes_estimate
            .checked_add(event_bytes)
            .is_none_or(|next| next > MAX_SERIALIZED_TRACE_BYTES)
        {
            self.mark_truncated()?;
            self.stopped = true;
            return Err(DecisionTraceError::SerializedTooLarge);
        }
        self.serialized_bytes_estimate += event_bytes;
        self.events.push(event);
        Ok(())
    }

    fn mark_truncated(&mut self) -> Result<(), DecisionTraceError> {
        if self.trace_truncated {
            return Ok(());
        }
        let marker = DecisionTraceEvent::new(
            DecisionTraceEventKind::TraceTruncated,
            "trace_truncated",
            0,
            None,
        )?;
        let marker_bytes = marker.serialized_estimate();
        if self
            .serialized_bytes_estimate
            .checked_add(marker_bytes)
            .is_none_or(|next| next > MAX_SERIALIZED_TRACE_BYTES)
        {
            // The marker itself cannot fit; the envelope bit is the only
            // signal that remains.
            return Ok(());
        }
        self.serialized_bytes_estimate += marker_bytes;
        self.events.push(marker);
        self.trace_truncated = true;
        Ok(())
    }

    pub(crate) fn finish(self) -> RequestDecisionTraceV1 {
        RequestDecisionTraceV1 {
            request_id: self.request_id,
            profile_version: DECISION_TRACE_PROFILE_VERSION,
            events: self.events,
            trace_truncated: self.trace_truncated,
            serialized_bytes_estimate: self.serialized_bytes_estimate,
        }
    }
}

/// Process-local bounded ring. When either the trace count or the retained
/// byte ceiling is exceeded, the oldest complete trace is evicted.
#[derive(Debug, Clone)]
pub(crate) struct DecisionTraceRing {
    max_traces: usize,
    max_retained_bytes: usize,
    retained_bytes: usize,
    traces: VecDeque<RequestDecisionTraceV1>,
    dropped_traces: u64,
}

impl DecisionTraceRing {
    pub(crate) fn new() -> Self {
        Self {
            max_traces: TRACE_RING_MAX_TRACES,
            max_retained_bytes: TRACE_RING_MAX_RETAINED_BYTES,
            retained_bytes: 0,
            traces: VecDeque::new(),
            dropped_traces: 0,
        }
    }

    #[cfg(test)]
    pub(crate) fn with_limits(
        max_traces: usize,
        max_retained_bytes: usize,
    ) -> Result<Self, DecisionTraceError> {
        if max_traces == 0 || max_retained_bytes == 0 {
            return Err(DecisionTraceError::SerializedTooLarge);
        }
        Ok(Self {
            max_traces,
            max_retained_bytes,
            retained_bytes: 0,
            traces: VecDeque::new(),
            dropped_traces: 0,
        })
    }

    /// Pushes a complete trace, evicting oldest traces until both ceilings
    /// hold. Returns the number of evicted traces (including the pushed one
    /// when the trace itself exceeds the retained ceiling).
    pub(crate) fn push(&mut self, trace: RequestDecisionTraceV1) -> usize {
        let mut evicted = 0_usize;
        let trace_bytes = trace.retained_bytes();
        self.traces.push_back(trace);
        self.retained_bytes = self.retained_bytes.saturating_add(trace_bytes);
        while (self.traces.len() > self.max_traces || self.retained_bytes > self.max_retained_bytes)
            && !self.traces.is_empty()
        {
            let oldest = self
                .traces
                .pop_front()
                .expect("non-empty trace ring just checked");
            self.retained_bytes = self.retained_bytes.saturating_sub(oldest.retained_bytes());
            self.dropped_traces = self.dropped_traces.saturating_add(1);
            evicted = evicted.saturating_add(1);
        }
        evicted
    }

    pub(crate) fn traces(&self) -> impl Iterator<Item = &RequestDecisionTraceV1> {
        self.traces.iter()
    }

    #[cfg(test)]
    pub(crate) fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.traces.len()
    }

    #[cfg(test)]
    pub(crate) fn dropped_traces(&self) -> u64 {
        self.dropped_traces
    }
}

impl Default for DecisionTraceRing {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(kind: DecisionTraceEventKind, code: &str, ordinal: u32) -> DecisionTraceEvent {
        DecisionTraceEvent::new(kind, code, ordinal, None).expect("stable event")
    }

    fn trace(request_id: &str, events: Vec<DecisionTraceEvent>) -> RequestDecisionTraceV1 {
        let mut builder = DecisionTraceBuilder::new(request_id).expect("builder");
        for event in events {
            let _ = builder.record(event);
        }
        builder.finish()
    }

    #[test]
    fn profile_constants_are_the_single_frozen_source() {
        assert_eq!(MAX_OUTBOUND_ATTEMPTS_PER_TRACE, 4);
        assert_eq!(MAX_TRACE_EVENTS_PER_REQUEST, 64);
        assert_eq!(MAX_TRACE_FIELD_BYTES, 512);
        assert_eq!(MAX_SERIALIZED_TRACE_BYTES, 32 * 1024);
        assert_eq!(TRACE_RING_MAX_TRACES, 512);
        assert_eq!(TRACE_RING_MAX_RETAINED_BYTES, 16 * 1024 * 1024);
        assert_eq!(DECISION_TRACE_PROFILE_VERSION, "DecisionTraceProfileV1");
    }

    #[test]
    fn trace_events_reject_secret_url_query_and_unbounded_codes() {
        for code in [
            "https://example.test/v1?token=secret",
            "Authorization: Bearer sk-secret",
            "station_key:sk-live-abc",
            "C:\\data\\relay-pool.db",
        ] {
            assert_eq!(
                DecisionTraceEvent::new(DecisionTraceEventKind::AttemptStart, code, 0, None),
                Err(DecisionTraceError::InvalidStableCode)
            );
        }
    }

    #[test]
    fn trace_builder_appends_exactly_one_truncated_marker_then_stops() {
        let mut builder = DecisionTraceBuilder::new("req-1").expect("builder");
        for _ in 0..MAX_TRACE_EVENTS_PER_REQUEST {
            builder
                .record(event(
                    DecisionTraceEventKind::AttemptStart,
                    "attempt_start",
                    0,
                ))
                .expect("event within cap");
        }
        assert_eq!(
            builder.record(event(
                DecisionTraceEventKind::AttemptStart,
                "attempt_start",
                99
            )),
            Err(DecisionTraceError::TooManyEvents)
        );
        let trace = builder.finish();
        assert_eq!(trace.events.len(), MAX_TRACE_EVENTS_PER_REQUEST + 1);
        assert!(trace.trace_truncated);
        assert_eq!(
            trace.events.last().unwrap().kind,
            DecisionTraceEventKind::TraceTruncated
        );
        assert_eq!(
            trace
                .events
                .iter()
                .filter(|event| event.kind == DecisionTraceEventKind::TraceTruncated)
                .count(),
            1
        );
    }

    #[test]
    fn trace_builder_rejects_oversized_serialized_trace_and_keeps_envelope_signal() {
        let mut builder = DecisionTraceBuilder::new("req-1").expect("builder");
        let mut huge = String::new();
        while huge.len() + TRACE_EVENT_ENVELOPE_OVERHEAD_BYTES <= MAX_TRACE_FIELD_BYTES {
            huge.push('a');
        }
        // A single event near the field cap still fits; overflow is enforced
        // only through the serialized ceiling, so craft many large events.
        for _ in 0..MAX_TRACE_EVENTS_PER_REQUEST {
            let outcome = builder.record(
                DecisionTraceEvent::new(
                    DecisionTraceEventKind::CanonicalFailure,
                    "canonical_failure",
                    0,
                    Some(&huge),
                )
                .expect("event within field cap"),
            );
            if outcome.is_err() {
                break;
            }
        }
        let trace = builder.finish();
        assert!(trace.serialized_bytes_estimate <= MAX_SERIALIZED_TRACE_BYTES);
        assert!(trace.trace_truncated);
        assert!(trace
            .events
            .last()
            .is_none_or(|event| event.kind == DecisionTraceEventKind::TraceTruncated));
    }

    #[test]
    fn ring_evicts_oldest_until_both_ceilings_hold() {
        let mut ring = DecisionTraceRing::with_limits(2, 1024).expect("ring");
        for index in 0..3 {
            ring.push(trace(
                &format!("req-{index}"),
                vec![event(
                    DecisionTraceEventKind::AttemptStart,
                    "attempt_start",
                    0,
                )],
            ));
        }
        assert_eq!(ring.len(), 2);
        assert_eq!(ring.dropped_traces(), 1);
        assert_eq!(ring.traces().next().unwrap().request_id, "req-1");
        assert!(ring.retained_bytes() <= 1024);
    }

    #[test]
    fn ring_rejects_a_trace_larger_than_the_whole_budget() {
        let mut ring = DecisionTraceRing::with_limits(1, 64).expect("ring");
        let large = trace(
            "req-big",
            vec![
                event(
                    DecisionTraceEventKind::CanonicalFailure,
                    "canonical_failure",
                    0,
                ),
                event(
                    DecisionTraceEventKind::SameTargetRetry,
                    "same_target_retry",
                    1,
                ),
            ],
        );
        assert!(large.retained_bytes() > 64);
        ring.push(large);
        assert_eq!(ring.len(), 0);
        assert_eq!(ring.dropped_traces(), 1);
    }

    #[test]
    fn builder_and_ring_profile_are_used_by_production_defaults() {
        let ring = DecisionTraceRing::new();
        assert_eq!(ring.max_traces, TRACE_RING_MAX_TRACES);
        assert_eq!(ring.max_retained_bytes, TRACE_RING_MAX_RETAINED_BYTES);
    }
}
