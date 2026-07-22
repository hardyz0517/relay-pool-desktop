#[cfg(test)]
use bytes::Bytes;
#[cfg(test)]
use http::{HeaderMap, StatusCode};
#[cfg(test)]
use serde_json::Value;

// These protocol machines are exercised as contract fixtures; production streaming
// completion is owned by the response-body finalization path.
#[cfg(test)]
pub(crate) mod chat_sse;
#[cfg(test)]
pub(crate) mod responses_sse;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TransportMode {
    Buffered,
    Streaming,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UpstreamProtocol {
    ResponsesJson,
    ResponsesSse,
    ChatCompletionsJson,
    ChatCompletionsSse,
    EmbeddingsJson,
    ModelsJson,
    #[expect(dead_code, reason = "reserved by the local-response protocol contract")]
    LocalJson,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DownstreamTransform {
    Passthrough,
    ChatToResponses,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionPolicy {
    ValidatedJsonBody,
    ResponsesTerminalEvent,
    ChatDoneSentinel,
    #[expect(dead_code, reason = "reserved by the local-response protocol contract")]
    LocalConstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponsePlan {
    pub transport: TransportMode,
    pub upstream_protocol: UpstreamProtocol,
    pub downstream_transform: DownstreamTransform,
    pub completion_policy: CompletionPolicy,
}

// Shared types for the contract-only protocol machines above.
#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProtocolTerminal {
    Completed,
    Failed,
    Incomplete,
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProtocolProgress {
    Observed,
    Terminal(ProtocolTerminal),
}

#[cfg(test)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolFailure {
    pub code: &'static str,
    pub detail: String,
}

#[cfg(test)]
pub(crate) trait ProtocolMachine: Send {
    fn observe_headers(
        &mut self,
        status: StatusCode,
        headers: &HeaderMap,
    ) -> Result<(), ProtocolFailure>;

    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<ProtocolProgress, ProtocolFailure>;

    fn finish_eof(&mut self) -> Result<ProtocolTerminal, ProtocolFailure>;
}

#[cfg(test)]
fn event_data(event: &[u8]) -> String {
    String::from_utf8_lossy(event)
        .lines()
        .filter_map(|line| line.strip_prefix("data:"))
        .map(str::trim_start)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
fn terminal_from_json(value: &Value) -> Option<ProtocolTerminal> {
    match value.get("type").and_then(Value::as_str) {
        Some("response.completed") => Some(ProtocolTerminal::Completed),
        Some("response.failed") => Some(ProtocolTerminal::Failed),
        Some("response.incomplete") => Some(ProtocolTerminal::Incomplete),
        _ => None,
    }
}

#[cfg(test)]
fn split_sse_events(pending: &mut Vec<u8>) -> Vec<Vec<u8>> {
    let mut events = Vec::new();
    loop {
        let lf = pending
            .windows(2)
            .position(|window| window == b"\n\n")
            .map(|index| (index, 2));
        let crlf = pending
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .map(|index| (index, 4));
        let boundary = match (lf, crlf) {
            (Some(left), Some(right)) => Some(if left.0 <= right.0 { left } else { right }),
            (Some(found), None) | (None, Some(found)) => Some(found),
            (None, None) => None,
        };
        let Some((index, delimiter_len)) = boundary else {
            break;
        };
        events.push(pending[..index].to_vec());
        pending.drain(..index + delimiter_len);
    }
    events
}

#[cfg(test)]
pub(crate) fn decode_response_event(
    event: &[u8],
) -> Result<Option<ProtocolTerminal>, ProtocolFailure> {
    let data = event_data(event);
    if data.trim().is_empty() {
        return Ok(None);
    }
    let value = serde_json::from_str::<Value>(&data).map_err(|error| ProtocolFailure {
        code: "malformed_protocol_event",
        detail: error.to_string(),
    })?;
    Ok(terminal_from_json(&value))
}

#[cfg(test)]
pub(crate) fn split_events(pending: &mut Vec<u8>) -> Vec<Vec<u8>> {
    split_sse_events(pending)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_split_supports_lf_and_crlf_boundaries() {
        let mut pending = b"data: one\n\ndata: two\r\n\r\npartial".to_vec();
        let events = split_events(&mut pending);
        assert_eq!(events.len(), 2);
        assert_eq!(pending, b"partial");
    }
}
