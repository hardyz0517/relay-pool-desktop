use std::str;

/// Resource limits for one SSE response body.
///
/// The limits deliberately protect different resources. In particular,
/// `max_pending_event_bytes` is never used as a limit for the total stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SseLimits {
    pub max_pending_event_bytes: usize,
    pub max_total_stream_bytes: usize,
    pub max_sse_events: usize,
}

impl Default for SseLimits {
    fn default() -> Self {
        Self {
            max_pending_event_bytes: 64 * 1024,
            // Responses streams can legitimately contain substantial typed
            // reasoning/event traffic. These remain bounded well below the
            // outbound client's 8 MiB transport ceiling while avoiding a
            // second false rejection for normal verbose streams.
            max_total_stream_bytes: 2 * 1024 * 1024,
            max_sse_events: 4_096,
        }
    }
}

/// Stable, safe-to-persist reasons produced by stream framing and reducers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StreamError {
    ResponseBodyLimit,
    SseEventLimit,
    SseEventTooLarge,
    InvalidSseUtf8,
    InvalidSseFraming,
    InvalidEventJson,
    UnexpectedContentType,
    MissingTerminalEvent,
    UpstreamFailedEvent,
    UpstreamIncompleteEvent,
    ContentValidationFailed,
    OutputLimit,
}

impl StreamError {
    pub(crate) const fn as_code(self) -> &'static str {
        match self {
            Self::ResponseBodyLimit => "response_body_limit",
            Self::SseEventLimit => "sse_event_limit",
            Self::SseEventTooLarge => "sse_event_too_large",
            Self::InvalidSseUtf8 => "invalid_sse_utf8",
            Self::InvalidSseFraming => "invalid_sse_framing",
            Self::InvalidEventJson => "invalid_event_json",
            Self::UnexpectedContentType => "unexpected_content_type",
            Self::MissingTerminalEvent => "missing_terminal_event",
            Self::UpstreamFailedEvent => "upstream_failed_event",
            Self::UpstreamIncompleteEvent => "upstream_incomplete_event",
            Self::ContentValidationFailed => "content_validation_failed",
            Self::OutputLimit => "output_limit",
        }
    }
}

/// A fully framed SSE event. The framing layer intentionally does not inspect
/// the data payload as JSON or infer a provider dialect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SseEvent {
    pub data: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SseStreamStats {
    pub total_stream_bytes: usize,
    pub event_count: usize,
    pub max_pending_event_bytes: usize,
}

/// An incremental Server-Sent Events framer.
///
/// A caller feeds each network chunk to [`Self::push`]. Complete events are
/// returned immediately and already-consumed bytes are drained before the
/// pending-event limit is checked. This is what makes a long sequence of small
/// legal events safe while still bounding a malformed, never-terminated event.
#[derive(Debug)]
pub(crate) struct SseDecoder {
    limits: SseLimits,
    pending: Vec<u8>,
    stats: SseStreamStats,
    failure: Option<StreamError>,
}

impl SseDecoder {
    pub(crate) fn new(limits: SseLimits) -> Self {
        Self {
            limits,
            pending: Vec::new(),
            stats: SseStreamStats::default(),
            failure: None,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, StreamError> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }

        self.stats.total_stream_bytes = self.stats.total_stream_bytes.saturating_add(chunk.len());
        if self.stats.total_stream_bytes > self.limits.max_total_stream_bytes {
            return Err(self.fail(StreamError::ResponseBodyLimit));
        }

        self.pending.extend_from_slice(chunk);
        let mut consumed = 0;
        let mut events = Vec::new();

        while let Some(boundary) = find_event_boundary(&self.pending[consumed..]) {
            let event_start = consumed;
            let event_end = event_start + boundary.event_len;
            if boundary.event_len > self.limits.max_pending_event_bytes {
                return Err(self.fail(StreamError::SseEventTooLarge));
            }

            let event = match parse_event(&self.pending[event_start..event_end]) {
                Ok(event) => event,
                Err(error) => return Err(self.fail(error)),
            };
            consumed = event_end + boundary.separator_len;

            if let Some(event) = event {
                self.stats.event_count = self.stats.event_count.saturating_add(1);
                if self.stats.event_count > self.limits.max_sse_events {
                    return Err(self.fail(StreamError::SseEventLimit));
                }
                events.push(event);
            }
        }

        if consumed > 0 {
            self.pending.drain(..consumed);
        }
        self.stats.max_pending_event_bytes =
            self.stats.max_pending_event_bytes.max(self.pending.len());
        if self.pending.len() > self.limits.max_pending_event_bytes {
            return Err(self.fail(StreamError::SseEventTooLarge));
        }
        Ok(events)
    }

