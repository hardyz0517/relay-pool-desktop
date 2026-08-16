use crate::observability::runtime::{
    descriptor::{standard_descriptor, EventDescriptor},
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[
    standard_descriptor(
        "persistence.runtime",
        "persistence.installation_lease.acquired",
        Component::Persistence,
        EventLevel::Info,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.installation_lease.contended",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.installation_lease.acquire_failed",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.installation_lease.released",
        Component::Persistence,
        EventLevel::Info,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.installation_lease.release_failed",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.device_key.recovery_required",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.relocation.recovery_required",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.runtime.close_failed",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.startup.plan_recovery_required",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.startup.probe_recovery_required",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.startup.recovery_required",
        Component::Persistence,
        EventLevel::Warn,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.database.initialized",
        Component::Persistence,
        EventLevel::Info,
    ),
    standard_descriptor(
        "persistence.runtime",
        "persistence.recovery_mode.started",
        Component::Persistence,
        EventLevel::Warn,
    ),
];

pub(crate) fn installation_lease_acquired() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
pub(crate) fn installation_lease_contended() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[1]
}
pub(crate) fn installation_lease_acquire_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[2]
}
pub(crate) fn installation_lease_released() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[3]
}
pub(crate) fn installation_lease_release_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[4]
}
pub(crate) fn device_key_recovery_required() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[5]
}
pub(crate) fn relocation_recovery_required() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[6]
}
pub(crate) fn runtime_close_failed() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[7]
}
pub(crate) fn startup_plan_recovery_required() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[8]
}
pub(crate) fn startup_probe_recovery_required() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[9]
}
pub(crate) fn startup_recovery_required() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[10]
}
pub(crate) fn database_initialized() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[11]
}
pub(crate) fn recovery_mode_started() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[12]
}
