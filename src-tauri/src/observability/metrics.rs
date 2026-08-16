use std::{
    collections::VecDeque,
    time::{SystemTime, UNIX_EPOCH},
};

pub(crate) const MAX_METRIC_LABELS: usize = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricKind {
    Classification,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ClassificationMetricLabel {
    AttemptStart,
    CanonicalFailure,
    SameTargetRetry,
    SameDomainSuppressed,
    CrossDomainFallback,
    CommittedStop,
    SsePrecommitError,
    Saturation,
    FailClosed,
    ProfileMismatch,
    Truncated,
    RequestTerminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MetricLabel {
    Classification(ClassificationMetricLabel),
}

impl MetricLabel {
    fn validate(self) -> Self {
        let Self::Classification(_) = self;
        self
    }
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
            .collect::<Vec<_>>();
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
    TooManyLabels,
    ZeroCapacity,
    #[cfg(test)]
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

    #[cfg(test)]
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

    #[cfg(test)]
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
#[cfg(test)]
pub(crate) struct MetricSnapshot {
    pub dropped: u64,
    pub events: Vec<MetricEvent>,
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
        let labels = vec![
            MetricLabel::Classification(ClassificationMetricLabel::AttemptStart);
            MAX_METRIC_LABELS + 1
        ];

        assert_eq!(
            MetricEvent::new(MetricKind::Classification, 10, labels),
            Err(MetricError::TooManyLabels)
        );
    }

    #[test]
    fn local_metric_buffer_is_bounded_and_drops_oldest() {
        let mut buffer = LocalMetricBuffer::new(2).expect("buffer");
        for value in 1..=3 {
            buffer.record(
                MetricEvent::new(
                    MetricKind::Classification,
                    value,
                    vec![MetricLabel::Classification(
                        ClassificationMetricLabel::Saturation,
                    )],
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
                MetricKind::Classification,
                1,
                vec![MetricLabel::Classification(
                    ClassificationMetricLabel::AttemptStart,
                )],
                100,
            )
            .expect("first event"),
        );
        buffer.record(
            MetricEvent::new_at(
                MetricKind::Classification,
                2,
                vec![MetricLabel::Classification(
                    ClassificationMetricLabel::RequestTerminal,
                )],
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
