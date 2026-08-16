use serde::{de::Error as DeError, Deserialize, Deserializer, Serialize};

use super::{
    clock::Elapsed,
    error::RuntimeError,
    subject::{
        CorrelationIdRef, InteractionId, OperationId, SessionId, StableEventCode, SubjectRef,
    },
};

pub(crate) const RUNTIME_EVENT_SCHEMA_VERSION: u16 = 1;
pub(crate) const MAX_SERIALIZED_EVENT_BYTES: usize = 16 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventLevel {
    Error,
    Warn,
    Info,
    Debug,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EventOutcome {
    Ok,
    Error,
    Cancelled,
    Timeout,
    Overloaded,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Component {
    App,
    Ipc,
    Persistence,
    Proxy,
    Outbound,
    Collector,
    Monitoring,
    Operation,
    Migration,
    Frontend,
    Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum DetailKind {
    None,
    Redacted,
    Phase,
    Retry,
    Queue,
    Clock,
    Recovery,
    Lease,
    Boundary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RedactionReason {
    ExternalText,
    SecretMaterial,
    UnknownError,
    Payload,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RuntimePhase {
    Bootstrap,
    Startup,
    Shutdown,
    Recovery,
    Dispatch,
    Persist,
    Transport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum QueueAction {
    Accepted,
    Dropped,
    Rejected,
    Drained,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum LeaseState {
    Acquired,
    Unavailable,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum BoundaryAction {
    Started,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub(crate) enum RuntimeDetail {
    None,
    Redacted {
        reason: RedactionReason,
    },
    Phase {
        phase: RuntimePhase,
    },
    Retry {
        attempt: u16,
        max_attempts: u16,
    },
    Queue {
        action: QueueAction,
        depth: u32,
    },
    Clock {
        adjustment: super::clock::ClockAdjustment,
    },
    Recovery {
        recovered_events: u32,
    },
    Lease {
        state: LeaseState,
    },
    Boundary {
        action: BoundaryAction,
    },
}

impl RuntimeDetail {
    pub(crate) fn kind(self) -> DetailKind {
        match self {
            Self::None => DetailKind::None,
            Self::Redacted { .. } => DetailKind::Redacted,
            Self::Phase { .. } => DetailKind::Phase,
            Self::Retry { .. } => DetailKind::Retry,
            Self::Queue { .. } => DetailKind::Queue,
            Self::Clock { .. } => DetailKind::Clock,
            Self::Recovery { .. } => DetailKind::Recovery,
            Self::Lease { .. } => DetailKind::Lease,
            Self::Boundary { .. } => DetailKind::Boundary,
        }
    }

    fn validate(self) -> Result<(), EventValidationError> {
        if let Self::Retry {
            attempt,
            max_attempts,
        } = self
        {
            if max_attempts == 0 || attempt == 0 || attempt > max_attempts {
                return Err(EventValidationError::InvalidDetail);
            }
        }
        if let Self::Queue { depth, .. } = self {
            if depth > 1_000_000 {
                return Err(EventValidationError::InvalidDetail);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RuntimeEvent {
    pub(crate) schema_version: u16,
    pub(crate) at_ms: i64,
    pub(crate) sequence: u64,
    pub(crate) level: EventLevel,
    pub(crate) event_code: StableEventCode,
    pub(crate) component: Component,
    pub(crate) outcome: EventOutcome,
    pub(crate) session_id: SessionId,
    pub(crate) correlation_id: Option<CorrelationIdRef>,
    pub(crate) interaction_id: Option<InteractionId>,
    pub(crate) operation_id: Option<OperationId>,
    pub(crate) subject: Option<SubjectRef>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) error: Option<RuntimeError>,
    pub(crate) detail: RuntimeDetail,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
struct RuntimeEventWire {
    schema_version: u16,
    at_ms: i64,
    sequence: u64,
    level: EventLevel,
    event_code: StableEventCode,
    component: Component,
    outcome: EventOutcome,
    session_id: SessionId,
    correlation_id: Option<CorrelationIdRef>,
    interaction_id: Option<InteractionId>,
    operation_id: Option<OperationId>,
    subject: Option<SubjectRef>,
    duration_ms: Option<u64>,
    error: Option<RuntimeError>,
    detail: RuntimeDetail,
}

impl<'de> Deserialize<'de> for RuntimeEvent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let wire = RuntimeEventWire::deserialize(deserializer)?;
        let event = Self {
            schema_version: wire.schema_version,
            at_ms: wire.at_ms,
            sequence: wire.sequence,
            level: wire.level,
            event_code: wire.event_code,
            component: wire.component,
            outcome: wire.outcome,
            session_id: wire.session_id,
            correlation_id: wire.correlation_id,
            interaction_id: wire.interaction_id,
            operation_id: wire.operation_id,
            subject: wire.subject,
            duration_ms: wire.duration_ms,
            error: wire.error,
            detail: wire.detail,
        };
        event.validate().map_err(D::Error::custom)?;
        Ok(event)
    }
}

impl RuntimeEvent {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        at_ms: i64,
        sequence: u64,
        level: EventLevel,
        event_code: StableEventCode,
        component: Component,
        outcome: EventOutcome,
        session_id: SessionId,
        correlation_id: Option<CorrelationIdRef>,
        interaction_id: Option<InteractionId>,
        operation_id: Option<OperationId>,
        subject: Option<SubjectRef>,
        duration: Option<Elapsed>,
        error: Option<RuntimeError>,
        detail: RuntimeDetail,
    ) -> Result<Self, EventValidationError> {
        let event = Self {
            schema_version: RUNTIME_EVENT_SCHEMA_VERSION,
            at_ms,
            sequence,
            level,
            event_code,
            component,
            outcome,
            session_id,
            correlation_id,
            interaction_id,
            operation_id,
            subject,
            duration_ms: duration.map(Elapsed::as_millis),
            error,
            detail,
        };
        event.validate()?;
        Ok(event)
    }

    pub(crate) fn validate(&self) -> Result<(), EventValidationError> {
        if self.schema_version != RUNTIME_EVENT_SCHEMA_VERSION
            || self.event_code.as_str().len() > super::subject::MAX_STABLE_CODE_BYTES
        {
            return Err(EventValidationError::UnsupportedSchema);
        }
        self.detail.validate()?;
        if self.outcome == EventOutcome::Error && self.error.is_none() {
            return Err(EventValidationError::MissingError);
        }
        if self.outcome != EventOutcome::Error && self.error.is_some() {
            return Err(EventValidationError::UnexpectedError);
        }
        Ok(())
    }

    pub(crate) fn to_json_line(&self) -> Result<String, EventValidationError> {
        self.validate()?;
        let json = serde_json::to_string(self).map_err(|_| EventValidationError::Serialization)?;
        if json.len() + 1 > MAX_SERIALIZED_EVENT_BYTES {
            return Err(EventValidationError::TooLarge);
        }
        Ok(format!("{json}\n"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EventValidationError {
    UnsupportedSchema,
    InvalidDetail,
    MissingError,
    UnexpectedError,
    Serialization,
    TooLarge,
}

impl std::fmt::Display for EventValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::UnsupportedSchema => "unsupported runtime event schema",
            Self::InvalidDetail => "invalid runtime event detail",
            Self::MissingError => "failed runtime event requires a typed error",
            Self::UnexpectedError => "non-failed runtime event cannot carry an error",
            Self::Serialization => "runtime event serialization failed",
            Self::TooLarge => "runtime event exceeds the 16 KiB line limit",
        })
    }
}

impl std::error::Error for EventValidationError {}
