use super::request::AttemptId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AttemptContext {
    pub attempt_id: AttemptId,
    pub station_id: String,
    pub station_key_id: String,
    pub endpoint_revision: i64,
    pub started_at_ms: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptPhase {
    Started,
    AwaitingHeaders,
    BootstrappingStream,
    Committed,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    TryNextCandidate,
    StopRequest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AttemptInvariantError {
    InvalidTransition {
        phase: AttemptPhase,
        event: &'static str,
    },
    AlreadyTerminal,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct AttemptLifecycle {
    context: AttemptContext,
    phase: AttemptPhase,
    terminal: Option<AttemptTerminal>,
}

#[cfg(test)]
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
        })
    }

    fn invalid(&self, event: &'static str) -> AttemptInvariantError {
        AttemptInvariantError::InvalidTransition {
            phase: self.phase,
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attempt() -> AttemptLifecycle {
        AttemptLifecycle::new(AttemptContext {
            attempt_id: AttemptId::new("req-1", 0),
            station_id: "station-1".to_string(),
            station_key_id: "key-1".to_string(),
            endpoint_revision: 1,
            started_at_ms: 1,
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
}
