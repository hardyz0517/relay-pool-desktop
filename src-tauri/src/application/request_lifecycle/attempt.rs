use super::request::AttemptId;
use crate::application::health_protection::HealthProtectionScope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptContext {
    pub attempt_id: AttemptId,
    pub station_id: String,
    pub station_key_id: String,
    pub endpoint_revision: i64,
    pub credential_revision: i64,
    pub account_revision: i64,
    pub group_binding_id: Option<String>,
    pub group_revision: Option<i64>,
    pub resolved_upstream_model: Option<String>,
    /// Opaque commitment proving which comparable protocol/model/request
    /// shape crossed the outbound boundary. It never contains request data.
    pub comparability_key: Option<String>,
    pub model_alias_revision: i64,
    pub started_at_ms: i64,
    /// The exact durable Half-Open scope leased for this attempt.  The
    /// revision alone is only a fence and cannot identify Credential versus
    /// Endpoint probes when a request is finalized asynchronously.
    pub probe_scope: Option<HealthProtectionScope>,
    pub probe_state_revision: Option<u64>,
}

#[cfg(any(test, debug_assertions))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptPhase {
    Started,
    AwaitingHeaders,
    BootstrappingStream,
    Committed,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included integration contracts exercise disjoint failure blame variants"
    )
)]
pub(crate) enum FailureBlame {
    Upstream,
    Downstream,
    LocalAdapter,
    #[cfg(test)]
    Persistence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included integration contracts exercise disjoint failure kind variants"
    )
)]
pub(crate) enum AttemptFailureKind {
    Authentication,
    Balance,
    RateLimit,
    Connect,
    Timeout,
    HttpStatus,
    CapabilityMismatch,
    BadRequest,
    MalformedResponse,
    StreamInterrupted,
    LocalAdapter,
    DownstreamDrop,
    #[cfg(test)]
    Persistence,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included integration contracts exercise disjoint retry variants"
    )
)]
pub(crate) enum RetryDisposition {
    RetrySameTarget,
    TryNextCandidate,
    StopRequest,
}

