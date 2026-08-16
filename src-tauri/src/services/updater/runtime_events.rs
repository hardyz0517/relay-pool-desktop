use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[standard_descriptor(
    "updater.manifests",
    "updater.manifest.inspect_failed",
    Component::Migration,
    EventLevel::Warn,
)];

pub(crate) fn manifest_inspect_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
