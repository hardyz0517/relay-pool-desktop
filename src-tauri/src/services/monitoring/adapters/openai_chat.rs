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
            openai_stream::IncrementalOpenAiStream,
            provider_error::classify_provider_error,
        },
        challenge::ChallengeValidator,
    },
};

#[derive(Debug, Clone)]
pub struct OpenAiChatAdapter {
    stream: bool,
}

impl OpenAiChatAdapter {
    pub fn new(stream: bool) -> Self {
        Self { stream }
    }
}

impl ProtocolAdapter for OpenAiChatAdapter {
    fn request_descriptor(&self) -> RequestDescriptor {
        RequestDescriptor {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
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
        parse_chat_response(
            ProtocolKind::OpenAiChat,
            self.stream,
            http_status,
            content_type,
            body,
            validator,
            limits,
        )
    }
}

pub(crate) fn parse_chat_response(
    protocol_kind: ProtocolKind,
    stream: bool,
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if body.len() > limits.max_response_bytes {
        return unavailable(
            protocol_kind,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    if let Some(failure_kind) = classify_provider_error(
        protocol_kind,
        http_status,
        content_type,
        body,
        crate::services::proxy::adapters::error_envelope::FailureTransport::Http,
    )
    .or_else(|| classify_http_status(http_status))
    {
        return unavailable(protocol_kind, http_status, failure_kind, body.len());
    }
    if stream {
        parse_chat_stream(
            protocol_kind,
            http_status,
            content_type,
            body,
            validator,
            limits,
        )
    } else {
        parse_chat_json(
            protocol_kind,
            http_status,
            content_type,
            body,
            validator,
            limits,
        )
    }
}

fn parse_chat_json(
    protocol_kind: ProtocolKind,
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    if !is_json(content_type) || body.is_empty() {
        return unavailable(
            protocol_kind,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let value = match serde_json::from_slice::<Value>(body) {
        Ok(value) => value,
        Err(_) => {
            return unavailable(
                protocol_kind,
                http_status,
                FailureKind::ProtocolMismatch,
                body.len(),
            )
        }
    };
    if let Some(failure_kind) = classify_provider_error(
        protocol_kind,
        http_status,
        content_type,
        body,
        crate::services::proxy::adapters::error_envelope::FailureTransport::Http,
    ) {
        return unavailable(protocol_kind, http_status, failure_kind, body.len());
    }
    if value.get("error").is_some() || value.get("choices").is_none() {
        return unavailable(
            protocol_kind,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let output_text = chat_message_content(&value);
    let model = string_field(&value, "model");
    let usage = openai_chat_usage(value.get("usage"));
    validate_output_text(
        protocol_kind,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn parse_chat_stream(
    protocol_kind: ProtocolKind,
    http_status: u16,
    content_type: Option<&str>,
    body: &[u8],
    validator: &ChallengeValidator,
    limits: ResponseLimits,
) -> ParsedProbeResponse {
    // The OpenAI Chat adapter's buffered contract path must share exactly the
    // same framing and terminal semantics as the live incremental executor.
    // Other chat-shaped adapters still use the legacy helper until their own
    // protocol reducers are migrated.
    if protocol_kind == ProtocolKind::OpenAiChat {
        let mut consumer = IncrementalOpenAiStream::for_protocol(protocol_kind, limits)
            .expect("OpenAI Chat has an incremental stream consumer");
        consumer.consume(body);
        return consumer.finish(http_status, content_type, validator).0;
    }
    if !is_sse(content_type) || body.is_empty() {
        return unavailable(
            protocol_kind,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    let events = match sse_data_events(body, limits) {
        Ok(events) => events,
        Err(failure_kind) => {
            return unavailable(protocol_kind, http_status, failure_kind, body.len())
        }
    };
    let mut output_text = String::new();
    let mut completed = false;
    let mut saw_finish_reason = false;
    let mut model = None;
    let mut usage = None;
    for event in events {
        if event.trim() == "[DONE]" {
            completed = true;
            continue;
        }
        let value = match serde_json::from_str::<Value>(&event) {
            Ok(value) => value,
            Err(_) => {
                return unavailable(
                    protocol_kind,
                    http_status,
                    FailureKind::ProtocolMismatch,
                    body.len(),
                )
            }
        };
        if let Some(failure_kind) = classify_provider_error(
            protocol_kind,
            http_status,
            Some("application/json"),
            event.as_bytes(),
            crate::services::proxy::adapters::error_envelope::FailureTransport::ChatSseError,
        ) {
            return unavailable(protocol_kind, http_status, failure_kind, body.len());
        }
        if value.get("error").is_some() {
            return unavailable(
                protocol_kind,
                http_status,
                FailureKind::ProtocolMismatch,
                body.len(),
            );
        }
        model = model.or_else(|| string_field(&value, "model"));
        usage = usage.or_else(|| openai_chat_usage(value.get("usage")));
        if let Some(choices) = value.get("choices").and_then(Value::as_array) {
            for choice in choices {
                if let Some(content) = choice
                    .get("delta")
                    .and_then(|delta| delta.get("content"))
                    .and_then(Value::as_str)
                {
                    output_text.push_str(content);
                }
                if choice
                    .get("finish_reason")
                    .is_some_and(|reason| !reason.is_null())
                {
                    saw_finish_reason = true;
                }
            }
        } else {
            return unavailable(
                protocol_kind,
                http_status,
                FailureKind::ProtocolMismatch,
                body.len(),
            );
        }
    }
    if !completed || !saw_finish_reason {
        return unavailable(
            protocol_kind,
            http_status,
            FailureKind::ProtocolMismatch,
            body.len(),
        );
    }
    validate_output_text(
        protocol_kind,
        Some(http_status),
        output_text,
        body.len(),
        validator,
        limits,
    )
    .with_model(model)
    .with_usage(usage)
}

fn chat_message_content(value: &Value) -> String {
    value
        .get("choices")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|choice| choice.get("message"))
        .filter_map(|message| message.get("content"))
        .filter_map(Value::as_str)
        .collect::<String>()
}

pub(crate) fn sse_data_events(
    body: &[u8],
    limits: ResponseLimits,
) -> Result<Vec<String>, FailureKind> {
    if body.len() > limits.max_response_bytes {
        return Err(FailureKind::ProtocolMismatch);
    }
    let raw = std::str::from_utf8(body).map_err(|_| FailureKind::ProtocolMismatch)?;
    let normalized = raw.replace("\r\n", "\n");
    let mut events = Vec::new();
    for block in normalized.split("\n\n") {
        let data = block
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() {
            continue;
        }
        events.push(data);
        if events.len() > limits.max_sse_events {
            return Err(FailureKind::ProtocolMismatch);
        }
    }
    Ok(events)
}

pub(crate) fn openai_chat_usage(value: Option<&Value>) -> Option<ProbeUsage> {
    let value = value?;
    Some(ProbeUsage {
        input_tokens: integer_field(value, "prompt_tokens"),
        output_tokens: integer_field(value, "completion_tokens"),
        total_tokens: integer_field(value, "total_tokens"),
        cache_creation_tokens: integer_field(value, "cache_creation_tokens"),
        cache_read_tokens: integer_field(value, "cache_read_tokens"),
    })
}

pub(crate) fn integer_field(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

pub(crate) fn string_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

pub(crate) fn is_json(content_type: Option<&str>) -> bool {
    content_type
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("json")
}

pub(crate) fn is_sse(content_type: Option<&str>) -> bool {
    content_type
        .unwrap_or_default()
        .to_ascii_lowercase()
        .contains("text/event-stream")
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

#[cfg(test)]
mod tests {
    use crate::{
        models::monitoring::{ProbeOutcome, ProtocolKind},
        services::monitoring::{
            adapters::{contract::ResponseLimits, openai_chat::parse_chat_response},
            challenge::ChallengeValidator,
        },
    };

    #[test]
    fn services_monitoring_openai_chat_rejects_status_only_success() {
        let parsed = parse_chat_response(
            ProtocolKind::OpenAiChat,
            false,
            200,
            Some("application/json"),
            br#"{"choices":[{"message":{"content":""},"finish_reason":"stop"}]}"#,
            &ChallengeValidator::from_expected_answer_for_tests("RP_ANSWER=42"),
            ResponseLimits::default(),
        );

        assert_eq!(parsed.outcome, ProbeOutcome::Unavailable);
    }
}
