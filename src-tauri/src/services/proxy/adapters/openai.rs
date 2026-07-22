use serde_json::{json, Value};

pub fn generate_response_id(prefix: &str) -> String {
    format!(
        "{prefix}-{}",
        crate::services::time::now_millis_for_services()
    )
}

pub fn extract_choice_text(value: &Value) -> Option<String> {
    value
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .map(ToString::to_string)
}

pub fn wrap_chat_response_as_responses(value: Value, fallback_model: Option<&str>) -> Value {
    let content = extract_choice_text(&value).unwrap_or_default();
    let model = value
        .get("model")
        .and_then(Value::as_str)
        .or(fallback_model)
        .unwrap_or("unknown-model");
    let created = value
        .get("created")
        .and_then(Value::as_i64)
        .unwrap_or_else(|| (crate::services::time::now_millis_for_services() / 1000) as i64);
    let usage = value.get("usage").cloned().unwrap_or(Value::Null);

    json!({
        "id": value.get("id").cloned().unwrap_or_else(|| Value::String(generate_response_id("response"))),
        "object": "response",
        "created": created,
        "model": model,
        "output": [{
            "id": generate_response_id("output"),
            "type": "message",
            "role": "assistant",
            "content": [{
                "type": "output_text",
                "text": content,
            }],
        }],
        "output_text": content,
        "usage": usage,
    })
}
