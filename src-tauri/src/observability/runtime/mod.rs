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

pub(crate) use catalog::{Catalog, CatalogError, CatalogManifest};
#[cfg(test)]
pub(crate) use clock::MonotonicTimer;
pub(crate) use clock::{ClockAdjustment, ClockGuard, ClockObservation, Elapsed};
pub(crate) use crash::{CrashMarker, PreviousSession};
pub(crate) use descriptor::{
    standard_descriptor, EventDescriptor, Lifecycle, SamplingPolicy, CORE_DETAILS, CORE_OUTCOMES,
    CORE_SUBJECTS,
};
pub(crate) use error::{DataDisposition, RuntimeError};
pub(crate) use event::{
    Component, DetailKind, EventLevel, EventOutcome, EventValidationError, LeaseState,
    RuntimeDetail, RuntimeEvent, RuntimePhase, RUNTIME_EVENT_SCHEMA_VERSION,
};
pub(crate) use reader::RuntimeLogReader;
pub(crate) use service::{RuntimeLogService, RuntimeLogSnapshot, RuntimeLogState};
pub(crate) use subject::{
    CorrelationIdRef, InteractionId, OperationId, RedactedResourceId, SessionId, StableEventCode,
    SubjectKind, SubjectRef,
};

#[cfg(test)]
mod contract_tests;
