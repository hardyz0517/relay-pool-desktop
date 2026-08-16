use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "app.lifecycle",
        "app.exit.coordinator_unavailable",
        Component::App,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.bootstrap.started",
        Component::App,
        EventLevel::Info,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.shutdown.started",
        Component::App,
        EventLevel::Info,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.restart.requested",
        Component::App,
        EventLevel::Info,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.previous_session_unclean_exit",
        Component::App,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.exit.drain_timeout",
        Component::App,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.shutdown.blocking_failed",
        Component::App,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.shutdown.persistence_drain_failed",
        Component::App,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.shutdown.persistence_failed",
        Component::App,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "app.lifecycle",
        "app.shutdown.supervisor_failed",
        Component::App,
        EventLevel::Warn,
    ),
];

pub(crate) fn exit_coordinator_unavailable() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
pub(crate) fn bootstrap_started() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[1]
}
pub(crate) fn shutdown_started() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[2]
}
pub(crate) fn restart_requested() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[3]
}
pub(crate) fn previous_session_unclean_exit() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
pub(crate) fn exit_drain_timeout() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[5]
}
pub(crate) fn shutdown_blocking_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[6]
}
pub(crate) fn shutdown_persistence_drain_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[7]
}
pub(crate) fn shutdown_persistence_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[8]
}
pub(crate) fn shutdown_supervisor_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[9]
}
