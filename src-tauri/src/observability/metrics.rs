#![allow(
    dead_code,
    reason = "Task 18A freezes the local metrics contract before production recorders are wired to it"
)]

use std::collections::VecDeque;

pub(crate) const MAX_METRIC_LABELS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricKind {
    BindingDrift,
    BlockingSaturation,
    CommandError,
    CommandLatency,
    OperationTerminal,
    RuntimeStatus,
    TaskShutdownTimeout,
    WorkspaceLatency,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricLabel {
    Command(&'static str),
    Outcome(MetricOutcome),
    Runtime(RuntimeMetricLabel),
    Task(&'static str),
    WorkKind(&'static str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RuntimeMetricLabel {
    BlockingOrphaned,
    BlockingQueued,
    BlockingRunning,
    OperationExpiredTombstones,
    OperationRunning,
    OperationStored,
    OperationTerminal,
    OutboundClientInstancesCreated,
    OutboundPoolSize,
    TaskActive,
    TaskRegistered,
    TaskTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricOutcome {
    Cancelled,
    Error,
    Ok,
    Overloaded,
    Timeout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetricEvent {
    pub kind: MetricKind,
    pub value: u64,
    pub labels: Vec<MetricLabel>,
}

impl MetricEvent {
    pub(crate) fn new(
        kind: MetricKind,
        value: u64,
        labels: Vec<MetricLabel>,
    ) -> Result<Self, MetricError> {
        if labels.len() > MAX_METRIC_LABELS {
            return Err(MetricError::TooManyLabels);
        }
        Ok(Self {
            kind,
            value,
            labels,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricError {
    TooManyLabels,
    ZeroCapacity,
}

#[derive(Debug)]
pub(crate) struct LocalMetricBuffer {
    capacity: usize,
    dropped: u64,
    events: VecDeque<MetricEvent>,
}

impl LocalMetricBuffer {
    pub(crate) fn new(capacity: usize) -> Result<Self, MetricError> {
        if capacity == 0 {
            return Err(MetricError::ZeroCapacity);
        }
        Ok(Self {
            capacity,
            dropped: 0,
            events: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn record(&mut self, event: MetricEvent) {
        if self.events.len() == self.capacity {
            self.events.pop_front();
            self.dropped += 1;
        }
        self.events.push_back(event);
    }

    pub(crate) fn snapshot(&self) -> MetricSnapshot {
        MetricSnapshot {
            dropped: self.dropped,
            events: self.events.iter().cloned().collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetricSnapshot {
    pub dropped: u64,
    pub events: Vec<MetricEvent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_events_reject_unbounded_label_fanout() {
        let labels = vec![MetricLabel::Outcome(MetricOutcome::Ok); MAX_METRIC_LABELS + 1];

        assert_eq!(
            MetricEvent::new(MetricKind::CommandLatency, 10, labels),
            Err(MetricError::TooManyLabels)
        );
    }

    #[test]
    fn local_metric_buffer_is_bounded_and_drops_oldest() {
        let mut buffer = LocalMetricBuffer::new(2).expect("buffer");
        for value in 1..=3 {
            buffer.record(
                MetricEvent::new(
                    MetricKind::BlockingSaturation,
                    value,
                    vec![MetricLabel::WorkKind("blocking")],
                )
                .expect("metric event"),
            );
        }

        let snapshot = buffer.snapshot();
        assert_eq!(snapshot.dropped, 1);
        assert_eq!(snapshot.events.len(), 2);
        assert_eq!(snapshot.events[0].value, 2);
        assert_eq!(snapshot.events[1].value, 3);
    }
}