/// Project the canonical retry intent into the legacy lifecycle record shape.
///
/// `RetryDisposition` is intentionally a compatibility projection: it records
/// whether the request lifecycle may continue, while the canonical retry
/// intent and the execution action carry the user-visible reason and replay
/// details. Keeping this conversion here gives every producer one owner and
/// prevents lifecycle records from becoming a second retry planner.
pub(crate) fn project_retry_disposition(
    retry: crate::application::request_finalization::failure::RetryDisposition,
) -> RetryDisposition {
    use crate::application::request_finalization::failure::RetryDisposition as CanonicalRetry;

    match retry {
        CanonicalRetry::TryNextKey => RetryDisposition::TryNextCandidate,
        CanonicalRetry::StopRequest => RetryDisposition::StopRequest,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included integration contracts exercise disjoint health effect variants"
    )
)]
pub(crate) enum HealthEffect {
    #[allow(
        dead_code,
        reason = "production finalization accepts success-classified failure records"
    )]
    Success,
    ObserveFailure,
    Cooldown {
        retry_after_ms: Option<i64>,
    },
    HardFail,
    Neutral,
    Scoped(DurableHealthEffect),
    Capability(DurableCapabilityEffect),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DurableHealthScope {
    Credential {
        station_key_id: String,
    },
    Account {
        station_id: String,
    },
    Group {
        station_id: String,
        group_binding_id: String,
    },
    Endpoint {
        station_id: String,
        endpoint_revision: i64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableFailureDimension {
    Credential,
    AccountLifecycle,
    GroupSubscription,
    Balance,
    Quota,
    RateLimit,
    EndpointAvailability,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DurableVerdict {
    Degraded,
    Cooldown { retry_after_ms: Option<i64> },
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DurableHealthEffect {
    pub scope: DurableHealthScope,
    pub dimension: DurableFailureDimension,
    pub verdict: DurableVerdict,
    pub evidence_code: String,
    pub classifier_profile_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DurableCapabilityEffect {
    ConfirmUnsupportedModel {
        station_key_id: String,
        model: String,
        evidence_code: String,
        classifier_profile_version: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ClassifiedAttemptFailure {
    pub kind: AttemptFailureKind,
    pub blame: FailureBlame,
    pub retry: RetryDisposition,
    pub health: HealthEffect,
    pub public_code: String,
    pub sanitized_detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttemptTerminal {
    Succeeded,
    Failed(ClassifiedAttemptFailure),
    #[allow(
        dead_code,
        reason = "production finalization preserves abandoned attempt records"
    )]
    Abandoned {
        reason: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptTerminalRecord {
    pub context: AttemptContext,
    pub terminal: AttemptTerminal,
    pub output_committed: bool,
    pub terminal_at_ms: i64,
    /// Copied from the context so terminal writers never have to reconstruct
    /// a probe identity from mutable candidate facts.
    pub probe_scope: Option<HealthProtectionScope>,
    pub probe_state_revision: Option<u64>,
}

#[cfg(any(test, debug_assertions))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttemptInvariantError {
    InvalidTransition {
        phase: AttemptPhase,
        event: &'static str,
    },
    AlreadyTerminal,
}

#[cfg(any(test, debug_assertions))]
#[derive(Debug)]
pub(crate) struct AttemptLifecycle {
    context: AttemptContext,
    phase: AttemptPhase,
    terminal: Option<AttemptTerminal>,
}

#[cfg(any(test, debug_assertions))]
impl AttemptLifecycle {
    pub(crate) fn new(context: AttemptContext) -> Self {
        Self {
            context,
            phase: AttemptPhase::Started,
            terminal: None,
        }
    }

    pub(crate) fn observe_headers(&mut self) -> Result<(), AttemptInvariantError> {
        match self.phase {
            AttemptPhase::Started => {
                self.phase = AttemptPhase::AwaitingHeaders;
                Ok(())
            }
            _ => Err(self.invalid("observe_headers")),
        }
    }

    pub(crate) fn begin_stream(&mut self) -> Result<(), AttemptInvariantError> {
        match self.phase {
            AttemptPhase::AwaitingHeaders => {
                self.phase = AttemptPhase::BootstrappingStream;
                Ok(())
            }
            _ => Err(self.invalid("begin_stream")),
        }
    }

    pub(crate) fn commit(&mut self) -> Result<(), AttemptInvariantError> {
        match self.phase {
            AttemptPhase::BootstrappingStream => {
                self.phase = AttemptPhase::Committed;
                Ok(())
            }
            _ => Err(self.invalid("commit")),
        }
    }

    pub(crate) fn terminalize(
        &mut self,
        terminal: AttemptTerminal,
    ) -> Result<AttemptTerminal, AttemptInvariantError> {
        if self.terminal.is_some() {
            return Err(AttemptInvariantError::AlreadyTerminal);
        }
        if matches!(self.phase, AttemptPhase::Terminal) {
            return Err(AttemptInvariantError::AlreadyTerminal);
        }
        self.phase = AttemptPhase::Terminal;
        self.terminal = Some(terminal.clone());
        Ok(terminal)
    }

    pub(crate) fn terminal_record(
        &self,
        output_committed: bool,
        terminal_at_ms: i64,
    ) -> Result<AttemptTerminalRecord, AttemptInvariantError> {
        let terminal = self
            .terminal
            .clone()
            .ok_or_else(|| self.invalid("terminal_record"))?;
        Ok(AttemptTerminalRecord {
            context: self.context.clone(),
            terminal,
            output_committed,
            terminal_at_ms,
            probe_scope: self.context.probe_scope.clone(),
            probe_state_revision: self.context.probe_state_revision,
        })
    }

    fn invalid(&self, event: &'static str) -> AttemptInvariantError {
        AttemptInvariantError::InvalidTransition {
            phase: self.phase.clone(),
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::application::request_finalization::failure::RetryDisposition as CanonicalRetry;

    fn attempt() -> AttemptLifecycle {
        AttemptLifecycle::new(AttemptContext {
            attempt_id: AttemptId::new("req-1", 0),
            station_id: "station-1".to_string(),
            station_key_id: "key-1".to_string(),
            endpoint_revision: 1,
            credential_revision: 1,
            account_revision: 1,
            group_binding_id: None,
            group_revision: None,
            resolved_upstream_model: None,
            comparability_key: None,
            model_alias_revision: 1,
            started_at_ms: 1,
            probe_scope: None,
            probe_state_revision: None,
        })
    }

    #[test]
    fn attempt_terminal_is_exactly_once() {
        let mut attempt = attempt();
        attempt.observe_headers().expect("headers");
        attempt.begin_stream().expect("stream");
        attempt.commit().expect("commit");
        attempt
            .terminalize(AttemptTerminal::Succeeded)
            .expect("terminal");
        let record = attempt.terminal_record(true, 2).expect("terminal record");
        assert_eq!(record.context.attempt_id, AttemptId::new("req-1", 0));

        assert!(matches!(
            attempt.terminalize(AttemptTerminal::Succeeded),
            Err(AttemptInvariantError::AlreadyTerminal)
        ));
    }

    #[test]
    fn pre_commit_failure_can_be_classified_without_health_retry_coupling() {
        let mut attempt = attempt();
        attempt.observe_headers().expect("headers");
        let failure = ClassifiedAttemptFailure {
            kind: AttemptFailureKind::Timeout,
            blame: FailureBlame::Upstream,
            retry: RetryDisposition::TryNextCandidate,
            health: HealthEffect::ObserveFailure,
            public_code: "upstream_timeout".to_string(),
            sanitized_detail: None,
        };
        attempt
            .terminalize(AttemptTerminal::Failed(failure.clone()))
            .expect("terminal");
        assert_eq!(failure.retry, RetryDisposition::TryNextCandidate);
        assert_eq!(failure.health, HealthEffect::ObserveFailure);
    }

    #[test]
    fn canonical_retry_projection_has_one_compatibility_mapping() {
        assert_eq!(
            project_retry_disposition(CanonicalRetry::TryNextKey),
            RetryDisposition::TryNextCandidate
        );
        assert_eq!(
            project_retry_disposition(CanonicalRetry::StopRequest),
            RetryDisposition::StopRequest
        );
    }
}
