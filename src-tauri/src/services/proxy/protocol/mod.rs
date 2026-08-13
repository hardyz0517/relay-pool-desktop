use bytes::{Bytes, BytesMut};

use super::diagnostic_memory::{
    json_complexity, DiagnosticMemoryBudget, DiagnosticMemoryPermit, JsonComplexity,
    JSON_PARSER_SCRATCH_BYTES,
};

pub(crate) mod chat_sse;
pub(crate) mod responses_sse;

pub(crate) const MAX_PROTOCOL_EVENT_BYTES: usize = 256 * 1024;
pub(crate) const MAX_PROTOCOL_BOOTSTRAP_BYTES: usize = 256 * 1024;
pub(crate) const MAX_PROTOCOL_CHUNK_BYTES: usize = 256 * 1024;
pub(crate) const MAX_PROTOCOL_EVENTS_PER_CHUNK: usize = 4_096;
const SSE_RETAINED_MEMORY_UPPER_BOUND: usize =
    (MAX_PROTOCOL_BOOTSTRAP_BYTES + MAX_PROTOCOL_EVENT_BYTES) * 2;
// One input byte can delimit an empty SSE field/event and cause container
// metadata in addition to a retained raw byte. Forty bytes per input byte is a
// conservative bound for the decoded-event Vec/String/Bytes views. JSON Value
// construction is reserved separately by its frozen parser bound.
const SSE_TRANSIENT_BYTES_PER_INPUT_BYTE: usize = 40;

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
    #[expect(
        dead_code,
        reason = "contract=local-response-protocol; owner=services/proxy; remove_when=response protocol drops reserved variant"
    )]
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
    #[expect(
        dead_code,
        reason = "contract=local-response-protocol; owner=services/proxy; remove_when=response protocol drops reserved variant"
    )]
    LocalConstruction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ResponsePlan {
    pub transport: TransportMode,
    pub upstream_protocol: UpstreamProtocol,
    pub downstream_transform: DownstreamTransform,
    pub completion_policy: CompletionPolicy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtocolTerminal {
    Completed,
    Failed,
    Incomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProtocolEventKind {
    Heartbeat,
    Control,
    Semantic,
    Terminal(ProtocolTerminal),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolEvent {
    pub raw: Bytes,
    pub kind: ProtocolEventKind,
}

/// Extract the JSON `data:` payload from one complete SSE event.
///
/// The protocol machines deliberately preserve the raw event for downstream
/// passthrough.  Failure classification, however, must consume only the JSON
/// envelope carried by `data:` and must never attempt to parse the surrounding
/// SSE field syntax as JSON.
pub(crate) fn failure_event_json(event: &Bytes) -> Option<Bytes> {
    let decoded = decode_sse_event(event.clone()).ok()?;
    (!decoded.data.trim().is_empty() && decoded.data.trim() != "[DONE]")
        .then(|| Bytes::from(decoded.data))
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct ProtocolProgress {
    events: Vec<ProtocolEvent>,
}

impl ProtocolProgress {
    pub(crate) fn new(events: Vec<ProtocolEvent>) -> Self {
        Self { events }
    }

    pub(crate) fn into_events(self) -> Vec<ProtocolEvent> {
        self.events
    }

    pub(crate) fn terminal(&self) -> Option<ProtocolTerminal> {
        self.events.iter().find_map(|event| match event.kind {
            ProtocolEventKind::Terminal(terminal) => Some(terminal),
            _ => None,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProtocolFailure {
    pub code: &'static str,
    pub detail: String,
}

pub(crate) trait ProtocolMachine: Send {
    fn observe_chunk(&mut self, bytes: &Bytes) -> Result<ProtocolProgress, ProtocolFailure>;

    fn finish_eof(&mut self) -> Result<ProtocolTerminal, ProtocolFailure>;

    fn retained_bytes(&self) -> usize;

    fn discard_retained(&mut self);

    fn take_diagnostic_memory_permit(&mut self) -> Option<DiagnosticMemoryPermit> {
        None
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BootstrapDisposition {
    Pending,
    Emit {
        events: Vec<Bytes>,
        terminal: Option<ProtocolTerminal>,
    },
    PrecommitTerminal {
        terminal: ProtocolTerminal,
        event: Bytes,
    },
}

pub(crate) struct SseBootstrapMachine {
    protocol: Box<dyn ProtocolMachine>,
    // Preserve precommit control bytes contiguously. A `Vec<Bytes>` would let
    // an upstream inflate allocator metadata with thousands of tiny events
    // while staying below the raw-byte limit.
    buffered: BytesMut,
    committed: bool,
    terminal: Option<ProtocolTerminal>,
}

impl SseBootstrapMachine {
    pub(crate) fn new(protocol: Box<dyn ProtocolMachine>) -> Self {
        Self {
            protocol,
            buffered: BytesMut::new(),
            committed: false,
            terminal: None,
        }
    }

    pub(crate) fn for_completion_policy_with_diagnostic_memory(
        policy: CompletionPolicy,
        diagnostic_memory: DiagnosticMemoryBudget,
    ) -> Result<Option<Self>, ProtocolFailure> {
        match policy {
            CompletionPolicy::ResponsesTerminalEvent => Ok(Some(Self::new(Box::new(
                responses_sse::ResponsesSseMachine::with_diagnostic_memory(diagnostic_memory)?,
            )))),
            CompletionPolicy::ChatDoneSentinel => Ok(Some(Self::new(Box::new(
                chat_sse::ChatSseMachine::with_diagnostic_memory(diagnostic_memory)?,
            )))),
            CompletionPolicy::ValidatedJsonBody | CompletionPolicy::LocalConstruction => Ok(None),
        }
    }

    pub(crate) fn observe_chunk(
        &mut self,
        bytes: &Bytes,
    ) -> Result<BootstrapDisposition, ProtocolFailure> {
        if self.terminal.is_some() {
            return Err(ProtocolFailure {
                code: "protocol_terminal_already_seen",
                detail: "SSE bootstrap received bytes after terminal".to_string(),
            });
        }
        let progress = match self.protocol.observe_chunk(bytes) {
            Ok(progress) => progress,
            Err(failure) => {
                self.release_retained();
                return Err(failure);
            }
        };
        let mut emitted = Vec::new();
        let mut observed_terminal = None;

        for event in progress.into_events() {
            match event.kind {
                ProtocolEventKind::Heartbeat | ProtocolEventKind::Control if !self.committed => {
                    if self
                        .buffered
                        .len()
                        .checked_add(event.raw.len())
                        .is_none_or(|next| next > MAX_PROTOCOL_BOOTSTRAP_BYTES)
                    {
                        self.release_retained();
                        return Err(bootstrap_too_large());
                    }
                    self.buffered.extend_from_slice(&event.raw);
                }
                ProtocolEventKind::Semantic if !self.committed => {
                    self.committed = true;
                    self.flush_buffered(&mut emitted);
                    emitted.push(event.raw);
                }
                ProtocolEventKind::Terminal(ProtocolTerminal::Completed) if !self.committed => {
                    self.committed = true;
                    self.flush_buffered(&mut emitted);
                    emitted.push(event.raw);
                    observed_terminal = Some(ProtocolTerminal::Completed);
                }
                ProtocolEventKind::Terminal(terminal) if !self.committed => {
                    self.release_retained();
                    self.terminal = Some(terminal);
                    return Ok(BootstrapDisposition::PrecommitTerminal {
                        terminal,
                        event: event.raw,
                    });
                }
                ProtocolEventKind::Terminal(terminal) => {
                    emitted.push(event.raw);
                    observed_terminal = Some(terminal);
                }
                ProtocolEventKind::Heartbeat
                | ProtocolEventKind::Control
                | ProtocolEventKind::Semantic => emitted.push(event.raw),
            }
        }

        if self.retained_bytes() > MAX_PROTOCOL_BOOTSTRAP_BYTES {
            self.release_retained();
            return Err(bootstrap_too_large());
        }

        if let Some(terminal) = observed_terminal {
            self.terminal = Some(terminal);
        }
        if emitted.is_empty() {
            Ok(BootstrapDisposition::Pending)
        } else {
            Ok(BootstrapDisposition::Emit {
                events: emitted,
                terminal: observed_terminal,
            })
        }
    }

    pub(crate) fn finish_eof(&mut self) -> Result<BootstrapDisposition, ProtocolFailure> {
        let terminal = match self.protocol.finish_eof() {
            Ok(terminal) => terminal,
            Err(failure) => {
                self.release_retained();
                return Err(failure);
            }
        };
        if terminal == ProtocolTerminal::Completed {
            self.committed = true;
            self.terminal = Some(terminal);
            let mut events = Vec::with_capacity(1);
            self.flush_buffered(&mut events);
            return Ok(BootstrapDisposition::Emit {
                events,
                terminal: Some(terminal),
            });
        }
        if self.committed {
            self.terminal = Some(terminal);
            Ok(BootstrapDisposition::Emit {
                events: Vec::new(),
                terminal: Some(terminal),
            })
        } else {
            self.release_retained();
            self.terminal = Some(terminal);
            Ok(BootstrapDisposition::PrecommitTerminal {
                terminal,
                event: Bytes::new(),
            })
        }
    }

    pub(crate) fn take_diagnostic_memory_permit(&mut self) -> Option<DiagnosticMemoryPermit> {
        self.protocol.take_diagnostic_memory_permit()
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.buffered
            .len()
            .saturating_add(self.protocol.retained_bytes())
    }

    fn release_retained(&mut self) {
        self.buffered.clear();
        self.protocol.discard_retained();
    }

    fn flush_buffered(&mut self, emitted: &mut Vec<Bytes>) {
        if !self.buffered.is_empty() {
            emitted.push(std::mem::take(&mut self.buffered).freeze());
        }
    }
}

fn bootstrap_too_large() -> ProtocolFailure {
    ProtocolFailure {
        code: "protocol_bootstrap_too_large",
        detail: "SSE bootstrap exceeded the protocol buffer limit".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecodedSseEvent {
    pub raw: Bytes,
    pub event_name: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub(super) struct SseEventDecoder {
    pending: Vec<u8>,
    diagnostic_memory: Option<DiagnosticMemoryBudget>,
    _retained_memory: Option<DiagnosticMemoryPermit>,
}

impl SseEventDecoder {
    pub fn with_diagnostic_memory(
        diagnostic_memory: DiagnosticMemoryBudget,
    ) -> Result<Self, ProtocolFailure> {
        let retained_memory = diagnostic_memory
            .try_reserve(SSE_RETAINED_MEMORY_UPPER_BOUND)
            .map_err(|_| diagnostic_memory_saturated())?;
        Ok(Self {
            pending: Vec::new(),
            diagnostic_memory: Some(diagnostic_memory),
            _retained_memory: Some(retained_memory),
        })
    }

    pub fn from_retained_memory(retained_memory: DiagnosticMemoryPermit) -> Self {
        let diagnostic_memory = retained_memory.budget();
        Self {
            pending: Vec::new(),
            diagnostic_memory: Some(diagnostic_memory),
            _retained_memory: Some(retained_memory),
        }
    }

    pub fn push(&mut self, bytes: &Bytes) -> Result<Vec<DecodedSseEvent>, ProtocolFailure> {
        if bytes.len() > MAX_PROTOCOL_CHUNK_BYTES {
            self.pending.clear();
            // Preserve the more specific event contract when this oversized
            // transport chunk already contains an oversized complete event.
            // A delimiter-free oversized chunk remains a chunk-level failure
            // and is rejected before byte-wise retention.
            let oversized_complete_event = bytes
                .windows(2)
                .position(|window| window == b"\n\n")
                .map(|index| index + 2)
                .or_else(|| {
                    bytes
                        .windows(4)
                        .position(|window| window == b"\r\n\r\n")
                        .map(|index| index + 4)
                })
                .is_some_and(|event_bytes| event_bytes > MAX_PROTOCOL_EVENT_BYTES);
            let delimiter_free_event_would_overflow =
                !bytes.windows(2).any(|window| window == b"\n\n")
                    && !bytes.windows(4).any(|window| window == b"\r\n\r\n")
                    && self
                        .pending
                        .len()
                        .checked_add(bytes.len())
                        .is_none_or(|size| size > MAX_PROTOCOL_EVENT_BYTES);
            if oversized_complete_event || delimiter_free_event_would_overflow {
                return Err(event_too_large());
            }
            return Err(ProtocolFailure {
                code: "protocol_chunk_too_large",
                detail: "SSE input chunk exceeded the protocol limit".to_string(),
            });
        }
        let mut events = Vec::new();
        for byte in bytes {
            self.pending.push(*byte);
            if self.pending.ends_with(b"\n\n") || self.pending.ends_with(b"\r\n\r\n") {
                if self.pending.len() > MAX_PROTOCOL_EVENT_BYTES {
                    self.pending.clear();
                    return Err(event_too_large());
                }
                let raw = Bytes::from(std::mem::take(&mut self.pending));
                events.push(decode_sse_event(raw)?);
                if events.len() > MAX_PROTOCOL_EVENTS_PER_CHUNK {
                    self.pending.clear();
                    return Err(ProtocolFailure {
                        code: "too_many_protocol_events",
                        detail: "SSE input chunk contained too many events".to_string(),
                    });
                }
            } else if self.pending.len() > MAX_PROTOCOL_EVENT_BYTES {
                self.pending.clear();
                return Err(event_too_large());
            }
        }
        Ok(events)
    }

    pub fn try_reserve_scratch(
        &self,
        input_bytes: usize,
    ) -> Result<Option<DiagnosticMemoryPermit>, ProtocolFailure> {
        self.diagnostic_memory
            .as_ref()
            .map(|budget| {
                let transient = input_bytes
                    .checked_mul(SSE_TRANSIENT_BYTES_PER_INPUT_BYTE)
                    .and_then(|bytes| bytes.checked_add(JSON_PARSER_SCRATCH_BYTES))
                    .ok_or_else(diagnostic_memory_saturated)?;
                budget
                    .try_reserve(transient)
                    .map_err(|_| diagnostic_memory_saturated())
            })
            .transpose()
    }

    pub fn finish_eof(&self, protocol: &'static str) -> Result<(), ProtocolFailure> {
        if self.pending.iter().all(u8::is_ascii_whitespace) {
            Ok(())
        } else {
            Err(ProtocolFailure {
                code: "partial_protocol_event",
                detail: format!("{protocol} SSE ended with a partial event"),
            })
        }
    }

    pub fn retained_bytes(&self) -> usize {
        self.pending.len()
    }

    pub fn clear(&mut self) {
        self.pending.clear();
    }

    pub fn take_diagnostic_memory_permit(&mut self) -> Option<DiagnosticMemoryPermit> {
        self._retained_memory.take()
    }
}

fn event_too_large() -> ProtocolFailure {
    ProtocolFailure {
        code: "protocol_event_too_large",
        detail: "SSE event exceeded the protocol buffer limit".to_string(),
    }
}

fn diagnostic_memory_saturated() -> ProtocolFailure {
    ProtocolFailure {
        code: "diagnostic_memory_saturated",
        detail: "proxy diagnostic memory admission is saturated".to_string(),
    }
}

fn decode_sse_event(raw: Bytes) -> Result<DecodedSseEvent, ProtocolFailure> {
    let text = std::str::from_utf8(&raw).map_err(|_| ProtocolFailure {
        code: "invalid_protocol_utf8",
        detail: "SSE event contained invalid UTF-8".to_string(),
    })?;
    let mut event_name = None;
    let mut data = Vec::new();
    for line in text.lines() {
        if line.starts_with(':') {
            continue;
        }
        if let Some(value) = line.strip_prefix("event:") {
            event_name = Some(value.trim_start().to_string());
        } else if let Some(value) = line.strip_prefix("data:") {
            data.push(value.trim_start().to_string());
        }
    }
    Ok(DecodedSseEvent {
        raw,
        event_name,
        data: data.join("\n"),
    })
}

pub(super) fn json_event_value(
    event: &DecodedSseEvent,
) -> Result<serde_json::Value, ProtocolFailure> {
    match json_complexity(event.data.as_bytes()) {
        JsonComplexity::WithinLimit => {}
        JsonComplexity::TooDeep => {
            return Err(ProtocolFailure {
                code: "protocol_json_too_deep",
                detail: "SSE JSON exceeded the nesting limit".to_string(),
            })
        }
        JsonComplexity::TooComplex => {
            return Err(ProtocolFailure {
                code: "protocol_json_too_complex",
                detail: "SSE JSON exceeded the structural node limit".to_string(),
            })
        }
    }
    serde_json::from_str(&event.data).map_err(|error| ProtocolFailure {
        code: "malformed_protocol_event",
        detail: error.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decoder_supports_lf_crlf_and_partial_boundaries() {
        let mut decoder = SseEventDecoder::default();
        let first = decoder
            .push(&Bytes::from_static(b"data: one\n\ndata: tw"))
            .expect("first chunk");
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].data, "one");
        let second = decoder
            .push(&Bytes::from_static(b"o\r\n\r\n"))
            .expect("second chunk");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].data, "two");
        assert_eq!(decoder.retained_bytes(), 0);
    }

    #[test]
    fn every_two_chunk_boundary_preserves_responses_and_chat_terminals() {
        let cases = [
            (
                CompletionPolicy::ResponsesTerminalEvent,
                b"event: response.completed\r\ndata: {\"type\":\"response.completed\"}\r\n\r\n"
                    .as_slice(),
            ),
            (
                CompletionPolicy::ChatDoneSentinel,
                b"data: [DONE]\n\n".as_slice(),
            ),
        ];

        for (policy, event) in cases {
            for split_at in 0..=event.len() {
                let mut bootstrap = SseBootstrapMachine::for_completion_policy_with_diagnostic_memory(
                    policy,
                    DiagnosticMemoryBudget::new(
                        crate::services::proxy::diagnostic_memory::DEFAULT_DIAGNOSTIC_MEMORY_LIMIT_BYTES,
                    ),
                )
                .expect("bootstrap")
                .expect("streaming policy");
                let first = bootstrap
                    .observe_chunk(&Bytes::copy_from_slice(&event[..split_at]))
                    .expect("first chunk");
                let first_completed = matches!(
                    first,
                    BootstrapDisposition::Emit {
                        terminal: Some(ProtocolTerminal::Completed),
                        ..
                    }
                );
                if first_completed {
                    assert_eq!(
                        split_at,
                        event.len(),
                        "terminal must not be recognized before all wire bytes arrive for {policy:?}"
                    );
                } else {
                    assert!(
                        matches!(first, BootstrapDisposition::Pending),
                        "policy {policy:?}, split {split_at}"
                    );
                    assert!(matches!(
                        bootstrap
                            .observe_chunk(&Bytes::copy_from_slice(&event[split_at..]))
                            .expect("second chunk"),
                        BootstrapDisposition::Emit {
                            terminal: Some(ProtocolTerminal::Completed),
                            ..
                        }
                    ));
                }
                assert!(
                    matches!(
                        bootstrap.finish_eof().expect("terminal eof"),
                        BootstrapDisposition::Emit {
                            terminal: Some(ProtocolTerminal::Completed),
                            ..
                        }
                    ),
                    "policy {policy:?}, split {split_at}"
                );
            }
        }
    }

    #[test]
    fn failure_event_json_returns_only_joined_data_fields() {
        let json = failure_event_json(&Bytes::from_static(
            b"event: error\r\ndata: {\"error\":\r\ndata: {\"code\":\"server_error\"}}\r\n\r\n",
        ))
        .expect("JSON payload");
        assert_eq!(
            json,
            Bytes::from_static(b"{\"error\":\n{\"code\":\"server_error\"}}")
        );
        assert!(failure_event_json(&Bytes::from_static(b"data: [DONE]\n\n")).is_none());
    }

    #[test]
    fn sse_decoder_reserves_and_releases_shared_retention_and_scratch() {
        let retained_bound = SSE_RETAINED_MEMORY_UPPER_BOUND;
        let parser_bound = JSON_PARSER_SCRATCH_BYTES;
        let transient = 16 * SSE_TRANSIENT_BYTES_PER_INPUT_BYTE + parser_bound;
        let budget = DiagnosticMemoryBudget::new(retained_bound + transient);
        let decoder = SseEventDecoder::with_diagnostic_memory(budget.clone()).expect("decoder");
        assert_eq!(budget.retained(), retained_bound);
        let scratch = decoder
            .try_reserve_scratch(16)
            .expect("scratch")
            .expect("permit");
        assert_eq!(budget.retained(), retained_bound + transient);
        drop(scratch);
        assert_eq!(budget.retained(), retained_bound);
        drop(decoder);
        assert_eq!(budget.retained(), 0);
    }

    #[test]
    fn sse_decoder_fails_fast_before_allocating_when_shared_budget_is_full() {
        let budget = DiagnosticMemoryBudget::new(SSE_RETAINED_MEMORY_UPPER_BOUND - 1);
        let failure = SseEventDecoder::with_diagnostic_memory(budget.clone())
            .expect_err("retention admission must fail closed");
        assert_eq!(failure.code, "diagnostic_memory_saturated");
        assert_eq!(budget.retained(), 0);
    }

    #[test]
    fn one_hundred_sse_bootstraps_share_the_32_mib_budget_and_release_on_drop() {
        use std::{
            sync::{Arc, Barrier},
            thread,
        };

        const WORKERS: usize = 100;
        let budget = DiagnosticMemoryBudget::new(32 * 1024 * 1024);
        let rendezvous = Arc::new(Barrier::new(WORKERS + 1));
        let mut workers = Vec::with_capacity(WORKERS);

        for _ in 0..WORKERS {
            let budget = budget.clone();
            let rendezvous = Arc::clone(&rendezvous);
            workers.push(thread::spawn(move || {
                let bootstrap = SseBootstrapMachine::for_completion_policy_with_diagnostic_memory(
                    CompletionPolicy::ResponsesTerminalEvent,
                    budget,
                );
                let admitted = matches!(bootstrap, Ok(Some(_)));
                rendezvous.wait();
                rendezvous.wait();
                admitted
            }));
        }

        rendezvous.wait();
        assert_eq!(budget.retained(), budget.limit());
        assert_eq!(budget.retained(), 32 * 1024 * 1024);
        rendezvous.wait();

        let admitted = workers
            .into_iter()
            .map(|worker| worker.join().expect("SSE bootstrap worker"))
            .filter(|admitted| *admitted)
            .count();
        assert_eq!(
            admitted, 32,
            "each bootstrap reserves a 1 MiB hard upper bound before parsing"
        );
        assert_eq!(
            budget.retained(),
            0,
            "dropping admitted bootstraps releases memory"
        );
    }

    #[test]
    fn one_hundred_sse_decoders_bound_retention_and_parser_scratch_together() {
        use std::{
            sync::{Arc, Barrier},
            thread,
        };

        const WORKERS: usize = 100;
        const INPUT_BYTES: usize = 1;
        let scratch_bytes = JSON_PARSER_SCRATCH_BYTES + SSE_TRANSIENT_BYTES_PER_INPUT_BYTE;
        let per_worker = SSE_RETAINED_MEMORY_UPPER_BOUND + scratch_bytes;
        let budget = DiagnosticMemoryBudget::new(32 * 1024 * 1024);
        let rendezvous = Arc::new(Barrier::new(WORKERS + 1));
        let mut workers = Vec::with_capacity(WORKERS);

        for _ in 0..WORKERS {
            let budget = budget.clone();
            let rendezvous = Arc::clone(&rendezvous);
            workers.push(thread::spawn(move || {
                let decoder = SseEventDecoder::with_diagnostic_memory(budget);
                let admitted = decoder
                    .as_ref()
                    .ok()
                    .and_then(|decoder| decoder.try_reserve_scratch(INPUT_BYTES).ok())
                    .flatten();
                rendezvous.wait();
                rendezvous.wait();
                admitted.is_some()
            }));
        }

        rendezvous.wait();
        assert!(budget.retained() <= budget.limit());
        rendezvous.wait();

        let admitted = workers
            .into_iter()
            .map(|worker| worker.join().expect("SSE decoder worker"))
            .filter(|admitted| *admitted)
            .count();
        assert_eq!(admitted, budget.limit() / per_worker);
        assert_eq!(
            budget.retained(),
            0,
            "decoder and scratch permits both release"
        );
    }

    #[test]
    fn retained_permit_keeps_committed_decoder_on_the_same_shared_budget() {
        let transient = JSON_PARSER_SCRATCH_BYTES + SSE_TRANSIENT_BYTES_PER_INPUT_BYTE;
        let budget = DiagnosticMemoryBudget::new(SSE_RETAINED_MEMORY_UPPER_BOUND + transient);
        let mut bootstrap_decoder =
            SseEventDecoder::with_diagnostic_memory(budget.clone()).expect("bootstrap decoder");
        let retained = bootstrap_decoder
            .take_diagnostic_memory_permit()
            .expect("retained permit");
        drop(bootstrap_decoder);

        let committed_decoder = SseEventDecoder::from_retained_memory(retained);
        assert_eq!(budget.retained(), SSE_RETAINED_MEMORY_UPPER_BOUND);
        let scratch = committed_decoder
            .try_reserve_scratch(1)
            .expect("committed scratch")
            .expect("shared permit");
        assert_eq!(
            budget.retained(),
            SSE_RETAINED_MEMORY_UPPER_BOUND + transient
        );
        drop(scratch);
        drop(committed_decoder);
        assert_eq!(budget.retained(), 0);
    }

    #[test]
    fn decoder_rejects_oversized_chunks_before_iterating_or_retaining_them() {
        let mut decoder = SseEventDecoder::default();
        let failure = decoder
            .push(&Bytes::from(vec![b'x'; MAX_PROTOCOL_CHUNK_BYTES + 1]))
            .expect_err("oversized input chunk");
        assert_eq!(failure.code, "protocol_event_too_large");
        assert_eq!(decoder.retained_bytes(), 0);
    }

    #[test]
    fn json_structure_is_gated_before_serde_allocation() {
        let too_deep = DecodedSseEvent {
            raw: Bytes::new(),
            event_name: None,
            data: format!(
                "{}0{}",
                "[".repeat(super::super::diagnostic_memory::MAX_DIAGNOSTIC_JSON_DEPTH + 1),
                "]".repeat(super::super::diagnostic_memory::MAX_DIAGNOSTIC_JSON_DEPTH + 1)
            ),
        };
        let failure = json_event_value(&too_deep).expect_err("depth gate");
        assert_eq!(failure.code, "protocol_json_too_deep");
    }
}
