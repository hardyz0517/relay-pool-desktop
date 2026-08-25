use crate::{
    models::monitoring::{FailureKind, ProtocolKind},
    services::{
        monitoring::{
            adapters::{
                contract::{validate_output_text, ParsedProbeResponse, ProbeUsage, ResponseLimits},
                http_mapping::classify_http_status,
                provider_error::classify_provider_error,
            },
            challenge::ChallengeValidator,
        },
        protocol_streaming::{
            OpenAiChatReducer, OpenAiResponsesReducer, OpenAiStreamSummary, SseDecoder, SseLimits,
            StreamError,
        },
    },
};

/// Bounded, incremental stream consumer for the two OpenAI SSE protocols.
///
/// The consumer deliberately retains only decoder state and the small,
/// challenge-relevant output extracted by the reducer; it never reconstructs
/// an upstream response body.
pub(crate) struct IncrementalOpenAiStream {
    protocol_kind: ProtocolKind,
    limits: ResponseLimits,
    decoder: SseDecoder,
    reducer: OpenAiReducer,
    failure: Option<StreamError>,
    failure_kind_override: Option<FailureKind>,
}

enum OpenAiReducer {
    Chat(OpenAiChatReducer),
    Responses(OpenAiResponsesReducer),
}

impl IncrementalOpenAiStream {
    pub(crate) fn for_protocol(
        protocol_kind: ProtocolKind,
        limits: ResponseLimits,
    ) -> Option<Self> {
        let reducer = match protocol_kind {
            ProtocolKind::OpenAiChat => {
                OpenAiReducer::Chat(OpenAiChatReducer::new(limits.max_output_bytes))
            }
            ProtocolKind::OpenAiResponses => {
                OpenAiReducer::Responses(OpenAiResponsesReducer::new(limits.max_output_bytes))
            }
            _ => return None,
        };
        Some(Self {
            protocol_kind,
            limits,
            decoder: SseDecoder::new(SseLimits {
                max_pending_event_bytes: SseLimits::default().max_pending_event_bytes,
                // This is independent from the legacy buffered-response
                // limit: Responses streams commonly contain many benign
                // reasoning events. The transport's global outbound limit is
                // larger; this remains the monitoring-specific admission cap.
                max_total_stream_bytes: 2 * 1024 * 1024,
                // Responses can legitimately include many typed reasoning
                // events. Keep this stream-specific admission bound separate
                // from the legacy buffered adapter's event limit.
                max_sse_events: 4_096,
            }),
            reducer,
            failure: None,
            failure_kind_override: None,
        })
    }

    pub(crate) fn consume(&mut self, chunk: &[u8]) {
        // Do not ask the transport to turn a parser error into a network error:
        // the completed HTTP metadata must still win for non-2xx responses.
        if self.failure.is_some() {
            return;
        }
        let events = match self.decoder.push(chunk) {
            Ok(events) => events,
            Err(error) => {
                self.failure = Some(error);
                return;
            }
        };
        self.consume_events(events);
    }

    fn consume_events(&mut self, events: Vec<crate::services::protocol_streaming::SseEvent>) {
        for event in events {
            let result = match &mut self.reducer {
                OpenAiReducer::Chat(reducer) => reducer.push(&event),
                OpenAiReducer::Responses(reducer) => reducer.push(&event),
            };
            if let Err(error) = result {
                if matches!(error, StreamError::UpstreamFailedEvent) {
                    let transport = match self.protocol_kind {
                        ProtocolKind::OpenAiChat => {
                            crate::services::proxy::adapters::error_envelope::FailureTransport::ChatSseError
                        }
                        ProtocolKind::OpenAiResponses => {
                            crate::services::proxy::adapters::error_envelope::FailureTransport::ResponsesSseFailure
                        }
                        _ => crate::services::proxy::adapters::error_envelope::FailureTransport::Http,
                    };
                    self.failure_kind_override = classify_provider_error(
                        self.protocol_kind,
                        200,
                        Some("application/json"),
                        event.data.as_bytes(),
                        transport,
                    );
                }
                self.failure = Some(error);
                return;
            }
        }
    }

