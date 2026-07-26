#![allow(
    dead_code,
    reason = "Task 18A freezes the local metrics contract before production recorders are wired to it"
)]

use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_METRIC_LABELS: usize = 6;
pub(crate) const MAX_METRIC_LABEL_VALUE_BYTES: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricKind {
    BindingDrift,
    BlockingOrphan,
    BlockingSaturation,
    CollectorFailure,
    CommandError,
    CommandLatency,
    HiddenQueryStart,
    OperationCancelLatency,
    OperationTerminal,
    RuntimeStatus,
    TaskBackoff,
    TaskShutdownTimeout,
    TaskStatus,
    WorkspaceIpcCount,
    WorkspaceLatency,
    WorkspacePayloadBytes,
}

impl MetricKind {
    pub(crate) fn stage4_required() -> &'static [Self] {
        &[
            Self::BindingDrift,
            Self::BlockingOrphan,
            Self::BlockingSaturation,
            Self::CollectorFailure,
            Self::CommandError,
            Self::CommandLatency,
            Self::HiddenQueryStart,
            Self::OperationCancelLatency,
            Self::OperationTerminal,
            Self::RuntimeStatus,
            Self::TaskBackoff,
            Self::TaskShutdownTimeout,
            Self::TaskStatus,
            Self::WorkspaceIpcCount,
            Self::WorkspaceLatency,
            Self::WorkspacePayloadBytes,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricLabel {
    Command(&'static str),
    Outcome(MetricOutcome),
    Runtime(RuntimeMetricLabel),
    Task(&'static str),
    WorkKind(&'static str),
}

impl MetricLabel {
    fn validate(self) -> Result<Self, MetricError> {
        match self {
            Self::Command(value) | Self::Task(value) | Self::WorkKind(value) => {
                validate_label_value(value)?;
            }
            Self::Outcome(_) | Self::Runtime(_) => {}
        }
        Ok(self)
    }
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
    pub recorded_at_ms: u64,
}

impl MetricEvent {
    pub(crate) fn new(
        kind: MetricKind,
        value: u64,
        labels: Vec<MetricLabel>,
    ) -> Result<Self, MetricError> {
        Self::new_at(kind, value, labels, now_millis())
    }

    pub(crate) fn new_at(
        kind: MetricKind,
        value: u64,
        labels: Vec<MetricLabel>,
        recorded_at_ms: u64,
    ) -> Result<Self, MetricError> {
        if labels.len() > MAX_METRIC_LABELS {
            return Err(MetricError::TooManyLabels);
        }
        let labels = labels
            .into_iter()
            .map(MetricLabel::validate)
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            kind,
            value,
            labels,
            recorded_at_ms,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricError {
    InvalidLabel,
    TooManyLabels,
    ZeroCapacity,
    ZeroTtl,
}

#[derive(Debug)]
pub(crate) struct LocalMetricBuffer {
    capacity: usize,
    ttl_ms: Option<u64>,
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
            ttl_ms: None,
            dropped: 0,
            events: VecDeque::with_capacity(capacity),
        })
    }

    pub(crate) fn with_ttl(capacity: usize, ttl_ms: u64) -> Result<Self, MetricError> {
        if ttl_ms == 0 {
            return Err(MetricError::ZeroTtl);
        }
        let mut buffer = Self::new(capacity)?;
        buffer.ttl_ms = Some(ttl_ms);
        Ok(buffer)
    }

    pub(crate) fn record(&mut self, event: MetricEvent) {
        self.collect_garbage_at(event.recorded_at_ms);
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

    pub(crate) fn collect_garbage_at(&mut self, now_ms: u64) {
        let Some(ttl_ms) = self.ttl_ms else {
            return;
        };
        while self
            .events
            .front()
            .is_some_and(|event| now_ms.saturating_sub(event.recorded_at_ms) > ttl_ms)
        {
            self.events.pop_front();
            self.dropped += 1;
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MetricSnapshot {
    pub dropped: u64,
    pub events: Vec<MetricEvent>,
}

fn validate_label_value(value: &str) -> Result<(), MetricError> {
    let lower = value.to_ascii_lowercase();
    if value.is_empty()
        || value.len() > MAX_METRIC_LABEL_VALUE_BYTES
        || value.contains("://")
        || value.contains('?')
        || value.contains('=')
        || lower.contains("authorization")
        || lower.contains("bearer")
        || lower.contains("cookie")
        || lower.contains("password")
        || lower.contains("sk-")
        || lower.contains("token")
    {
        return Err(MetricError::InvalidLabel);
    }
    Ok(())
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(u64::MAX as u128) as u64
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
    fn metric_events_reject_secret_url_and_unbounded_freeform_labels() {
        for label in [
            MetricLabel::Command("https://provider.example/v1?token=secret"),
            MetricLabel::Task("authorization-bearer-sk-secret"),
            MetricLabel::WorkKind(
                "a-label-that-is-longer-than-the-allowed-low-cardinality-budget-for-metrics",
            ),
        ] {
            assert_eq!(
                MetricEvent::new(MetricKind::CommandLatency, 1, vec![label]),
                Err(MetricError::InvalidLabel)
            );
        }
    }

    #[test]
    fn stage4_required_metric_kinds_cover_runtime_and_frontend_contracts() {
        let kinds = MetricKind::stage4_required();
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
            assert!(
                kinds.contains(&required),
                "missing required metric kind: {required:?}"
            );
        }
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

    #[test]
    fn local_metric_buffer_collects_expired_events_by_ttl() {
        let mut buffer = LocalMetricBuffer::with_ttl(4, 10).expect("buffer");
        buffer.record(
            MetricEvent::new_at(
                MetricKind::CommandLatency,
                1,
                vec![MetricLabel::Command("get_settings")],
                100,
            )
            .expect("first event"),
        );
        buffer.record(
            MetricEvent::new_at(
                MetricKind::CommandLatency,
                2,
                vec![MetricLabel::Command("list_stations")],
                105,
            )
            .expect("second event"),
        );

        buffer.collect_garbage_at(111);
        let snapshot = buffer.snapshot();

        assert_eq!(snapshot.dropped, 1);
        assert_eq!(snapshot.events.len(), 1);
        assert_eq!(snapshot.events[0].value, 2);
    }
}
