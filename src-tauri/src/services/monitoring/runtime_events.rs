use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.maintenance.failed",
        Component::Monitoring,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.maintenance.timeout",
        Component::Monitoring,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.runner.failed",
        Component::Monitoring,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.runner.query_failed",
        Component::Monitoring,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.runner.worker_failed",
        Component::Monitoring,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.runner.cancelled",
        Component::Monitoring,
        EventLevel::Info,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.runner.degraded",
        Component::Monitoring,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "monitoring.runtime",
        "monitoring.maintenance.cancelled",
        Component::Monitoring,
        EventLevel::Info,
    ),
    standard_descriptor(
        "monitoring.transport",
        "monitoring.transport.failed",
        Component::Monitoring,
        EventLevel::Warn,
    ),
];

pub(crate) fn maintenance_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
pub(crate) fn maintenance_timeout() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[1]
}
pub(crate) fn runner_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[2]
}
pub(crate) fn runner_query_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[3]
}
pub(crate) fn runner_worker_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
pub(crate) fn runner_cancelled() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[5]
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=monitoring.runner.degraded; owner=services/monitoring; remove_when=degraded runner state is emitted by the production runner"
    )
)]
pub(crate) fn runner_degraded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[6]
}
pub(crate) fn maintenance_cancelled() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[7]
}
pub(crate) fn transport_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[8]
}
