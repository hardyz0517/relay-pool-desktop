use serde_json::{json, Value};

use crate::application::request_finalization::failure::{
    CapabilityApplicabilitySet, ProviderErrorSemanticSignal,
};
use crate::models::proxy::UpstreamApiFormat;

use super::openai::{
    extract_choice_text, openai_error_semantic_signal, wrap_chat_response_as_responses,
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

pub(crate) fn responses_error_semantic_signal(
    status: u16,
    body: Option<&Value>,
    station_key_id: &str,
    station_id: &str,
    endpoint_revision: i64,
    model: Option<&str>,
    applicability: CapabilityApplicabilitySet,
) -> ProviderErrorSemanticSignal {
    openai_error_semantic_signal(
        status,
        body,
        station_key_id,
        station_id,
        endpoint_revision,
        model,
        applicability,
    )
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
}
