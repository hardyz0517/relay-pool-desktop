use bytes::Bytes;
use http::{HeaderMap, StatusCode};

use super::{
    decode_response_event, split_events, ProtocolFailure, ProtocolMachine, ProtocolProgress,
    ProtocolTerminal,
};

#[derive(Debug, Default)]
pub(crate) struct ChatSseMachine {
    pending: Vec<u8>,
    terminal: Option<ProtocolTerminal>,
}

impl ChatSseMachine {
    pub(crate) fn new() -> Self {
        Self::default()
    }
}

impl ProtocolMachine for ChatSseMachine {
    fn observe_headers(
        &mut self,
        status: StatusCode,
        _headers: &HeaderMap,
    ) -> Result<(), ProtocolFailure> {
        if !status.is_success() {
            return Err(ProtocolFailure {
                code: "upstream_http_error",
                detail: format!("chat SSE status {status}"),
            });
        }
        Ok(())
    }

    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<ProtocolProgress, ProtocolFailure> {
        if self.terminal.is_some() {
            return Err(ProtocolFailure {
                code: "protocol_terminal_already_seen",
                detail: "chat SSE received bytes after terminal".to_string(),
            });
        }
        self.pending.extend_from_slice(bytes);
        let mut progress = ProtocolProgress::Observed;
        for event in split_events(&mut self.pending) {
            let data = String::from_utf8_lossy(&event)
                .lines()
                .filter_map(|line| line.strip_prefix("data:"))
                .map(str::trim_start)
                .collect::<Vec<_>>()
                .join("\n");
            if data.trim() == "[DONE]" {
                self.terminal = Some(ProtocolTerminal::Completed);
                progress = ProtocolProgress::Terminal(ProtocolTerminal::Completed);
                break;
            }
            if !data.trim().is_empty() {
                decode_response_event(&event)?;
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
                detail: "chat SSE ended with a partial event".to_string(),
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
    fn chat_done_is_the_only_stream_success_terminal() {
        let mut machine = ChatSseMachine::new();
        machine
            .observe_headers(StatusCode::OK, &HeaderMap::new())
            .expect("headers");
        machine
            .observe_chunk(&Bytes::from_static(
                b"data: {\"choices\":[]}\n\ndata: [DONE]\n\n",
            ))
            .expect("events");
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Completed
        );
    }

    #[test]
    fn chat_clean_eof_without_done_is_incomplete() {
        let mut machine = ChatSseMachine::new();
        machine
            .observe_headers(StatusCode::OK, &HeaderMap::new())
            .expect("headers");
        machine
            .observe_chunk(&Bytes::from_static(b"data: {\"choices\":[]}\n\n"))
            .expect("event");
        assert_eq!(
            machine.finish_eof().expect("eof"),
            ProtocolTerminal::Incomplete
        );
    }

    #[test]
    fn chat_partial_event_is_failure() {
        let mut machine = ChatSseMachine::new();
        machine
            .observe_headers(StatusCode::OK, &HeaderMap::new())
            .expect("headers");
        machine
            .observe_chunk(&Bytes::from_static(b"data: {\"choices\":"))
            .expect("buffer partial event");
        let error = machine.finish_eof().expect_err("partial must fail");
        assert_eq!(error.code, "partial_protocol_event");
    }
}