    /// Completes framing at end-of-stream.
    ///
    /// A provider may end the connection immediately after a complete final
    /// field line instead of emitting one more blank SSE line. Accept that
    /// final line as an event, but never manufacture an event from a partial
    /// line: a residual buffer without a line ending remains malformed.
    /// Protocol reducers separately decide whether the resulting event stream
    /// contains the required success terminal.
    pub(crate) fn finish(&mut self) -> Result<Vec<SseEvent>, StreamError> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if self.pending.is_empty() {
            return Ok(Vec::new());
        }
        if !self.pending.ends_with(b"\n") {
            return Err(self.fail(StreamError::InvalidSseFraming));
        }

        let pending = std::mem::take(&mut self.pending);
        let event = match parse_event(&pending) {
            Ok(event) => event,
            Err(error) => return Err(self.fail(error)),
        };
        let Some(event) = event else {
            return Ok(Vec::new());
        };
        self.stats.event_count = self.stats.event_count.saturating_add(1);
        if self.stats.event_count > self.limits.max_sse_events {
            return Err(self.fail(StreamError::SseEventLimit));
        }
        Ok(vec![event])
    }

    pub(crate) const fn stats(&self) -> SseStreamStats {
        self.stats
    }

    fn fail(&mut self, failure: StreamError) -> StreamError {
        self.pending.clear();
        self.failure = Some(failure);
        failure
    }
}

#[derive(Debug, Clone, Copy)]
struct EventBoundary {
    event_len: usize,
    separator_len: usize,
}

fn find_event_boundary(bytes: &[u8]) -> Option<EventBoundary> {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\n' && bytes.get(index + 1) == Some(&b'\n') {
            return Some(EventBoundary {
                event_len: index,
                separator_len: 2,
            });
        }
        if bytes[index] == b'\r'
            && bytes.get(index + 1) == Some(&b'\n')
            && bytes.get(index + 2) == Some(&b'\r')
            && bytes.get(index + 3) == Some(&b'\n')
        {
            return Some(EventBoundary {
                event_len: index,
                separator_len: 4,
            });
        }
        index += 1;
    }
    None
}

fn parse_event(bytes: &[u8]) -> Result<Option<SseEvent>, StreamError> {
    let raw = str::from_utf8(bytes).map_err(|_| StreamError::InvalidSseUtf8)?;
    let mut data_lines = Vec::new();

    for raw_line in raw.split('\n') {
        let line = match raw_line.strip_suffix('\r') {
            Some(line) => line,
            None if raw_line.contains('\r') => return Err(StreamError::InvalidSseFraming),
            None => raw_line,
        };
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let (field, value) = line.split_once(':').unwrap_or((line, ""));
        if field == "data" {
            data_lines.push(value.strip_prefix(' ').unwrap_or(value));
        }
    }

    if data_lines.is_empty() {
        return Ok(None);
    }
    Ok(Some(SseEvent {
        data: data_lines.join("\n"),
    }))
}

#[cfg(test)]
mod tests {
    use super::{SseDecoder, SseLimits, StreamError};

    fn generous_limits() -> SseLimits {
        SseLimits {
            max_pending_event_bytes: 64 * 1024,
            max_total_stream_bytes: 512 * 1024,
            max_sse_events: 2_000,
        }
    }

    #[test]
    fn sse_decoder_is_invariant_to_every_byte_boundary_including_utf8() {
        let input = "data: {\"delta\":\"你好\"}\r\n\r\ndata: [DONE]\n\n".as_bytes();
        let expected = vec!["{\"delta\":\"你好\"}", "[DONE]"];

        let mut decoder = SseDecoder::new(generous_limits());
        let mut actual = Vec::new();
        for byte in input {
            actual.extend(
                decoder
                    .push(std::slice::from_ref(byte))
                    .expect("each byte chunk is valid"),
            );
        }
        decoder.finish().expect("complete stream");

        assert_eq!(
            actual
                .into_iter()
                .map(|event| event.data)
                .collect::<Vec<_>>(),
            expected
        );
    }

