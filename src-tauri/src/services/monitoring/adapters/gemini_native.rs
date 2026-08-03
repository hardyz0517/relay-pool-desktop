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
pub struct GeminiNativeAdapter {
    stream: bool,
}

impl GeminiNativeAdapter {
    pub fn new(stream: bool) -> Self {
        Self { stream }
    }
}

impl ProtocolAdapter for GeminiNativeAdapter {
    fn request_descriptor(&self) -> RequestDescriptor {
        RequestDescriptor {
            method: "POST".to_string(),
            path: if self.stream {
                "/v1beta/models/{model}:streamGenerateContent".to_string()
            } else {
                "/v1beta/models/{model}:generateContent".to_string()
            },
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
    if has_gemini_error_or_block(&value) || !has_stop_candidate(&value) {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }
    let output_text = gemini_parts_text(&value);
    let model = string_field(&value, "modelVersion");
    let usage = gemini_usage(value.get("usageMetadata"));
    validate_output_text(
        ProtocolKind::GeminiNative,
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
    let mut saw_stop = false;
    let mut model = None;
    let mut usage = None;

    for event in events {
        let value = match serde_json::from_str::<Value>(&event) {
            Ok(value) => value,
            Err(_) => return unavailable(http_status, FailureKind::ProtocolMismatch, body.len()),
        };
        if has_gemini_error_or_block(&value) {
            return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
        }
        output_text.push_str(&gemini_parts_text(&value));
        if has_stop_candidate(&value) {
            saw_stop = true;
        }
        model = model.or_else(|| string_field(&value, "modelVersion"));
        usage = usage.or_else(|| gemini_usage(value.get("usageMetadata")));
    }

    if !saw_stop {
        return unavailable(http_status, FailureKind::ProtocolMismatch, body.len());
    }
    validate_output_text(
        ProtocolKind::GeminiNative,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn has_gemini_error_or_block(value: &Value) -> bool {
    value.get("error").is_some()
        || value
            .get("promptFeedback")
            .and_then(|feedback| feedback.get("blockReason"))
            .is_some()
        || value
            .get("candidates")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .any(|candidate| {
                matches!(
                    candidate.get("finishReason").and_then(Value::as_str),
                    Some("SAFETY" | "RECITATION" | "BLOCKLIST" | "PROHIBITED_CONTENT" | "SPII")
                )
            })
}

fn has_stop_candidate(value: &Value) -> bool {
    value
        .get("candidates")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .any(|candidate| candidate.get("finishReason").and_then(Value::as_str) == Some("STOP"))
}

fn gemini_parts_text(value: &Value) -> String {
    let mut output = String::new();
    if let Some(candidates) = value.get("candidates").and_then(Value::as_array) {
        for candidate in candidates {
            if let Some(parts) = candidate
                .get("content")
                .and_then(|content| content.get("parts"))
                .and_then(Value::as_array)
            {
                for part in parts {
                    if let Some(text) = part.get("text").and_then(Value::as_str) {
                        output.push_str(text);
                    }
                }
            }
        }
    }
    output
}

fn gemini_usage(value: Option<&Value>) -> Option<ProbeUsage> {
    let value = value?;
    Some(ProbeUsage {
        input_tokens: integer_field(value, "promptTokenCount"),
        output_tokens: integer_field(value, "candidatesTokenCount"),
        total_tokens: integer_field(value, "totalTokenCount"),
        cache_creation_tokens: None,
        cache_read_tokens: integer_field(value, "cachedContentTokenCount"),
    })
}

fn unavailable(
    http_status: u16,
    failure_kind: FailureKind,
    response_bytes: usize,
) -> ParsedProbeResponse {
    ParsedProbeResponse::unavailable(
        ProtocolKind::GeminiNative,
        Some(http_status),
        failure_kind,
        response_bytes,
    )
}
