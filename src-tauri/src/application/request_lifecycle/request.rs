use super::delivery::DeliveryTerminal;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct AttemptId {
    pub request_id: String,
    pub ordinal: u16,
}

impl AttemptId {
    pub(crate) fn new(request_id: impl Into<String>, ordinal: u16) -> Self {
        Self {
            request_id: request_id.into(),
            ordinal,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestContextSnapshot {
    pub request_id: String,
    pub method: String,
    pub local_path: String,
    pub endpoint: String,
    pub received_at_ms: i64,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestPhase {
    Accepted,
    Admitted,
    Routing,
    Attempting { ordinal: u16 },
    Committed { attempt_id: AttemptId },
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included integration contracts exercise disjoint request terminal variants"
    )
)]
pub(crate) enum RequestTerminal {
    Completed(RequestCompletion),
    PartialSuccess(RequestCompletion),
    Failed(RequestFailure),
    Interrupted(DeliveryFailure),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestCompletion {
    pub protocol_completed: bool,
    pub attempt_id: Option<AttemptId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestFailure {
    pub code: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DeliveryFailure {
    pub terminal: DeliveryTerminal,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequestTerminalSnapshot {
    pub terminal: RequestTerminal,
    pub delivery: DeliveryTerminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "the domain-only integration contract does not enqueue request start records"
    )
)]
pub(crate) struct RequestStartRecord {
    pub context: RequestContextSnapshot,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct RequestLogAnnotations {
    pub model: Option<String>,
    pub stream: bool,
    pub selected_station_key_id: Option<String>,
    pub selected_station_id: Option<String>,
    pub upstream_base_url: Option<String>,
    pub route_policy: Option<String>,
    pub route_reason: Option<String>,
    pub rejected_candidates_json: Option<String>,
    pub body_bytes: Option<i64>,
    pub route_wait_ms: Option<i64>,
    pub upstream_headers_ms: Option<i64>,
    pub failure_source: Option<String>,
    pub attempts_json: Option<String>,
    pub completion_source: Option<String>,
    pub prompt_tokens: Option<i64>,
    pub completion_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
    pub reasoning_effort: Option<String>,
    pub first_token_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FinalRequestRecord {
    pub context: RequestContextSnapshot,
    pub terminal: RequestTerminalSnapshot,
    pub selected_attempt_id: Option<AttemptId>,
    pub attempt_count: u16,
    pub fallback_count: u16,
    pub annotations: RequestLogAnnotations,
}

impl FinalRequestRecord {
    pub(crate) fn new(
        context: RequestContextSnapshot,
        terminal: RequestTerminalSnapshot,
        selected_attempt_id: Option<AttemptId>,
        attempt_count: u16,
        fallback_count: u16,
        annotations: RequestLogAnnotations,
    ) -> Self {
        Self {
            context,
            terminal,
            selected_attempt_id,
            attempt_count,
            fallback_count,
            annotations,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included lifecycle contracts do not all exercise response-body finalization"
    )
)]
pub(crate) struct PendingFinalRequestRecord {
    context: RequestContextSnapshot,
    selected_attempt_id: Option<AttemptId>,
    attempt_count: u16,
    fallback_count: u16,
    annotations: RequestLogAnnotations,
}

#[cfg_attr(
    test,
    allow(
        dead_code,
        reason = "path-included lifecycle contracts do not all exercise response-body finalization"
    )
)]
impl PendingFinalRequestRecord {
    pub(crate) fn new(
        context: RequestContextSnapshot,
        selected_attempt_id: Option<AttemptId>,
        attempt_count: u16,
        fallback_count: u16,
        annotations: RequestLogAnnotations,
    ) -> Self {
        Self {
            context,
            selected_attempt_id,
            attempt_count,
            fallback_count,
            annotations,
        }
    }

    pub(crate) fn context(&self) -> &RequestContextSnapshot {
        &self.context
    }

    pub(crate) fn annotations(&self) -> &RequestLogAnnotations {
        &self.annotations
    }

    pub(crate) fn annotations_mut(&mut self) -> &mut RequestLogAnnotations {
        &mut self.annotations
    }

    pub(crate) fn complete(self, delivery: DeliveryTerminal) -> FinalRequestRecord {
        let completion = RequestCompletion {
            protocol_completed: true,
            attempt_id: self.selected_attempt_id.clone(),
        };
        let terminal = if self.fallback_count > 0 {
            RequestTerminal::PartialSuccess(completion)
        } else {
            RequestTerminal::Completed(completion)
        };
        self.into_record(terminal, delivery)
    }

    pub(crate) fn fail(
        self,
        code: impl Into<String>,
        detail: Option<String>,
        delivery: DeliveryTerminal,
    ) -> FinalRequestRecord {
        self.into_record(
            RequestTerminal::Failed(RequestFailure {
                code: code.into(),
                detail,
            }),
            delivery,
        )
    }

    pub(crate) fn interrupt(
        self,
        delivery: DeliveryTerminal,
        detail: Option<String>,
    ) -> FinalRequestRecord {
        self.into_record(
            RequestTerminal::Interrupted(DeliveryFailure {
                terminal: delivery,
                detail,
            }),
            delivery,
        )
    }

    fn into_record(
        self,
        terminal: RequestTerminal,
        delivery: DeliveryTerminal,
    ) -> FinalRequestRecord {
        FinalRequestRecord::new(
            self.context,
            RequestTerminalSnapshot { terminal, delivery },
            self.selected_attempt_id,
            self.attempt_count,
            self.fallback_count,
            self.annotations,
        )
    }
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RequestInvariantError {
    InvalidTransition {
        phase: RequestPhase,
        event: &'static str,
    },
    AlreadyTerminal,
}

#[cfg(test)]
#[derive(Debug)]
pub(crate) struct RequestLifecycle {
    context: RequestContextSnapshot,
    phase: RequestPhase,
    attempt_count: u16,
    fallback_count: u16,
    selected_attempt_id: Option<AttemptId>,
    terminal_record: Option<FinalRequestRecord>,
}

#[cfg(test)]
impl RequestLifecycle {
    pub(crate) fn new(context: RequestContextSnapshot) -> Self {
        Self {
            context,
            phase: RequestPhase::Accepted,
            attempt_count: 0,
            fallback_count: 0,
            selected_attempt_id: None,
            terminal_record: None,
        }
    }

    pub(crate) fn admit(&mut self) -> Result<(), RequestInvariantError> {
        match &self.phase {
            RequestPhase::Accepted => {
                self.phase = RequestPhase::Admitted;
                Ok(())
            }
            _ => Err(self.invalid("admit")),
        }
    }

    pub(crate) fn start_routing(&mut self) -> Result<(), RequestInvariantError> {
        match &self.phase {
            RequestPhase::Admitted => {
                self.phase = RequestPhase::Routing;
                Ok(())
            }
            _ => Err(self.invalid("start_routing")),
        }
    }

    pub(crate) fn start_attempt(&mut self, ordinal: u16) -> Result<(), RequestInvariantError> {
        match &self.phase {
            RequestPhase::Routing | RequestPhase::Attempting { .. } => {
                self.attempt_count = self.attempt_count.max(ordinal.saturating_add(1));
                self.phase = RequestPhase::Attempting { ordinal };
                Ok(())
            }
            _ => Err(self.invalid("start_attempt")),
        }
    }

    pub(crate) fn commit(&mut self, attempt_id: AttemptId) -> Result<(), RequestInvariantError> {
        match &self.phase {
            RequestPhase::Attempting { .. } => {
                self.selected_attempt_id = Some(attempt_id.clone());
                self.phase = RequestPhase::Committed { attempt_id };
                Ok(())
            }
            _ => Err(self.invalid("commit")),
        }
    }

    pub(crate) fn note_fallback(&mut self) -> Result<(), RequestInvariantError> {
        match &self.phase {
            RequestPhase::Attempting { .. } => {
                self.fallback_count = self.fallback_count.saturating_add(1);
                Ok(())
            }
            _ => Err(self.invalid("note_fallback")),
        }
    }

    pub(crate) fn terminalize(
        &mut self,
        terminal: RequestTerminal,
        delivery: DeliveryTerminal,
    ) -> Result<FinalRequestRecord, RequestInvariantError> {
        if self.terminal_record.is_some() {
            return Err(RequestInvariantError::AlreadyTerminal);
        }
        if matches!(&self.phase, RequestPhase::Accepted) {
            return Err(self.invalid("terminalize"));
        }

        let record = FinalRequestRecord::new(
            self.context.clone(),
            RequestTerminalSnapshot { terminal, delivery },
            self.selected_attempt_id.clone(),
            self.attempt_count,
            self.fallback_count,
            RequestLogAnnotations::default(),
        );
        self.phase = RequestPhase::Terminal;
        self.terminal_record = Some(record.clone());
        Ok(record)
    }

    pub(crate) fn terminal_record(&self) -> Option<&FinalRequestRecord> {
        self.terminal_record.as_ref()
    }

    fn invalid(&self, event: &'static str) -> RequestInvariantError {
        RequestInvariantError::InvalidTransition {
            phase: self.phase.clone(),
            event,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> RequestContextSnapshot {
        RequestContextSnapshot {
            request_id: "req-1".to_string(),
            method: "POST".to_string(),
            local_path: "/v1/chat/completions".to_string(),
            endpoint: "chat_completions".to_string(),
            received_at_ms: 1,
        }
    }

    #[test]
    fn request_has_one_terminal_record_and_preserves_attempt_count() {
        let mut lifecycle = RequestLifecycle::new(context());
        lifecycle.admit().expect("admission");
        lifecycle.start_routing().expect("routing");
        lifecycle.start_attempt(0).expect("attempt a");
        lifecycle.note_fallback().expect("fallback");
        lifecycle.start_attempt(1).expect("attempt b");
        let attempt_id = AttemptId::new("req-1", 1);
        lifecycle.commit(attempt_id.clone()).expect("commit");
        let record = lifecycle
            .terminalize(
                RequestTerminal::Completed(RequestCompletion {
                    protocol_completed: true,
                    attempt_id: Some(attempt_id.clone()),
                }),
                DeliveryTerminal::BodyCompleted,
            )
            .expect("terminal");

        assert_eq!(record.attempt_count, 2);
        assert_eq!(record.fallback_count, 1);
        assert_eq!(record.selected_attempt_id, Some(attempt_id));
        assert_eq!(lifecycle.terminal_record(), Some(&record));
        assert!(matches!(
            lifecycle.terminalize(
                RequestTerminal::Failed(RequestFailure {
                    code: "duplicate".to_string(),
                    detail: None,
                }),
                DeliveryTerminal::BodyCompleted,
            ),
            Err(RequestInvariantError::AlreadyTerminal)
        ));
    }

    #[test]
    fn accepted_request_cannot_be_terminalized_without_admission() {
        let mut lifecycle = RequestLifecycle::new(context());
        let error = lifecycle
            .terminalize(
                RequestTerminal::Failed(RequestFailure {
                    code: "invalid".to_string(),
                    detail: None,
                }),
                DeliveryTerminal::NotStarted,
            )
            .expect_err("accepted must not silently finalize");
        assert!(matches!(
            error,
            RequestInvariantError::InvalidTransition {
                event: "terminalize",
                ..
            }
        ));
    }

    #[test]
    fn admitted_request_can_terminalize_before_routing() {
        let mut lifecycle = RequestLifecycle::new(context());
        lifecycle.admit().expect("admission");
        let record = lifecycle
            .terminalize(
                RequestTerminal::Failed(RequestFailure {
                    code: "invalid_body".to_string(),
                    detail: None,
                }),
                DeliveryTerminal::NotStarted,
            )
            .expect("terminal");
        assert_eq!(record.attempt_count, 0);
    }
}
