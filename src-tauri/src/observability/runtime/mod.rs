pub(crate) mod bootstrap;
pub(crate) mod catalog;
pub(crate) mod clock;
pub(crate) mod crash;
pub(crate) mod descriptor;
pub(crate) mod error;
pub(crate) mod event;
pub(crate) mod lease;
pub(crate) mod reader;
pub(crate) mod recovery;
pub(crate) mod retention;
pub(crate) mod runtime_events;
pub(crate) mod service;
pub(crate) mod sink;
pub(crate) mod subject;

pub(crate) use catalog::Catalog;
pub(crate) use crash::{CrashMarker, PreviousSession};
pub(crate) use event::{
    Component, EventLevel, EventOutcome, RuntimeDetail, RuntimeEvent, RuntimePhase,
};
#[cfg(test)]
pub(crate) use event::{DetailKind, LeaseState};
pub(crate) use reader::RuntimeLogReader;
pub(crate) use service::{RuntimeLogService, RuntimeLogState};
pub(crate) use subject::{CorrelationIdRef, InteractionId, StableEventCode};

#[cfg(test)]
mod contract_tests;
