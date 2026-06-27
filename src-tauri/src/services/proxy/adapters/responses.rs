use serde_json::{json, Value};

use crate::models::proxy::UpstreamApiFormat;

use super::openai::{extract_choice_text, wrap_chat_response_as_responses};

pub fn extract_responses_metadata(body: &Value) -> (Option<String>, bool) {
    let model = body
        .get("model")
        .and_then(Value::as_str)
        .map(ToString::to_string);
    let stream = body
        .get("stream")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    (model, stream)
}

pub fn normalize_responses_request(body: &Value) -> Value {
    let model = body.get("model").cloned().unwrap_or(Value::Null);
    let stream = body.get("stream").cloned().unwrap_or(Value::Bool(false));
    let messages = responses_input_to_messages(body);

    json!({
        "model": model,
        "stream": stream,
        "messages": messages,
    })
}

pub fn upstream_responses_path(format: &UpstreamApiFormat) -> &'static str {
    match format {
        UpstreamApiFormat::OpenAiChatCompletions => "/v1/chat/completions",
        UpstreamApiFormat::OpenAiResponses
        | UpstreamApiFormat::Auto
        | UpstreamApiFormat::CustomOpenAiCompatible => "/v1/responses",
    }
}

pub fn should_try_chat_fallback(format: &UpstreamApiFormat) -> bool {
    matches!(
        format,
        UpstreamApiFormat::Auto | UpstreamApiFormat::CustomOpenAiCompatible
    )
}

pub fn render_responses_response(body: Value, fallback_model: Option<&str>) -> Value {
    if let Some(json_value) = body.as_object() {
        if json_value.get("object").and_then(Value::as_str) == Some("response") {
            return body;
        }
    }

    let content = extract_choice_text(&body).unwrap_or_else(|| {
        body.to_string()
    });
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_responses_path_prefers_responses_for_compatible_formats() {
        assert_eq!(upstream_responses_path(&UpstreamApiFormat::OpenAiResponses), "/v1/responses");
        assert_eq!(upstream_responses_path(&UpstreamApiFormat::Auto), "/v1/responses");
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
    fn should_try_chat_fallback_only_for_ambiguous_formats() {
        assert!(should_try_chat_fallback(&UpstreamApiFormat::Auto));
        assert!(should_try_chat_fallback(&UpstreamApiFormat::CustomOpenAiCompatible));
        assert!(!should_try_chat_fallback(&UpstreamApiFormat::OpenAiResponses));
        assert!(!should_try_chat_fallback(&UpstreamApiFormat::OpenAiChatCompletions));
    }
}

fn responses_input_to_messages(body: &Value) -> Value {
    if let Some(messages) = body.get("messages") {
        return messages.clone();
    }

    let input = body.get("input");
    let mut collected = Vec::new();
    if let Some(text) = input.and_then(Value::as_str) {
        collected.push(json!({
            "role": "user",
            "content": text,
        }));
        return Value::Array(collected);
    }

    if let Some(array) = input.and_then(Value::as_array) {
        for item in array {
            match item {
                Value::String(text) => collected.push(json!({
                    "role": "user",
                    "content": text,
                })),
                Value::Object(map) => {
                    let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
                    let content = map.get("content").cloned().unwrap_or(Value::Null);
                    collected.push(json!({
                        "role": role,
                        "content": content,
                    }));
                }
                _ => {}
            }
        }
    }

    if collected.is_empty() {
        collected.push(json!({
            "role": "user",
            "content": "",
        }));
    }

    Value::Array(collected)
}
