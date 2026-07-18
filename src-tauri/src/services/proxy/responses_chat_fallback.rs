use serde_json::{json, Value};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResponsesChatFallbackError {
    PreviousResponseUnsupported,
    ConversationUnsupported,
    StreamingUnsupported,
    BuiltInToolUnsupported(String),
    InvalidInput,
}

pub fn normalize_for_chat(body: &Value) -> Result<Value, ResponsesChatFallbackError> {
    if body.get("stream").and_then(Value::as_bool).unwrap_or(false) {
        return Err(ResponsesChatFallbackError::StreamingUnsupported);
    }
    normalize_for_chat_with_stream(body, false)
}

pub fn normalize_for_chat_streaming(body: &Value) -> Result<Value, ResponsesChatFallbackError> {
    normalize_for_chat_with_stream(body, true)
}

fn normalize_for_chat_with_stream(
    body: &Value,
    stream: bool,
) -> Result<Value, ResponsesChatFallbackError> {
    if body.get("previous_response_id").is_some() {
        return Err(ResponsesChatFallbackError::PreviousResponseUnsupported);
    }
    if body.get("conversation").is_some() {
        return Err(ResponsesChatFallbackError::ConversationUnsupported);
    }

    let mut output = serde_json::Map::new();
    copy(body, &mut output, "model", "model");
    copy(body, &mut output, "temperature", "temperature");
    copy(body, &mut output, "top_p", "top_p");
    copy(body, &mut output, "tool_choice", "tool_choice");
    copy(
        body,
        &mut output,
        "parallel_tool_calls",
        "parallel_tool_calls",
    );
    copy(body, &mut output, "prompt_cache_key", "prompt_cache_key");
    copy(
        body,
        &mut output,
        "prompt_cache_options",
        "prompt_cache_options",
    );
    copy(
        body,
        &mut output,
        "prompt_cache_retention",
        "prompt_cache_retention",
    );
    copy(
        body,
        &mut output,
        "max_output_tokens",
        "max_completion_tokens",
    );

    if let Some(effort) = body.pointer("/reasoning/effort").cloned() {
        output.insert("reasoning_effort".to_string(), effort);
    }
    output.insert("messages".to_string(), build_messages(body)?);
    if stream {
        output.insert("stream".to_string(), Value::Bool(true));
        output
            .entry("stream_options".to_string())
            .or_insert_with(|| {
                json!({
                    "include_usage": true,
                })
            });
    }
    if let Some(tools) = body.get("tools") {
        output.insert("tools".to_string(), convert_function_tools(tools)?);
    }
    Ok(Value::Object(output))
}

pub fn responses_fallback_error_message(error: &ResponsesChatFallbackError) -> String {
    match error {
        ResponsesChatFallbackError::PreviousResponseUnsupported => {
            "Responses-to-Chat fallback does not support previous_response_id".to_string()
        }
        ResponsesChatFallbackError::ConversationUnsupported => {
            "Responses-to-Chat fallback does not support conversation state".to_string()
        }
        ResponsesChatFallbackError::StreamingUnsupported => {
            "Responses-to-Chat fallback does not support streaming".to_string()
        }
        ResponsesChatFallbackError::BuiltInToolUnsupported(tool_type) => format!(
            "Responses-to-Chat fallback supports only function tools; unsupported tool type: {tool_type}"
        ),
        ResponsesChatFallbackError::InvalidInput => {
            "Responses-to-Chat fallback could not convert the input".to_string()
        }
    }
}

fn copy(body: &Value, output: &mut serde_json::Map<String, Value>, from: &str, to: &str) {
    if let Some(value) = body.get(from) {
        output.insert(to.to_string(), value.clone());
    }
}

