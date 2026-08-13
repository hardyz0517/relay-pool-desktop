use bytes::Bytes;
use serde_json::Value;

use super::{
    json_event_value, DecodedSseEvent, ProtocolEvent, ProtocolEventKind, ProtocolFailure,
    ProtocolMachine, ProtocolProgress, ProtocolTerminal, SseEventDecoder,
};
use crate::services::proxy::diagnostic_memory::{DiagnosticMemoryBudget, DiagnosticMemoryPermit};

#[derive(Debug, Default)]
pub(crate) struct ResponsesSseMachine {
    decoder: SseEventDecoder,
    terminal: Option<ProtocolTerminal>,
}

impl ResponsesSseMachine {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_diagnostic_memory(
        diagnostic_memory: DiagnosticMemoryBudget,
    ) -> Result<Self, ProtocolFailure> {
        Ok(Self {
            decoder: SseEventDecoder::with_diagnostic_memory(diagnostic_memory)?,
            terminal: None,
        })
    }

    pub(crate) fn from_retained_memory(retained_memory: DiagnosticMemoryPermit) -> Self {
        Self {
            decoder: SseEventDecoder::from_retained_memory(retained_memory),
            terminal: None,
        }
    }
}

impl ProtocolMachine for ResponsesSseMachine {
    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<ProtocolProgress, ProtocolFailure> {
        if self.terminal.is_some() && !bytes.is_empty() {
            return Err(terminal_already_seen());
        }

        let _scratch = self.decoder.try_reserve_scratch(bytes.len())?;
        let decoded = self.decoder.push(bytes)?;
        let mut events = Vec::with_capacity(decoded.len());
        for event in decoded {
            if self.terminal.is_some() {
                return Err(terminal_already_seen());
            }
            let kind = classify_event(&event)?;
            if let ProtocolEventKind::Terminal(terminal) = kind {
                self.terminal = Some(terminal);
            }
            events.push(ProtocolEvent {
                raw: event.raw,
                kind,
            });
        }
        Ok(ProtocolProgress::new(events))
    }

    fn finish_eof(&mut self) -> Result<ProtocolTerminal, ProtocolFailure> {
        if let Some(terminal) = self.terminal {
            return Ok(terminal);
        }
        self.decoder.finish_eof("responses")?;
        self.terminal = Some(ProtocolTerminal::Incomplete);
        Ok(ProtocolTerminal::Incomplete)
    }

    fn retained_bytes(&self) -> usize {
        self.decoder.retained_bytes()
    }

    fn discard_retained(&mut self) {
        self.decoder.clear();
    }

    fn take_diagnostic_memory_permit(
        &mut self,
    ) -> Option<crate::services::proxy::diagnostic_memory::DiagnosticMemoryPermit> {
        self.decoder.take_diagnostic_memory_permit()
    }
}

fn classify_event(event: &DecodedSseEvent) -> Result<ProtocolEventKind, ProtocolFailure> {
    if event.data.trim().is_empty()
        || matches!(event.event_name.as_deref(), Some("ping" | "heartbeat"))
    {
        return Ok(ProtocolEventKind::Heartbeat);
    }
    if event.data.trim() == "[DONE]" {
        return Ok(ProtocolEventKind::Terminal(ProtocolTerminal::Completed));
    }
    let value = json_event_value(event)?;
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .or(event.event_name.as_deref());

    if event.event_name.as_deref() == Some("error")
        || event_type == Some("error")
        || value.get("error").is_some()
    {
        return Ok(ProtocolEventKind::Terminal(ProtocolTerminal::Failed));
    }

    match event_type {
        Some("response.completed") => Ok(ProtocolEventKind::Terminal(ProtocolTerminal::Completed)),
        Some("response.failed") => Ok(ProtocolEventKind::Terminal(ProtocolTerminal::Failed)),
        Some("response.incomplete") => {
            Ok(ProtocolEventKind::Terminal(ProtocolTerminal::Incomplete))
        }
        Some("response.created" | "response.in_progress" | "response.queued") => {
            Ok(ProtocolEventKind::Control)
        }
        Some(
            "response.output_text.delta"
            | "response.refusal.delta"
            | "response.function_call_arguments.delta"
            | "response.reasoning_text.delta"
            | "response.reasoning_summary_text.delta"
            | "response.output_item.added",
        ) => Ok(ProtocolEventKind::Semantic),
        // A syntactically valid event unknown to this profile may be visible to a newer
        // client. Commit and preserve it rather than silently discarding it before retry.
        _ => Ok(ProtocolEventKind::Semantic),
    }
}

fn terminal_already_seen() -> ProtocolFailure {
    ProtocolFailure {
        code: "protocol_terminal_already_seen",
        detail: "Responses SSE received an event after terminal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_is_an_explicit_empty_success_terminal() {
        let mut machine = ResponsesSseMachine::new();
        let progress = machine
            .observe_chunk(&Bytes::from_static(
                br#"data: {"type":"response.completed","response":{"output":[]}}

"#,
            ))
            .expect("event");
        assert_eq!(progress.terminal(), Some(ProtocolTerminal::Completed));
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Completed
        );
    }

    #[test]
    fn eof_without_terminal_is_incomplete() {
        let mut machine = ResponsesSseMachine::new();
        machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"type\":\"response.output_text.delta\"}\n\n",
            ))
            .expect("event");
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Incomplete
        );
    }

    #[test]
    fn malformed_event_is_not_success() {
        let mut machine = ResponsesSseMachine::new();
        let error = machine
            .observe_chunk(&Bytes::from_static(b"data: {not-json}\n\n"))
            .expect_err("malformed must fail");
        assert_eq!(error.code, "malformed_protocol_event");
    }

    #[test]
    fn invalid_utf8_event_fails_before_it_can_commit_or_terminalize() {
        let mut machine = ResponsesSseMachine::new();
        let error = machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"type\":\"response.completed\",\"padding\":\"\xFF\"}\n\n",
            ))
            .expect_err("invalid UTF-8 must not become a terminal event");
        assert_eq!(error.code, "invalid_protocol_utf8");
        assert_eq!(
            machine.finish_eof().expect("clean decoder state"),
            ProtocolTerminal::Incomplete
        );
    }
}
