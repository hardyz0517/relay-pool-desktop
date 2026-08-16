use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "migration.portable",
        "migration.portable.recovery_required",
        Component::Migration,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "migration.portable",
        "migration.portable.export_failed",
        Component::Migration,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "migration.portable",
        "migration.portable.import_failed",
        Component::Migration,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "migration.portable",
        "migration.portable.inspect_failed",
        Component::Migration,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "migration.portable",
        "migration.portable.prepare_failed",
        Component::Migration,
        EventLevel::Warn,
    ),
];

pub(crate) fn recovery_required() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
pub(crate) fn export_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[1]
}
pub(crate) fn inspect_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[3]
}
pub(crate) fn prepare_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