    pub(crate) fn finish(
        mut self,
        http_status: u16,
        content_type: Option<&str>,
        validator: &ChallengeValidator,
    ) -> (ParsedProbeResponse, Option<String>) {
        let response_bytes = self.decoder.stats().total_stream_bytes;
        let protocol_kind = self.protocol_kind;

        // HTTP failures have priority over incidental bytes that happen to
        // resemble malformed SSE in the error envelope.
        if let Some(failure_kind) = classify_http_status(http_status) {
            return (
                unavailable(
                    self.protocol_kind,
                    http_status,
                    failure_kind,
                    response_bytes,
                ),
                Some(failure_kind.as_str().to_string()),
            );
        }
        if !is_sse(content_type) {
            return stream_error_response(
                protocol_kind,
                http_status,
                response_bytes,
                StreamError::UnexpectedContentType,
            );
        }
        if let Some(error) = self.failure {
            if let Some(failure_kind) = self.failure_kind_override {
                return (
                    unavailable(protocol_kind, http_status, failure_kind, response_bytes),
                    Some(failure_kind.as_str().to_string()),
                );
            }
            return stream_error_response(protocol_kind, http_status, response_bytes, error);
        }
        match self.decoder.finish() {
            Ok(events) => self.consume_events(events),
            Err(error) => {
                return stream_error_response(protocol_kind, http_status, response_bytes, error)
            }
        }
        if let Some(error) = self.failure {
            return stream_error_response(protocol_kind, http_status, response_bytes, error);
        }
        let summary = match self.reducer {
            OpenAiReducer::Chat(reducer) => reducer.finish(),
            OpenAiReducer::Responses(reducer) => reducer.finish(),
        };
        match summary {
            Ok(summary) => validated_summary(
                self.protocol_kind,
                self.limits,
                http_status,
                response_bytes,
                summary,
                validator,
            ),
            Err(error) => stream_error_response(protocol_kind, http_status, response_bytes, error),
        }
    }
}

fn stream_error_response(
    protocol_kind: ProtocolKind,
    http_status: u16,
    response_bytes: usize,
    error: StreamError,
) -> (ParsedProbeResponse, Option<String>) {
    let failure_kind = match error {
        StreamError::ContentValidationFailed => FailureKind::ContentMismatch,
        _ => FailureKind::ProtocolMismatch,
    };
    (
        unavailable(protocol_kind, http_status, failure_kind, response_bytes),
        Some(error.as_code().to_string()),
    )
}

fn validated_summary(
    protocol_kind: ProtocolKind,
    limits: ResponseLimits,
    http_status: u16,
    response_bytes: usize,
    summary: OpenAiStreamSummary,
    validator: &ChallengeValidator,
) -> (ParsedProbeResponse, Option<String>) {
    let parsed = validate_output_text(
        protocol_kind,
        Some(http_status),
        summary.output_text,
        response_bytes,
        validator,
        limits,
    )
    .with_model(summary.model)
    .with_usage(summary.usage.map(usage));
    let reason = parsed.failure_kind.map(|kind| match kind {
        FailureKind::ContentMismatch => StreamError::ContentValidationFailed.as_code().to_string(),
        _ => kind.as_str().to_string(),
    });
    (parsed, reason)
}

fn usage(value: crate::services::protocol_streaming::OpenAiUsage) -> ProbeUsage {
    ProbeUsage {
        input_tokens: value.input_tokens,
        output_tokens: value.output_tokens,
        total_tokens: value.total_tokens,
        cache_creation_tokens: value.cache_creation_tokens,
        cache_read_tokens: value.cache_read_tokens,
    }
}

fn unavailable(
    protocol_kind: ProtocolKind,
    http_status: u16,
    failure_kind: FailureKind,
    response_bytes: usize,
) -> ParsedProbeResponse {
    ParsedProbeResponse::unavailable(
        protocol_kind,
        Some(http_status),
        failure_kind,
        response_bytes,
    )
}

fn is_sse(content_type: Option<&str>) -> bool {
    content_type
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("text/event-stream")
}

#[cfg(test)]
mod tests {
    use super::IncrementalOpenAiStream;
    use crate::{
        models::monitoring::{ProbeOutcome, ProtocolKind},
        services::{
            monitoring::{adapters::contract::ResponseLimits, challenge::ChallengeValidator},
            protocol_streaming::StreamError,
        },
    };

    #[test]
    fn responses_stream_over_legacy_buffer_limit_is_reduced_incrementally() {
        let mut stream = IncrementalOpenAiStream::for_protocol(
            ProtocolKind::OpenAiResponses,
            ResponseLimits::default(),
        )
        .expect("Responses has an incremental consumer");
        let reasoning = b"data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"padding-padding-padding-padding-padding-padding-padding-padding\"}\n\n";
        for _ in 0..2_200 {
            stream.consume(reasoning);
        }
        stream.consume(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"RP_ANSWER=42\"}\n\ndata: {\"type\":\"response.completed\"}\n\n");

        let (parsed, reason) = stream.finish(
            200,
            Some("text/event-stream; charset=utf-8"),
            &ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42"),
        );

        assert_eq!(parsed.outcome, ProbeOutcome::Available);
        assert!(parsed.response_bytes > ResponseLimits::default().max_response_bytes);
        assert_eq!(reason, None);
    }

    #[test]
    fn responses_stream_exposes_the_specific_safe_parser_reason() {
        let mut stream = IncrementalOpenAiStream::for_protocol(
            ProtocolKind::OpenAiResponses,
            ResponseLimits::default(),
        )
        .expect("Responses has an incremental consumer");
        stream.consume(b"data: {not-json}\n\n");

        let (parsed, reason) = stream.finish(
            200,
            Some("text/event-stream"),
            &ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42"),
        );

        assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
        assert_eq!(
            reason.as_deref(),
            Some(StreamError::InvalidEventJson.as_code())
        );
    }
}
