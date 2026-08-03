use crate::{
    models::monitoring::{FailureKind, ProbeOutcome, ProtocolKind},
    services::monitoring::challenge::ChallengeValidator,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ResponseLimits {
    pub max_response_bytes: usize,
    pub max_output_bytes: usize,
    pub max_sse_events: usize,
}

impl Default for ResponseLimits {
    fn default() -> Self {
        Self {
            max_response_bytes: 256 * 1024,
            max_output_bytes: 8 * 1024,
            max_sse_events: 512,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestDescriptor {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
    pub stream: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProbeUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedProbeResponse {
    pub protocol_kind: ProtocolKind,
    pub outcome: ProbeOutcome,
    pub failure_kind: Option<FailureKind>,
    pub terminal: bool,
    pub http_status: Option<u16>,
    pub model: Option<String>,
    pub usage: Option<ProbeUsage>,
    pub output_text: Option<String>,
    pub response_bytes: usize,
    pub output_bytes: usize,
}

impl ParsedProbeResponse {
    pub fn available(
        protocol_kind: ProtocolKind,
        http_status: Option<u16>,
        output_text: String,
        response_bytes: usize,
    ) -> Self {
        let output_bytes = output_text.len();
        Self {
            protocol_kind,
            outcome: ProbeOutcome::Available,
            failure_kind: None,
            terminal: true,
            http_status,
            model: None,
            usage: None,
            output_text: Some(output_text),
            response_bytes,
            output_bytes,
        }
    }

    pub fn unavailable(
        protocol_kind: ProtocolKind,
        http_status: Option<u16>,
        failure_kind: FailureKind,
        response_bytes: usize,
    ) -> Self {
        Self {
            protocol_kind,
            outcome: ProbeOutcome::Unavailable,
            failure_kind: Some(failure_kind),
            terminal: true,
            http_status,
            model: None,
            usage: None,
            output_text: None,
            response_bytes,
            output_bytes: 0,
        }
    }

    pub fn with_model(mut self, model: Option<String>) -> Self {
        self.model = model;
        self
    }

    pub fn with_usage(mut self, usage: Option<ProbeUsage>) -> Self {
        self.usage = usage;
        self
    }
}

pub trait ProtocolAdapter: Send + Sync {
    fn request_descriptor(&self) -> RequestDescriptor;
    fn parse_response(
        &self,
        http_status: u16,
        content_type: Option<&str>,
        body: &[u8],
        validator: &ChallengeValidator,
        limits: ResponseLimits,
    ) -> ParsedProbeResponse;
}

pub(crate) fn validate_output_text(
    protocol_kind: ProtocolKind,
    http_status: Option<u16>,
    output_text: String,
    response_bytes: usize,
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if output_text.is_empty() {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            http_status,
            FailureKind::ContentMismatch,
            response_bytes,
        );
    }
    if output_text.len() > limits.max_output_bytes {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            http_status,
            FailureKind::ProtocolMismatch,
            response_bytes,
        );
    }
    if !validator.validate(&output_text) {
        return ParsedProbeResponse::unavailable(
            protocol_kind,
            http_status,
            FailureKind::ContentMismatch,
            response_bytes,
        );
    }
    ParsedProbeResponse::available(protocol_kind, http_status, output_text, response_bytes)
}

#[cfg(test)]
pub(crate) fn extract_text_fields(value: &Value) -> String {
    let mut text = String::new();
    collect_text(value, &mut text);
    text
}

#[cfg(test)]
fn collect_text(value: &Value, output: &mut String) {
    match value {
        Value::Object(map) => {
            for (key, value) in map {
                if matches!(key.as_str(), "content" | "text" | "output_text" | "delta") {
                    if let Some(text) = value.as_str() {
                        output.push_str(text);
                    }
                }
                collect_text(value, output);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_text(item, output);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
use serde_json::Value;