    #[test]
    fn sse_decoder_supports_comments_empty_events_and_multiline_data() {
        let mut decoder = SseDecoder::new(generous_limits());
        let events = decoder
            .push(b": keepalive\r\n\r\ndata: first\r\ndata: second\r\n\r\n\n\n")
            .expect("valid SSE framing");
        decoder.finish().expect("complete stream");

        assert_eq!(events.len(), 1);
        assert_eq!(events[0].data, "first\nsecond");
        assert_eq!(decoder.stats().event_count, 1);
    }

    #[test]
    fn sse_decoder_allows_large_total_stream_with_small_complete_events() {
        let mut decoder = SseDecoder::new(generous_limits());
        let event = b"data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"padding-padding-padding-padding-padding-padding-padding\"}\n\n";
        let mut event_count = 0;
        for _ in 0..700 {
            event_count += decoder.push(event).expect("small complete event").len();
        }
        decoder.finish().expect("complete stream");

        assert!(decoder.stats().total_stream_bytes > 64 * 1024);
        assert_eq!(event_count, 700);
        assert_eq!(decoder.stats().max_pending_event_bytes, 0);
    }

    #[test]
    fn default_sse_policy_allows_a_verbose_responses_stream_larger_than_256_kib() {
        let mut decoder = SseDecoder::new(SseLimits::default());
        let event = b"data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"bounded diagnostic-free reasoning padding for an otherwise valid stream\"}\n\n";
        let mut event_count = 0;
        for _ in 0..2_100 {
            event_count += decoder.push(event).expect("default policy event").len();
        }
        decoder.finish().expect("complete stream");

        assert!(decoder.stats().total_stream_bytes > 256 * 1024);
        assert_eq!(event_count, 2_100);
    }

    #[test]
    fn sse_decoder_enforces_exact_single_event_limit_not_total_size() {
        let limits = SseLimits {
            max_pending_event_bytes: 16,
            max_total_stream_bytes: 1_024,
            max_sse_events: 10,
        };
        let mut exact = SseDecoder::new(limits);
        assert_eq!(
            exact
                .push(b"data: 1234567890\n\n")
                .expect("exact event limit")[0]
                .data,
            "1234567890"
        );

        let mut oversized = SseDecoder::new(limits);
        assert_eq!(
            oversized.push(b"data: 12345678901\n\n"),
            Err(StreamError::SseEventTooLarge)
        );
        assert_eq!(oversized.stats().max_pending_event_bytes, 0);
    }

    #[test]
    fn sse_decoder_rejects_an_unterminated_pending_event_and_incomplete_eof() {
        let limits = SseLimits {
            max_pending_event_bytes: 8,
            max_total_stream_bytes: 1_024,
            max_sse_events: 10,
        };
        let mut oversized = SseDecoder::new(limits);
        assert_eq!(
            oversized.push(b"data: 123"),
            Err(StreamError::SseEventTooLarge)
        );

        let mut incomplete = SseDecoder::new(generous_limits());
        incomplete.push(b"data: hello").expect("within limit");
        assert_eq!(incomplete.finish(), Err(StreamError::InvalidSseFraming));
    }

    #[test]
    fn sse_decoder_accepts_a_complete_final_line_at_eof() {
        let mut decoder = SseDecoder::new(generous_limits());
        decoder
            .push(b"data: final event\r\n")
            .expect("final line is buffered until EOF");

        let events = decoder.finish().expect("complete final line at EOF");
        assert_eq!(
            events,
            vec![super::SseEvent {
                data: "final event".into()
            }]
        );
        assert_eq!(decoder.stats().event_count, 1);
    }

    #[test]
    fn sse_decoder_has_independent_total_and_event_count_limits() {
        let mut total_limited = SseDecoder::new(SseLimits {
            max_pending_event_bytes: 64,
            max_total_stream_bytes: 8,
            max_sse_events: 10,
        });
        assert_eq!(
            total_limited.push(b"data: 1\n\n"),
            Err(StreamError::ResponseBodyLimit)
        );

        let mut event_limited = SseDecoder::new(SseLimits {
            max_pending_event_bytes: 64,
            max_total_stream_bytes: 1_024,
            max_sse_events: 1,
        });
        assert_eq!(
            event_limited.push(b"data: 1\n\ndata: 2\n\n"),
            Err(StreamError::SseEventLimit)
        );
    }
}
