use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "collector.station",
        "collector.station.query_failed",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "collector.station",
        "collector.station.started",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "collector.station",
        "collector.station.completed",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "collector.station",
        "collector.station.failed",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "collector.station",
        "collector.station.cancelled",
        Component::Collector,
        EventLevel::Info,
    ),
    standard_descriptor(
        "collector.station",
        "collector.station.degraded",
        Component::Collector,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "collector.driver",
        "collector.driver.failed",
        Component::Collector,
        EventLevel::Warn,
    ),
];

pub(crate) fn query_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
pub(crate) fn failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[3]
}
pub(crate) fn cancelled() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
pub(crate) fn driver_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[6]
}
