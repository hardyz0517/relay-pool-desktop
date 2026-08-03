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
pub struct AnthropicMessagesAdapter {
    stream: bool,
}

impl AnthropicMessagesAdapter {
    pub fn new(stream: bool) -> Self {
        Self { stream }
    }
}

impl ProtocolAdapter for AnthropicMessagesAdapter {
    fn request_descriptor(&self) -> RequestDescriptor {
        RequestDescriptor {
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
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
            return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
        }
        if let Some(failure_kind) = classify_http_status(http_status) {
            return unavailable(http_status, failure_kind, body.len());
        }
        if self.stream {
            parse_stream(http_status, content_type, body, validator, limits)
        } else {
            parse_json(http_status, content_type, body, validator, limits)
        }
    }
}

fn parse_json(
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if !is_json(content_type) || body.is_empty() {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }
    let value = match serde_json::from_slice::<Value>(body) {
        Ok(value) => value,
        Err(_) => return unavailable(http_status, FailureKind::ProtocolMismatch, body.len()),
    };
    if value.get("error").is_some() || value.get("type").and_then(Value::as_str) == Some("error") {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }
    if value.get("type").and_then(Value::as_str) != Some("message")
        || !is_successful_stop_reason(value.get("stop_reason").and_then(Value::as_str))
    {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }

    let output_text = anthropic_content_text(&value);
    let model = string_field(&value, "model");
    let usage = anthropic_usage(value.get("usage"));
    validate_output_text(
        ProtocolKind::AnthropicMessages,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn parse_stream(
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if !is_sse(content_type) || body.is_empty() {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }
    let events = match sse_data_events(body, limits) {
        Ok(events) => events,
        Err(failure_kind) => return unavailable(http_status, failure_kind, body.len()),
    };

    let mut output_text = String::new();
    let mut saw_message_stop = false;
    let mut saw_successful_stop_reason = false;
    let mut model = None;
    let mut input_tokens = None;
    let mut output_tokens = None;
    let mut cache_creation_tokens = None;
    let mut cache_read_tokens = None;

    for event in events {
        let value = match serde_json::from_str::<Value>(&event) {
            Ok(value) => value,
            Err(_) => return unavailable(http_status, FailureKind::ProtocolMismatch, body.len()),
        };
        if value.get("error").is_some()
            || value.get("type").and_then(Value::as_str) == Some("error")
        {
            return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
        }

        match value.get("type").and_then(Value::as_str) {
            Some("message_start") => {
                let message = value.get("message").unwrap_or(&value);
                model = model.or_else(|| string_field(message, "model"));
                if let Some(usage) = message.get("usage") {
                    input_tokens = input_tokens.or_else(|| integer_field(usage, "input_tokens"));
                    cache_creation_tokens = cache_creation_tokens
                        .or_else(|| integer_field(usage, "cache_creation_input_tokens"));
                    cache_read_tokens = cache_read_tokens
                        .or_else(|| integer_field(usage, "cache_read_input_tokens"));
                }
            }
            Some("content_block_delta") => {
                if let Some(text) = value
                    .get("delta")
                    .and_then(|delta| delta.get("text"))
                    .and_then(Value::as_str)
                {
                    output_text.push_str(text);
                }
            }
            Some("message_delta") => {
                if is_successful_stop_reason(
                    value
                        .get("delta")
                        .and_then(|delta| delta.get("stop_reason"))
                        .and_then(Value::as_str),
                ) {
                    saw_successful_stop_reason = true;
                }
                if let Some(usage) = value.get("usage") {
                    output_tokens = output_tokens.or_else(|| integer_field(usage, "output_tokens"));
                }
            }
            Some("message_stop") => saw_message_stop = true,
            Some("content_block_start") | Some("content_block_stop") | Some("ping") => {}
            _ => {}
        }
    }

    if !saw_message_stop || !saw_successful_stop_reason {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }
    let usage = Some(ProbeUsage {
        input_tokens,
        output_tokens,
        total_tokens: sum_tokens(input_tokens, output_tokens),
        cache_creation_tokens,
        cache_read_tokens,
    });
    validate_output_text(
        ProtocolKind::AnthropicMessages,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn anthropic_content_text(value: &Value) -> String {
    value
        .get("content")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("text"))
        .filter_map(|block| block.get("text").and_then(Value::as_str))
        .collect::<String>()
}

fn anthropic_usage(value: Option<&Value>) -> Option<ProbeUsage> {
    let value = value?;
    let input_tokens = integer_field(value, "input_tokens");
    let output_tokens = integer_field(value, "output_tokens");
    Some(ProbeUsage {
        input_tokens,
        output_tokens,
        total_tokens: sum_tokens(input_tokens, output_tokens),
        cache_creation_tokens: integer_field(value, "cache_creation_input_tokens"),
        cache_read_tokens: integer_field(value, "cache_read_input_tokens"),
    })
}

fn is_successful_stop_reason(value: Option<&str>) -> bool {
    matches!(value, Some("end_turn" | "stop_sequence"))
}

fn sum_tokens(left: Option<i64>, right: Option<i64>) -> Option<i64> {
    match (left, right) {
        (Some(left), Some(right)) => Some(left + right),
        _ => None,
    }
}

fn unavailable(
    http_status: u16,
    failure_kind: FailureKind,
    response_bytes: usize,
) -> ParsedProbeResponse {
    ParsedProbeResponse::unavailable(
        ProtocolKind::AnthropicMessages,
        Some(http_status),
        failure_kind,
        response_bytes,
    )
}
