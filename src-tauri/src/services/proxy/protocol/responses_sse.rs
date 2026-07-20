use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use super::{
    decode_response_event, split_events, ProtocolFailure, ProtocolMachine, ProtocolProgress,
    ProtocolTerminal,
};

#[derive(Debug, Default)]
pub(crate) struct ResponsesSseMachine {
    pending: Vec<u8>,
    terminal: Option<ProtocolTerminal>,
}

impl ResponsesSseMachine {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn closed(&self) -> Result<(), ProtocolFailure> {
        if self.terminal.is_some() {
            Err(ProtocolFailure {
                code: "protocol_terminal_already_seen",
                detail: "response machine already has a terminal".to_string(),
            })
        } else {
            Ok(())
        }
    }
}

impl ProtocolMachine for ResponsesSseMachine {
    fn observe_headers(
        &mut self,
        status: StatusCode,
        _headers: &HeaderMap,
    ) -> Result<(), ProtocolFailure> {
        if !status.is_success() {
            return Err(ProtocolFailure {
                code: "upstream_http_error",
                detail: format!("responses SSE status {status}"),
            });
        }
        Ok(())
    }

    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<ProtocolProgress, ProtocolFailure> {
        self.closed()?;
        self.pending.extend_from_slice(bytes);
        let mut progress = ProtocolProgress::Observed;
        for event in split_events(&mut self.pending) {
            if let Some(terminal) = decode_response_event(&event)? {
                self.terminal = Some(terminal.clone());
                progress = ProtocolProgress::Terminal(terminal);
                break;
            }
        }
        Ok(progress)
    }

    fn finish_eof(&mut self) -> Result<ProtocolTerminal, ProtocolFailure> {
        if let Some(terminal) = self.terminal.clone() {
            return Ok(terminal);
        }
        if !self.pending.iter().all(u8::is_ascii_whitespace) {
            return Err(ProtocolFailure {
                code: "partial_protocol_event",
                detail: "responses SSE ended with a partial event".to_string(),
            });
        }
        self.terminal = Some(ProtocolTerminal::Incomplete);
        Ok(ProtocolTerminal::Incomplete)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::proxy::protocol::ProtocolTerminal;

    #[test]
    fn responses_completed_is_explicit_terminal() {
        let mut machine = ResponsesSseMachine::new();
        machine
            .observe_headers(StatusCode::OK, &HeaderMap::new())
            .expect("headers");
        let progress = machine
            .observe_chunk(&Bytes::from_static(
                br#"data: {"type":"response.completed"}

"#,
            ))
            .expect("event");
        assert_eq!(
            progress,
            ProtocolProgress::Terminal(ProtocolTerminal::Completed)
        );
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Completed
        );
    }

    #[test]
    fn responses_eof_without_terminal_is_incomplete() {
        let mut machine = ResponsesSseMachine::new();
        machine
            .observe_headers(StatusCode::OK, &HeaderMap::new())
            .expect("headers");
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
        machine
            .observe_headers(StatusCode::OK, &HeaderMap::new())
            .expect("headers");
        let error = machine
            .observe_chunk(&Bytes::from_static(b"data: {not-json}\n\n"))
            .expect_err("malformed must fail");
        assert_eq!(error.code, "malformed_protocol_event");
    }
}