fn build_messages(body: &Value) -> Result<Value, ResponsesChatFallbackError> {
    let mut messages = Vec::new();
    if let Some(instructions) = body.get("instructions").and_then(Value::as_str) {
        if !instructions.trim().is_empty() {
            messages.push(json!({
                "role": "developer",
                "content": instructions,
            }));
        }
    }

    if let Some(existing) = body.get("messages") {
        let Some(existing_messages) = existing.as_array() else {
            return Err(ResponsesChatFallbackError::InvalidInput);
        };
        messages.extend(existing_messages.iter().cloned());
        if messages.is_empty() {
            return Err(ResponsesChatFallbackError::InvalidInput);
        }
        return Ok(Value::Array(messages));
    }

    let Some(input) = body.get("input") else {
        return Err(ResponsesChatFallbackError::InvalidInput);
    };
    match input {
        Value::String(text) => messages.push(json!({
            "role": "user",
            "content": text,
        })),
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(text) => messages.push(json!({
                        "role": "user",
                        "content": text,
                    })),
                    Value::Object(map) => {
                        let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
                        let content = map.get("content").cloned().unwrap_or(Value::Null);
                        messages.push(json!({
                            "role": role,
                            "content": content,
                        }));
                    }
                    _ => return Err(ResponsesChatFallbackError::InvalidInput),
                }
            }
        }
        Value::Object(map) => {
            let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = map.get("content").cloned().unwrap_or(Value::Null);
            messages.push(json!({
                "role": role,
                "content": content,
            }));
        }
        _ => return Err(ResponsesChatFallbackError::InvalidInput),
    }

    if messages.is_empty() {
        return Err(ResponsesChatFallbackError::InvalidInput);
    }
    Ok(Value::Array(messages))
}

fn convert_function_tools(tools: &Value) -> Result<Value, ResponsesChatFallbackError> {
    let Some(tools) = tools.as_array() else {
        return Err(ResponsesChatFallbackError::InvalidInput);
    };
    let mut converted = Vec::with_capacity(tools.len());
    for tool in tools {
        let Some(tool) = tool.as_object() else {
            return Err(ResponsesChatFallbackError::InvalidInput);
        };
        let tool_type = tool
            .get("type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        if tool_type != "function" {
            return Err(ResponsesChatFallbackError::BuiltInToolUnsupported(
                tool_type.to_string(),
            ));
        }

        let Some(name) = tool.get("name").cloned() else {
            return Err(ResponsesChatFallbackError::InvalidInput);
        };
        let mut function = serde_json::Map::new();
        function.insert("name".to_string(), name);
        if let Some(description) = tool.get("description") {
            function.insert("description".to_string(), description.clone());
        }
        if let Some(parameters) = tool.get("parameters") {
            function.insert("parameters".to_string(), parameters.clone());
        }
        if let Some(strict) = tool.get("strict") {
            function.insert("strict".to_string(), strict.clone());
        }
        converted.push(json!({
            "type": "function",
            "function": Value::Object(function),
        }));
    }
    Ok(Value::Array(converted))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn responses_chat_fallback_preserves_cache_and_tool_fields() {
        let body = json!({
            "model": "gpt-5.6",
            "instructions": "Use the repository rules.",
            "input": "Inspect the current change.",
            "tools": [{
                "type": "function",
                "name": "read_file",
                "description": "Read a file",
                "parameters": {"type": "object", "properties": {}}
            }],
            "tool_choice": "auto",
            "prompt_cache_key": "workspace-a",
            "prompt_cache_options": {"mode": "implicit"},
            "max_output_tokens": 512,
            "reasoning": {"effort": "high"}
        });

        let chat = normalize_for_chat(&body).expect("compatible fallback");
        assert_eq!(chat["prompt_cache_key"], "workspace-a");
        assert_eq!(chat["prompt_cache_options"]["mode"], "implicit");
        assert_eq!(chat["max_completion_tokens"], 512);
        assert_eq!(chat["reasoning_effort"], "high");
        assert_eq!(chat["messages"][0]["role"], "developer");
        assert_eq!(chat["tools"][0]["function"]["name"], "read_file");
    }

    #[test]
    fn responses_chat_fallback_rejects_server_side_continuation_and_streaming() {
        assert_eq!(
            normalize_for_chat(
                &json!({"model":"gpt-5.6","input":"x","previous_response_id":"resp_1"})
            )
            .unwrap_err(),
            ResponsesChatFallbackError::PreviousResponseUnsupported,
        );
        assert_eq!(
            normalize_for_chat(&json!({"model":"gpt-5.6","input":"x","stream":true})).unwrap_err(),
            ResponsesChatFallbackError::StreamingUnsupported,
        );
    }
}
