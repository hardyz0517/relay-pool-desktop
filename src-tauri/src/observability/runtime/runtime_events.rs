use super::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "observability.runtime",
        "runtime.clock.wall_adjusted",
        Component::Runtime,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "observability.runtime",
        "runtime.log_event.dropped",
        Component::Runtime,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "observability.runtime",
        "runtime.crash_marker.clean_failed",
        Component::Runtime,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "observability.runtime",
        "runtime.crash_marker.locked",
        Component::Runtime,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "observability.runtime",
        "runtime.crash_marker.unavailable",
        Component::Runtime,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "observability.runtime",
        "runtime.log_recovery.completed",
        Component::Runtime,
        EventLevel::Info,
    ),
    standard_descriptor(
        "observability.runtime",
        "runtime.log_retention.degraded",
        Component::Runtime,
        EventLevel::Warn,
    ),
];

pub(crate) fn crash_marker_clean_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[2]
}
pub(crate) fn crash_marker_unavailable() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
pub(crate) fn log_recovery_completed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[5]
}
pub(crate) fn log_retention_degraded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[6]
}
