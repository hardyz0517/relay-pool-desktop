use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[standard_descriptor(
    "outbound.transport",
    "outbound.request.failed",
    Component::Outbound,
    EventLevel::Warn,
)];

pub(crate) fn request_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
