use bytes::Bytes;
use serde_json::Value;

use super::{
    json_event_value, DecodedSseEvent, ProtocolEvent, ProtocolEventKind, ProtocolFailure,
    ProtocolMachine, ProtocolProgress, ProtocolTerminal, SseEventDecoder,
};
use crate::services::proxy::diagnostic_memory::{DiagnosticMemoryBudget, DiagnosticMemoryPermit};

#[derive(Debug, Default)]
pub(crate) struct ChatSseMachine {
    decoder: SseEventDecoder,
    terminal: Option<ProtocolTerminal>,
}

impl ChatSseMachine {
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

impl ProtocolMachine for ChatSseMachine {
    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<ProtocolProgress, ProtocolFailure> {
        let _scratch = self.decoder.try_reserve_scratch(bytes.len())?;
        let decoded = self.decoder.push(bytes)?;
        let mut events = Vec::with_capacity(decoded.len());
        for event in decoded {
            let kind = classify_event(&event)?;
            if let Some(terminal) = self.terminal {
                if !allow_event_after_terminal(terminal, kind) {
                    return Err(terminal_already_seen());
                }
                continue;
            }
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
        self.decoder.finish_eof("chat")?;
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
    if event.event_name.as_deref() == Some("error")
        || value.get("type").and_then(Value::as_str) == Some("error")
        || value.get("error").is_some()
    {
        return Ok(ProtocolEventKind::Terminal(ProtocolTerminal::Failed));
    }

    if is_chat_control_event(&value) {
        Ok(ProtocolEventKind::Control)
    } else {
        Ok(ProtocolEventKind::Semantic)
    }
}

fn is_chat_control_event(value: &Value) -> bool {
    let Some(choices) = value.get("choices").and_then(Value::as_array) else {
        // Usage-only stream chunks and metadata are safe to retain until actual output.
        return value.get("usage").is_some();
    };
    if choices.is_empty() {
        return true;
    }
    let no_output = choices.iter().all(choice_has_no_output_delta);
    if no_output && value.get("usage").is_some() {
        return true;
    }
    no_output
        && choices
            .iter()
            .all(|choice| choice.get("finish_reason").is_none_or(Value::is_null))
}

fn choice_has_no_output_delta(choice: &Value) -> bool {
    choice
        .get("delta")
        .and_then(Value::as_object)
        .is_none_or(|delta| {
            delta.is_empty()
                || delta.iter().all(|(key, value)| match key.as_str() {
                    "role" | "name" => true,
                    "content" => value.as_str().is_some_and(str::is_empty),
                    _ => false,
                })
        })
}

fn allow_event_after_terminal(terminal: ProtocolTerminal, kind: ProtocolEventKind) -> bool {
    matches!(
        (terminal, kind),
        (
            ProtocolTerminal::Completed,
            ProtocolEventKind::Heartbeat | ProtocolEventKind::Control
        )
    )
}

fn terminal_already_seen() -> ProtocolFailure {
    ProtocolFailure {
        code: "protocol_terminal_already_seen",
        detail: "Chat SSE received an event after terminal".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn done_is_the_only_stream_success_terminal() {
        let mut machine = ChatSseMachine::new();
        let progress = machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"choices\":[]}\n\ndata: [DONE]\n\n",
            ))
            .expect("events");
        assert_eq!(progress.terminal(), Some(ProtocolTerminal::Completed));
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Completed
        );
    }

    #[test]
    fn clean_eof_without_done_is_incomplete() {
        let mut machine = ChatSseMachine::new();
        machine
            .observe_chunk(&Bytes::from_static(b"data: {\"choices\":[]}\n\n"))
            .expect("event");
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Incomplete
        );
    }

    #[test]
    fn partial_event_is_failure() {
        let mut machine = ChatSseMachine::new();
        machine
            .observe_chunk(&Bytes::from_static(b"data: {\"choices\":"))
            .expect("buffer partial event");
        let error = machine.finish_eof().expect_err("partial must fail");
        assert_eq!(error.code, "partial_protocol_event");
    }

    #[test]
    fn usage_only_chunk_after_done_is_retained() {
        let mut machine = ChatSseMachine::new();
        machine
            .observe_chunk(&Bytes::from_static(b"data: [DONE]\n\n"))
            .expect("done");
        let progress = machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
            ))
            .expect("trailing usage");
        assert_eq!(progress.terminal(), None);
        let with_finish_reason = machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
            ))
            .expect("usage with finish_reason after done");
        assert_eq!(with_finish_reason.terminal(), None);
        let with_empty_content = machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"choices\":[{\"delta\":{\"content\":\"\"},\"finish_reason\":\"stop\"}],\"usage\":{\"prompt_tokens\":3,\"completion_tokens\":1,\"total_tokens\":4}}\n\n",
            ))
            .expect("usage with empty content after done");
        assert_eq!(with_empty_content.terminal(), None);
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Completed
        );
    }

    #[test]
    fn invalid_utf8_event_cannot_be_rewritten_as_done() {
        let mut machine = ChatSseMachine::new();
        let error = machine
            .observe_chunk(&Bytes::from_static(b"data: [D\xFFONE]\n\n"))
            .expect_err("invalid UTF-8 must not become the done sentinel");
        assert_eq!(error.code, "invalid_protocol_utf8");
        assert_eq!(
            machine.finish_eof().expect("clean decoder state"),
            ProtocolTerminal::Incomplete
        );
    }
}
