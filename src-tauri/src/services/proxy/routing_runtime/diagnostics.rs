//! Bounded, process-local diagnostic state.
//!
//! Decision traces, classification metrics, and diagnostic body memory are
//! intentionally disposable. They are observations of a proxy process, not
//! durable request outcomes or health state.

use std::sync::{Arc, Mutex};

pub(crate) use super::super::diagnostic_memory::{
    DiagnosticMemoryBudget, DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES,
};
use crate::observability::{
    decision_trace::{DecisionTraceEventKind, DecisionTraceRing, RequestDecisionTraceV1},
    metrics::{ClassificationMetricLabel, LocalMetricBuffer, MetricEvent, MetricKind, MetricLabel},
};

#[cfg(test)]
use crate::observability::metrics::MetricSnapshot;

#[derive(Debug)]
pub(crate) struct DiagnosticsState {
    diagnostic_memory: DiagnosticMemoryBudget,
    decision_traces: Arc<Mutex<DecisionTraceRing>>,
    classification_metrics: Arc<Mutex<LocalMetricBuffer>>,
}

impl DiagnosticsState {
    pub(crate) fn new() -> Self {
        Self {
            diagnostic_memory: DiagnosticMemoryBudget::new(DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES),
            decision_traces: Arc::new(Mutex::new(DecisionTraceRing::new())),
            classification_metrics: Arc::new(Mutex::new(
                LocalMetricBuffer::new(2_048).expect("non-zero routing metric capacity"),
            )),
        }
    }

    pub(crate) fn diagnostic_memory_budget(&self) -> DiagnosticMemoryBudget {
        self.diagnostic_memory.clone()
    }

    /// Appends one completed request trace to the process-local bounded ring.
    /// The ring is never persisted and remains bounded by DecisionTrace's
    /// profile.
    pub(crate) fn record_decision_trace(&self, trace: RequestDecisionTraceV1) {
        self.record_classification_metrics(&trace);
        self.decision_traces
            .lock()
            .expect("decision trace ring poisoned")
            .push(trace);
    }

    fn record_classification_metrics(&self, trace: &RequestDecisionTraceV1) {
        let Ok(mut metrics) = self.classification_metrics.lock() else {
            return;
        };
        for event in &trace.events {
            let label = match event.kind {
                DecisionTraceEventKind::AttemptStart => ClassificationMetricLabel::AttemptStart,
                DecisionTraceEventKind::CanonicalFailure => {
                    ClassificationMetricLabel::CanonicalFailure
                }
                DecisionTraceEventKind::SameTargetRetry => {
                    ClassificationMetricLabel::SameTargetRetry
                }
                DecisionTraceEventKind::SameDomainFallbackSuppressed => {
                    ClassificationMetricLabel::SameDomainSuppressed
                }
                DecisionTraceEventKind::CrossDomainFallback => {
                    ClassificationMetricLabel::CrossDomainFallback
                }
                DecisionTraceEventKind::CommittedStop => ClassificationMetricLabel::CommittedStop,
                DecisionTraceEventKind::SseErrorBeforeSemanticCommit => {
                    ClassificationMetricLabel::SsePrecommitError
                }
                DecisionTraceEventKind::Saturation => ClassificationMetricLabel::Saturation,
                DecisionTraceEventKind::FailClosed => ClassificationMetricLabel::FailClosed,
                DecisionTraceEventKind::ProfileVersionMismatch => {
                    ClassificationMetricLabel::ProfileMismatch
                }
                DecisionTraceEventKind::TraceTruncated => ClassificationMetricLabel::Truncated,
                DecisionTraceEventKind::RequestTerminal => {
                    ClassificationMetricLabel::RequestTerminal
                }
            };
            // Classification labels are closed; do not attach request or
            // provider identity to this bounded process-local metric.
            if let Ok(metric) = MetricEvent::new(
                MetricKind::Classification,
                1,
                vec![MetricLabel::Classification(label)],
            ) {
                metrics.record(metric);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn classification_metrics_snapshot(&self) -> MetricSnapshot {
        self.classification_metrics
            .lock()
            .expect("classification metric buffer poisoned")
            .snapshot()
    }

    pub(crate) fn decision_trace_snapshot(&self) -> Vec<RequestDecisionTraceV1> {
        self.decision_traces
            .lock()
            .expect("decision trace ring poisoned")
            .traces()
            .cloned()
            .collect()
    }
}
