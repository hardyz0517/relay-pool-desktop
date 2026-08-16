use serde::{Deserialize, Serialize};

use super::{
    event::{Component, DetailKind, EventLevel, EventOutcome},
    subject::SubjectKind,
};

pub(crate) const CORE_OUTCOMES: &[EventOutcome] = &[
    EventOutcome::Ok,
    EventOutcome::Error,
    EventOutcome::Cancelled,
    EventOutcome::Timeout,
    EventOutcome::Overloaded,
    EventOutcome::Degraded,
];
pub(crate) const CORE_DETAILS: &[DetailKind] = &[
    DetailKind::None,
    DetailKind::Redacted,
    DetailKind::Phase,
    DetailKind::Retry,
    DetailKind::Queue,
    DetailKind::Clock,
    DetailKind::Recovery,
    DetailKind::Lease,
    DetailKind::Boundary,
];
pub(crate) const CORE_SUBJECTS: &[SubjectKind] = &[
    SubjectKind::None,
    SubjectKind::Installation,
    SubjectKind::Session,
    SubjectKind::Interaction,
    SubjectKind::Operation,
    SubjectKind::Task,
    SubjectKind::Command,
    SubjectKind::Station,
    SubjectKind::Provider,
    SubjectKind::Resource,
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SamplingPolicy {
    Always,
    Default,
    DebugOnly,
    RateLimited,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "status")]
pub(crate) enum Lifecycle {
    Active,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "contract=runtime.catalog-deprecated-lifecycle; owner=observability/runtime; remove_when=historical manifest compatibility no longer accepts replacement chains"
        )
    )]
    Deprecated {
        replaced_by: &'static str,
        sunset_version: u16,
    },
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct EventDescriptor {
    pub(crate) code: &'static str,
    pub(crate) owner: &'static str,
    pub(crate) event_schema_version: u16,
    pub(crate) detail_schema_version: u16,
    pub(crate) component: Component,
    pub(crate) level: EventLevel,
    pub(crate) outcomes: &'static [EventOutcome],
    pub(crate) details: &'static [DetailKind],
    pub(crate) subjects: &'static [SubjectKind],
    pub(crate) sampling: SamplingPolicy,
    pub(crate) support_bundle: bool,
    pub(crate) message_key: &'static str,
    pub(crate) lifecycle: Lifecycle,
}

pub(crate) const fn standard_descriptor(
    owner: &'static str,
    code: &'static str,
    component: Component,
    level: EventLevel,
) -> EventDescriptor {
    EventDescriptor {
        code,
        owner,
        event_schema_version: super::event::RUNTIME_EVENT_SCHEMA_VERSION,
        detail_schema_version: 1,
        component,
        level,
        outcomes: CORE_OUTCOMES,
        details: CORE_DETAILS,
        subjects: CORE_SUBJECTS,
        sampling: SamplingPolicy::Always,
        support_bundle: true,
        message_key: code,
        lifecycle: Lifecycle::Active,
    }
}
