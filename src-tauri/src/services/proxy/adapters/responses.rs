use serde_json::{json, Value};

use crate::models::proxy::UpstreamApiFormat;

use super::{
    capability::{
        AdapterCapabilityFeature, AdapterCapabilityProtocol, AdapterCapabilitySignal,
        AdapterCapabilitySubject, AdapterCapabilityVerdict,
    },
    openai::{extract_choice_text, wrap_chat_response_as_responses},
};

pub fn upstream_responses_path(format: &UpstreamApiFormat) -> &'static str {
    match format {
        UpstreamApiFormat::OpenAiChatCompletions => "/v1/chat/completions",
        UpstreamApiFormat::OpenAiResponses
        | UpstreamApiFormat::Auto
        | UpstreamApiFormat::CustomOpenAiCompatible => "/v1/responses",
    }
}

pub fn render_responses_response(body: Value, fallback_model: Option<&str>) -> Value {
    if let Some(json_value) = body.as_object() {
        if json_value.get("object").and_then(Value::as_str) == Some("response") {
            return body;
        }
    }

    let content = extract_choice_text(&body).unwrap_or_else(|| body.to_string());
    let wrapped = wrap_chat_response_as_responses(body, fallback_model);
    if wrapped.get("output_text").and_then(Value::as_str).is_none() {
        return json!({
            "id": wrapped.get("id").cloned().unwrap_or(Value::Null),
            "object": "response",
            "created": wrapped.get("created").cloned().unwrap_or(Value::Null),
            "model": wrapped.get("model").cloned().unwrap_or(Value::Null),
            "output": [{
                "id": "output-unknown",
                "type": "message",
                "role": "assistant",
                "content": [{
                    "type": "output_text",
                    "text": content,
                }],
            }],
            "output_text": content,
            "usage": wrapped.get("usage").cloned().unwrap_or(Value::Null),
        });
    }
    wrapped
}

#[allow(dead_code)]
pub(crate) fn responses_capability_signals(
    format: &UpstreamApiFormat,
) -> Vec<AdapterCapabilitySignal> {
    let responses_verdict = match format {
        UpstreamApiFormat::OpenAiChatCompletions => AdapterCapabilityVerdict::Unsupported,
        UpstreamApiFormat::OpenAiResponses
        | UpstreamApiFormat::Auto
        | UpstreamApiFormat::CustomOpenAiCompatible => AdapterCapabilityVerdict::Supported,
    };
    vec![
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::Responses),
            responses_verdict,
            "responses_adapter_protocol_selection",
        ),
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Feature(AdapterCapabilityFeature::Stream),
            AdapterCapabilityVerdict::Supported,
            "responses_streaming_supported_by_wire_protocol",
        ),
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Feature(AdapterCapabilityFeature::Tools),
            AdapterCapabilityVerdict::Supported,
            "responses_tools_are_openai_compatible",
        ),
        AdapterCapabilitySignal::structural(
            AdapterCapabilitySubject::Feature(AdapterCapabilityFeature::Reasoning),
            AdapterCapabilityVerdict::Supported,
            "responses_reasoning_is_protocol_capable",
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_responses_path_prefers_responses_for_compatible_formats() {
        assert_eq!(
            upstream_responses_path(&UpstreamApiFormat::OpenAiResponses),
            "/v1/responses"
        );
        assert_eq!(
            upstream_responses_path(&UpstreamApiFormat::Auto),
            "/v1/responses"
        );
        assert_eq!(
            upstream_responses_path(&UpstreamApiFormat::CustomOpenAiCompatible),
            "/v1/responses"
        );
        assert_eq!(
            upstream_responses_path(&UpstreamApiFormat::OpenAiChatCompletions),
            "/v1/chat/completions"
        );
    }

    #[test]
    fn responses_adapter_reports_explicit_structural_capability_signals() {
        let signals = responses_capability_signals(&UpstreamApiFormat::OpenAiResponses);
        assert!(signals.iter().any(|signal| {
            signal.subject
                == AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::Responses)
                && signal.verdict == AdapterCapabilityVerdict::Supported
        }));

        let chat_only = responses_capability_signals(&UpstreamApiFormat::OpenAiChatCompletions);
        assert!(chat_only.iter().any(|signal| {
            signal.subject
                == AdapterCapabilitySubject::Protocol(AdapterCapabilityProtocol::Responses)
                && signal.verdict == AdapterCapabilityVerdict::Unsupported
        }));
    }
}
