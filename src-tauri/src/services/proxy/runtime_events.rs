use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.persistence_retry",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.persistence_failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.start_started",
        Component::Proxy,
        EventLevel::Info,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.start_succeeded",
        Component::Proxy,
        EventLevel::Info,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.start_failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.already_running",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.stop_succeeded",
        Component::Proxy,
        EventLevel::Info,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.stop_failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.drain_succeeded",
        Component::Proxy,
        EventLevel::Info,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.drain_failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.lifecycle.drain_timeout",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "proxy.startup.auto_start_failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "routing.projection.tick_failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.runtime",
        "routing.snapshot.failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "proxy.upstream",
        "proxy.upstream.failed",
        Component::Proxy,
        EventLevel::Warn,
    ),
];

pub(crate) fn persistence_retry() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
pub(crate) fn persistence_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[1]
}
pub(crate) fn lifecycle_start_started() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[2]
}
pub(crate) fn lifecycle_start_succeeded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[3]
}
pub(crate) fn lifecycle_start_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
pub(crate) fn lifecycle_already_running() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[5]
}
pub(crate) fn lifecycle_stop_succeeded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[6]
}
pub(crate) fn lifecycle_stop_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[7]
}
pub(crate) fn lifecycle_drain_succeeded() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[8]
}
pub(crate) fn lifecycle_drain_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[9]
}
pub(crate) fn lifecycle_drain_timeout() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[10]
}
pub(crate) fn startup_auto_start_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[11]
}
pub(crate) fn routing_projection_tick_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[12]
}
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=routing.snapshot.failed; owner=services/proxy; remove_when=all routing snapshot failures use the production diagnostics adapter"
    )
)]
pub(crate) fn planning_snapshot_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[13]
}
pub(crate) fn upstream_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[14]
}
