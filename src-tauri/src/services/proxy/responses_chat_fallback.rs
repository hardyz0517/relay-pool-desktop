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
        Value::String(text) => {
            push_text_message(&mut messages, "user", Value::String(text.clone()))?
        }
        Value::Array(items) => {
            for item in items {
                push_input_item(&mut messages, item)?;
            }
        }
        Value::Object(_) => push_input_item(&mut messages, input)?,
        _ => return Err(ResponsesChatFallbackError::InvalidInput),
    }

    if messages.is_empty() {
        return Err(ResponsesChatFallbackError::InvalidInput);
    }
    Ok(Value::Array(messages))
}

fn push_input_item(
    messages: &mut Vec<Value>,
    item: &Value,
) -> Result<(), ResponsesChatFallbackError> {
    let Value::Object(map) = item else {
        if let Value::String(text) = item {
            return push_text_message(messages, "user", Value::String(text.clone()));
        }
        return Err(ResponsesChatFallbackError::InvalidInput);
    };
    match map.get("type").and_then(Value::as_str) {
        Some("function_call") => push_function_call(messages, map),
        Some("function_call_output") => push_function_call_output(messages, map),
        Some("reasoning") => Ok(()),
        Some("message") | None => {
            let role = map.get("role").and_then(Value::as_str).unwrap_or("user");
            let content = map.get("content").cloned().unwrap_or(Value::Null);
            push_text_message(messages, role, content)
        }
        _ => Err(ResponsesChatFallbackError::InvalidInput),
    }
}

fn push_text_message(
    messages: &mut Vec<Value>,
    role: &str,
    content: Value,
) -> Result<(), ResponsesChatFallbackError> {
    let content = response_content_text(&content)?;
    messages.push(json!({
        "role": role,
        "content": content,
    }));
    Ok(())
}

fn push_function_call(
    messages: &mut Vec<Value>,
    item: &serde_json::Map<String, Value>,
) -> Result<(), ResponsesChatFallbackError> {
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or(ResponsesChatFallbackError::InvalidInput)?;
    let name = item
        .get("name")
        .and_then(Value::as_str)
        .ok_or(ResponsesChatFallbackError::InvalidInput)?;
    let arguments = item
        .get("arguments")
        .map(value_as_text)
        .transpose()?
        .unwrap_or_else(|| "{}".to_string());
    let tool_call = json!({
        "id": call_id,
        "type": "function",
        "function": {
            "name": name,
            "arguments": arguments,
        },
    });
    if let Some(last) = messages.last_mut().and_then(Value::as_object_mut) {
        if last.get("role").and_then(Value::as_str) == Some("assistant")
            && last.get("content").is_some_and(Value::is_null)
        {
            if let Some(tool_calls) = last.get_mut("tool_calls").and_then(Value::as_array_mut) {
                tool_calls.push(tool_call);
                return Ok(());
            }
        }
    }
    messages.push(json!({
        "role": "assistant",
        "content": Value::Null,
        "tool_calls": [tool_call],
    }));
    Ok(())
}

fn push_function_call_output(
    messages: &mut Vec<Value>,
    item: &serde_json::Map<String, Value>,
) -> Result<(), ResponsesChatFallbackError> {
    let call_id = item
        .get("call_id")
        .and_then(Value::as_str)
        .ok_or(ResponsesChatFallbackError::InvalidInput)?;
    let output = item
        .get("output")
        .map(value_as_text)
        .transpose()?
        .ok_or(ResponsesChatFallbackError::InvalidInput)?;
    messages.push(json!({
        "role": "tool",
        "tool_call_id": call_id,
        "content": output,
    }));
    Ok(())
}

fn response_content_text(content: &Value) -> Result<String, ResponsesChatFallbackError> {
    match content {
        Value::String(text) => Ok(text.clone()),
        Value::Array(parts) => {
            let mut text = String::new();
            for part in parts {
                match part {
                    Value::String(part) => text.push_str(part),
                    Value::Object(part) => {
                        let part_type = part.get("type").and_then(Value::as_str);
                        if matches!(part_type, Some("input_text" | "output_text" | "text")) {
                            text.push_str(
                                part.get("text")
                                    .and_then(Value::as_str)
                                    .ok_or(ResponsesChatFallbackError::InvalidInput)?,
                            );
                        } else {
                            return Err(ResponsesChatFallbackError::InvalidInput);
                        }
                    }
                    _ => return Err(ResponsesChatFallbackError::InvalidInput),
                }
            }
            Ok(text)
        }
        _ => Err(ResponsesChatFallbackError::InvalidInput),
    }
}

fn value_as_text(value: &Value) -> Result<String, ResponsesChatFallbackError> {
    match value {
        Value::String(text) => Ok(text.clone()),
        Value::Null => Err(ResponsesChatFallbackError::InvalidInput),
        value => serde_json::to_string(value).map_err(|_| ResponsesChatFallbackError::InvalidInput),
    }
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

    #[test]
    fn responses_chat_fallback_converts_function_call_round_trip() {
        let chat = normalize_for_chat_streaming(&json!({
            "model": "gpt-5.6",
            "stream": true,
            "input": [
                {
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "Where am I?"}]
                },
                {
                    "type": "reasoning",
                    "id": "reasoning-1"
                },
                {
                    "type": "function_call",
                    "call_id": "call_shell",
                    "name": "shell_command",
                    "arguments": "{\"command\":\"Get-Location\"}"
                },
                {
                    "type": "function_call_output",
                    "call_id": "call_shell",
                    "output": "E:\\\\Dev\\\\Projects"
                }
            ]
        }))
        .expect("function-call round trip");

        assert_eq!(chat["messages"][0]["role"], "user");
        assert_eq!(chat["messages"][0]["content"], "Where am I?");
        assert_eq!(chat["messages"][1]["role"], "assistant");
        assert_eq!(chat["messages"][1]["tool_calls"][0]["id"], "call_shell");
        assert_eq!(
            chat["messages"][1]["tool_calls"][0]["function"]["name"],
            "shell_command"
        );
        assert_eq!(chat["messages"][2]["role"], "tool");
        assert_eq!(chat["messages"][2]["tool_call_id"], "call_shell");
        assert_eq!(chat["messages"][2]["content"], "E:\\\\Dev\\\\Projects");
    }
}
