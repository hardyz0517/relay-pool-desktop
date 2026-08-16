use crate::{
    models::monitoring::{FailureKind, ProtocolKind},
    services::{
        monitoring::{
            adapters::{
                contract::{
                    validate_output_text, ParsedProbeResponse, ProbeUsage, ProtocolAdapter,
                    RequestDescriptor, ResponseLimits,
                },
                http_mapping::classify_http_status,
                openai_chat::is_json,
                openai_stream::IncrementalOpenAiStream,
            },
            challenge::ChallengeValidator,
        },
        protocol_streaming::{parse_openai_responses_json, OpenAiUsage},
    },
};

#[derive(Debug, Clone)]
pub struct OpenAiResponsesAdapter {
    stream: bool,
}

impl OpenAiResponsesAdapter {
    pub fn new(stream: bool) -> Self {
        Self { stream }
    }
}

impl ProtocolAdapter for OpenAiResponsesAdapter {
    fn request_descriptor(&self) -> RequestDescriptor {
        RequestDescriptor {
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            body: Vec::new(),
            stream: self.stream,
        }
    }

    fn parse_response(
        &self,
        http_status: u16,
        content_type: Option<&str>,
        body: &[u8],
        validator: &ChallengeValidator,
        limits: ResponseLimits,
    ) -> ParsedProbeResponse {
        if body.len() > limits.max_response_bytes {
            return unavailable(
                ProtocolKind::OpenAiResponses,
                http_status,
                FailureKind::ProtocolMismatch,
                body.len(),
            );
        }
        if let Some(failure_kind) = classify_http_status(http_status) {
            return unavailable(
                ProtocolKind::OpenAiResponses,
                http_status,
                failure_kind,
                body.len(),
            );
        }
        if self.stream {
            parse_responses_stream(http_status, content_type, body, validator, limits)
        } else {
            parse_responses_json(http_status, content_type, body, validator, limits)
        }
    }
}

fn parse_responses_json(
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if !is_json(content_type) || body.is_empty() {
        return unavailable(
            ProtocolKind::OpenAiResponses,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let summary = match parse_openai_responses_json(body, limits.max_output_bytes) {
        Ok(summary) => summary,
        Err(_) => {
            return unavailable(
                ProtocolKind::OpenAiResponses,
                http_status,
                FailureKind::ProtocolMismatch,
                body.len(),
            )
        }
    };
    validate_output_text(
        ProtocolKind::OpenAiResponses,
        Some(http_status),
        summary.output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(summary.model)
    .with_usage(summary.usage.map(probe_usage))
}

fn parse_responses_stream(
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    let mut consumer = IncrementalOpenAiStream::for_protocol(ProtocolKind::OpenAiResponses, limits)
        .expect("Responses has an incremental stream consumer");
    consumer.consume(body);
    // The historical whole-body adapter accepted a final complete `data:`
    // block at EOF. Preserve that compatibility for adapter-contract callers
    // while production monitoring feeds actual network chunks to the same
    // incremental consumer and still requires an explicit terminal event.
    if !body.ends_with(b"\n\n") && !body.ends_with(b"\r\n\r\n") {
        let eof_separator = if body.ends_with(b"\r\n") {
            b"\r\n".as_slice()
        } else if body.ends_with(b"\n") {
            b"\n".as_slice()
        } else {
            b"\n\n".as_slice()
        };
        consumer.consume(eof_separator);
    }
    consumer.finish(http_status, content_type, validator).0
}

fn probe_usage(value: OpenAiUsage) -> ProbeUsage {
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
