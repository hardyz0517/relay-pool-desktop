use crate::observability::runtime::{
    descriptor::{
        EventDescriptor, Lifecycle, SamplingPolicy, CORE_DETAILS, CORE_OUTCOMES, CORE_SUBJECTS,
    },
    event::{Component, EventLevel},
};

pub(crate) const EVENT_DESCRIPTORS: &[EventDescriptor] = &[EventDescriptor {
    code: "ipc.runtime_context.invalid",
    owner: "ipc.runtime_context",
    event_schema_version: 1,
    detail_schema_version: 1,
    component: Component::Ipc,
    level: EventLevel::Warn,
    outcomes: CORE_OUTCOMES,
    details: CORE_DETAILS,
    subjects: CORE_SUBJECTS,
    sampling: SamplingPolicy::RateLimited,
    support_bundle: true,
    message_key: "ipc.runtime_context.invalid",
    lifecycle: Lifecycle::Active,
}];

pub(crate) fn runtime_context_invalid() -> &'static EventDescriptor {
    &EVENT_DESCRIPTORS[0]
}
