use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[standard_descriptor(
    "frontend.shell",
    "frontend.boundary.failed",
    Component::Frontend,
    EventLevel::Error,
)];

pub(crate) fn frontend_boundary_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
