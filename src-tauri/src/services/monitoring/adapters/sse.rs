use serde_json::Value;

use crate::{
    models::monitoring::{FailureKind, ProtocolKind},
    services::monitoring::{
        adapters::contract::{
            extract_text_fields, validate_output_text, ParsedProbeResponse, ResponseLimits,
        },
        challenge::ChallengeValidator,
    },
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SseEvent {
    pub data: String,
}

#[derive(Debug)]
pub struct SseParser {
    protocol_kind: ProtocolKind,
    limits: ResponseLimits,
    buffer: Vec<u8>,
    response_bytes: usize,
    output_text: String,
    event_count: usize,
    completed: bool,
    failure_kind: Option<FailureKind>,
}

impl SseParser {
    pub fn new(protocol_kind: ProtocolKind, limits: ResponseLimits) -> Self {
        Self {
            protocol_kind,
            limits,
            buffer: Vec::new(),
            response_bytes: 0,
            output_text: String::new(),
            event_count: 0,
            completed: false,
            failure_kind: None,
        }
    }

    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, FailureKind> {
        self.response_bytes = self.response_bytes.saturating_add(chunk.len());
        if self.response_bytes > self.limits.max_response_bytes {
            self.failure_kind = Some(FailureKind::ProtocolMismatch);
            return Err(FailureKind::ProtocolMismatch);
        }
        self.buffer.extend_from_slice(chunk);

        let mut events = Vec::new();
        while let Some(end) = find_event_end(&self.buffer) {
            let raw = self.buffer.drain(..end.consumed).collect::<Vec<_>>();
            let event_bytes = &raw[..end.event_len];
            let event = parse_event(event_bytes)?;
            self.event_count += 1;
            if self.event_count > self.limits.max_sse_events {
                self.failure_kind = Some(FailureKind::ProtocolMismatch);
                return Err(FailureKind::ProtocolMismatch);
            }
            self.consume_event(&event)?;
            events.push(event);
        }
        Ok(events)
    }

    pub fn finish(&self, validator: &ChallengeValidator) -> ParsedProbeResponse {
        if let Some(failure_kind) = self.failure_kind {
            return ParsedProbeResponse::unavailable(
                self.protocol_kind,
                Some(200),
                failure_kind,
                self.response_bytes,
            );
        }
        if !self.completed {
            return ParsedProbeResponse::unavailable(
                self.protocol_kind,
                Some(200),
                FailureKind::ProtocolMismatch,
                self.response_bytes,
            );
        }
        validate_output_text(
            self.protocol_kind,
            Some(200),
            self.output_text.clone(),
            self.response_bytes,
            validator,
            self.limits,
        )
    }

    fn consume_event(&mut self, event: &SseEvent) -> Result<(), FailureKind> {
        let data = event.data.trim();
        if data == "[DONE]" {
            self.completed = true;
            return Ok(());
        }
        let value =
            serde_json::from_str::<Value>(data).map_err(|_| FailureKind::ProtocolMismatch)?;
        if value.get("error").is_some() {
            self.failure_kind = Some(FailureKind::ProtocolMismatch);
            return Err(FailureKind::ProtocolMismatch);
        }
        self.output_text.push_str(&extract_text_fields(&value));
        if self.output_text.len() > self.limits.max_output_bytes {
            self.failure_kind = Some(FailureKind::ProtocolMismatch);
            return Err(FailureKind::ProtocolMismatch);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
struct EventEnd {
    event_len: usize,
    consumed: usize,
}

fn find_event_end(buffer: &[u8]) -> Option<EventEnd> {
    for index in 0..buffer.len().saturating_sub(1) {
        if buffer[index] == b'\n' && buffer[index + 1] == b'\n' {
            return Some(EventEnd {
                event_len: index,
                consumed: index + 2,
            });
        }
        if index + 3 < buffer.len()
            && buffer[index] == b'\r'
            && buffer[index + 1] == b'\n'
            && buffer[index + 2] == b'\r'
            && buffer[index + 3] == b'\n'
        {
            return Some(EventEnd {
                event_len: index,
                consumed: index + 4,
            });
        }
    }
    None
}

fn parse_event(bytes: &[u8]) -> Result<SseEvent, FailureKind> {
    let raw = std::str::from_utf8(bytes).map_err(|_| FailureKind::ProtocolMismatch)?;
    let data = raw
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n");
    if data.is_empty() {
        return Err(FailureKind::ProtocolMismatch);
    }
    Ok(SseEvent { data })
}

#[cfg(test)]
mod tests {
    use crate::{
        models::monitoring::{ProbeOutcome, ProtocolKind},
        services::monitoring::{
            adapters::{contract::ResponseLimits, sse::SseParser},
            challenge::ChallengeValidator,
        },
    };

    #[test]
    fn services_monitoring_sse_requires_done_terminal_event() {
        let mut parser = SseParser::new(ProtocolKind::OpenAiChat, ResponseLimits::default());
        parser
            .push(
                br#"data: {"delta":"RP_ANSWER=42"}

"#,
            )
            .expect("chunk");

        let parsed = parser.finish(&ChallengeValidator::from_expected_answer_for_tests(
            "RP_ANSWER=42",
        ));
        assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
    }
}
