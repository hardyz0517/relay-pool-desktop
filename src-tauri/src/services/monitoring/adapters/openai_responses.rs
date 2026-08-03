use serde_json::Value;

use crate::{
    models::monitoring::{FailureKind, ProtocolKind},
    services::monitoring::{
        adapters::{
            contract::{
                validate_output_text, ParsedProbeResponse, ProbeUsage, ProtocolAdapter,
                RequestDescriptor, ResponseLimits,
            },
            http_mapping::classify_http_status,
            openai_chat::{integer_field, is_json, is_sse, sse_data_events, string_field},
        },
        challenge::ChallengeValidator,
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
    let value = match serde_json::from_slice::<Value>(body) {
        Ok(value) => value,
        Err(_) => {
            return unavailable(
                ProtocolKind::OpenAiResponses,
                http_status,
                FailureKind::ProtocolMismatch,
                body.len(),
            )
        }
    };
    if value.get("error").is_some() {
        return unavailable(
            ProtocolKind::OpenAiResponses,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    if value.get("status").and_then(Value::as_str) != Some("completed") {
        return unavailable(
            ProtocolKind::OpenAiResponses,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let output_text = response_output_text(&value);
    let model = string_field(&value, "model");
    let usage = responses_usage(value.get("usage"));
    validate_output_text(
        ProtocolKind::OpenAiResponses,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn parse_responses_stream(
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if !is_sse(content_type) || body.is_empty() {
        return unavailable(
            ProtocolKind::OpenAiResponses,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let events = match sse_data_events(body, limits) {
        Ok(events) => events,
        Err(failure_kind) => {
            return unavailable(
                ProtocolKind::OpenAiResponses,
                http_status,
                failure_kind,
                body.len(),
            )
        }
    };
    let mut output_text = String::new();
    let mut completed = false;
    let mut model = None;
    let mut usage = None;
    for event in events {
        let value = match serde_json::from_str::<Value>(&event) {
            Ok(value) => value,
            Err(_) => {
                return unavailable(
                    ProtocolKind::OpenAiResponses,
                    http_status,
                    FailureKind::ProtocolMismatch,
                    body.len(),
                )
            }
        };
        let event_type = value.get("type").and_then(Value::as_str);
        match event_type {
            Some("response.output_text.delta") => {
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    output_text.push_str(delta);
                }
            }
            Some("response.completed") => {
                completed = true;
                let response = value.get("response").unwrap_or(&value);
                model = model.or_else(|| string_field(response, "model"));
                usage = usage.or_else(|| responses_usage(response.get("usage")));
            }
            Some("response.failed") | Some("response.incomplete") => {
                return unavailable(
                    ProtocolKind::OpenAiResponses,
                    http_status,
                    FailureKind::ProtocolMismatch,
                    body.len(),
                );
            }
            Some("response.created") => {
                let response = value.get("response").unwrap_or(&value);
                model = model.or_else(|| string_field(response, "model"));
            }
            _ => {}
        }
    }
    if !completed {
        return unavailable(
            ProtocolKind::OpenAiResponses,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    validate_output_text(
        ProtocolKind::OpenAiResponses,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn response_output_text(value: &Value) -> String {
    let mut output = String::new();
    if let Some(items) = value.get("output").and_then(Value::as_array) {
        for item in items {
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for block in content {
                    if block.get("type").and_then(Value::as_str) == Some("output_text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            output.push_str(text);
                        }
                    }
                }
            }
        }
    }
    output
}

fn responses_usage(value: Option<&Value>) -> Option<ProbeUsage> {
    let value = value?;
    Some(ProbeUsage {
        input_tokens: integer_field(value, "input_tokens"),
        output_tokens: integer_field(value, "output_tokens"),
        total_tokens: integer_field(value, "total_tokens"),
        cache_creation_tokens: integer_field(value, "cache_creation_input_tokens"),
        cache_read_tokens: integer_field(value, "cached_input_tokens"),
    })
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
