use std::{
    collections::{BTreeMap, VecDeque},
    pin::Pin,
    task::{Context, Poll},
};

use bytes::Bytes;
use futures_util::Stream;
use http::StatusCode;
use serde_json::{json, Value};

use super::{
    error::{FailureSource, ProxyFailure, ProxyFailureCode, RetryClass},
    request::ByteStream,
};

const MAX_PENDING_SSE_BYTES: usize = 256 * 1024;

pub(crate) fn chat_sse_to_responses_stream(stream: ByteStream, model: Option<&str>) -> ByteStream {
    let response_id = crate::services::proxy::adapters::openai::generate_response_id("response");
    Box::pin(ChatToResponsesStream {
        inner: stream,
        decoder: ResponsesChatStreamDecoder::new(model.unwrap_or("unknown-model"), &response_id),
        pending: VecDeque::new(),
        upstream_done: false,
    })
}

struct ChatToResponsesStream {
    inner: ByteStream,
    decoder: ResponsesChatStreamDecoder,
    pending: VecDeque<Bytes>,
    upstream_done: bool,
}

impl Stream for ChatToResponsesStream {
    type Item = Result<Bytes, ProxyFailure>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            if let Some(bytes) = self.pending.pop_front() {
                return Poll::Ready(Some(Ok(bytes)));
            }
            if self.upstream_done {
                return Poll::Ready(None);
            }

            match self.inner.as_mut().poll_next(cx) {
                Poll::Ready(Some(Ok(bytes))) => match self.decoder.push(&bytes) {
                    Ok(chunks) => self.pending.extend(chunks),
                    Err(failure) => return Poll::Ready(Some(Err(failure))),
                },
                Poll::Ready(Some(Err(failure))) => return Poll::Ready(Some(Err(failure))),
                Poll::Ready(None) => match self.decoder.finish() {
                    Ok(chunks) => {
                        self.pending.extend(chunks);
                        self.upstream_done = true;
                    }
                    Err(failure) => return Poll::Ready(Some(Err(failure))),
                },
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

#[derive(Debug)]
pub(crate) struct ResponsesChatStreamDecoder {
    model: String,
    response_id: String,
    pending: Vec<u8>,
    text: String,
    message_output_index: Option<i64>,
    next_output_index: i64,
    tool_calls: BTreeMap<i64, ToolCallState>,
    created: bool,
    completed: bool,
    usage: Option<Value>,
    sequence_number: i64,
}

#[derive(Debug, Clone)]
struct ToolCallState {
    item_id: String,
    call_id: String,
    name: String,
    arguments: String,
    output_index: i64,
}

impl ResponsesChatStreamDecoder {
    pub(crate) fn new(model: &str, response_id: &str) -> Self {
        Self {
            model: model.to_string(),
            response_id: response_id.to_string(),
            pending: Vec::new(),
            text: String::new(),
            message_output_index: None,
            next_output_index: 0,
            tool_calls: BTreeMap::new(),
            created: false,
            completed: false,
            usage: None,
            sequence_number: 0,
        }
    }

    pub(crate) fn push(&mut self, chunk: &[u8]) -> Result<Vec<Bytes>, ProxyFailure> {
        self.pending.extend_from_slice(chunk);
        if self.pending.len() > MAX_PENDING_SSE_BYTES {
            return Err(stream_failure(
                "upstream chat SSE event exceeded pending buffer limit",
            ));
        }

        let mut output = Vec::new();
        while let Some((boundary, delimiter_len)) = find_event_boundary(&self.pending) {
            let event = self.pending[..boundary].to_vec();
            self.pending.drain(..boundary + delimiter_len);
            output.extend(self.decode_event(&event)?);
        }
        Ok(output)
    }

    pub(crate) fn finish(&mut self) -> Result<Vec<Bytes>, ProxyFailure> {
        if !self.pending.iter().all(u8::is_ascii_whitespace) {
            return Err(stream_failure(
                "upstream chat SSE ended with a partial event",
            ));
        }
        self.pending.clear();
        self.complete_once()
    }

    fn decode_event(&mut self, event: &[u8]) -> Result<Vec<Bytes>, ProxyFailure> {
        let data = String::from_utf8_lossy(event)
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.trim().is_empty() {
            return Ok(Vec::new());
        }
        if data.trim() == "[DONE]" {
            return self.complete_once();
        }

        let value = serde_json::from_str::<Value>(&data).map_err(|error| {
            stream_failure(format!("upstream chat SSE data was not JSON: {error}"))
        })?;
        if let Some(usage) = value.get("usage").cloned() {
            self.usage = Some(normalize_usage(usage));
        }

        let mut output = Vec::new();
        let choices = value
            .get("choices")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        for choice in choices {
            let Some(delta) = choice.get("delta") else {
                continue;
            };
            if let Some(content) = delta.get("content").and_then(Value::as_str) {
                if !content.is_empty() {
                    output.extend(self.ensure_created()?);
                    let output_index = self.message_output_index();
                    self.text.push_str(content);
                    output.push(self.event_bytes(
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "response_id": self.response_id,
                            "output_index": output_index,
                            "content_index": 0,
                            "delta": content,
                        }),
                    )?);
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for tool_call in tool_calls {
                    let tool_index = tool_call.get("index").and_then(Value::as_i64).unwrap_or(0);
                    let call_id = tool_call.get("id").and_then(Value::as_str);
                    let name = tool_call.pointer("/function/name").and_then(Value::as_str);
                    let arguments = tool_call
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                        .unwrap_or_default();
                    output.extend(self.ensure_created()?);
                    if !self.tool_calls.contains_key(&tool_index) {
                        let output_index = self.take_output_index();
                        let state = ToolCallState {
                            item_id: format!("fc_{}_{}", self.response_id, tool_index),
                            call_id: call_id.map(ToString::to_string).unwrap_or_else(|| {
                                format!("call_{}_{}", self.response_id, tool_index)
                            }),
                            name: name.unwrap_or("unknown_function").to_string(),
                            arguments: String::new(),
                            output_index,
                        };
                        output.push(self.event_bytes(
                            "response.output_item.added",
                            json!({
                                "type": "response.output_item.added",
                                "response_id": self.response_id,
                                "output_index": state.output_index,
                                "item": function_call_item(&state, "in_progress"),
                            }),
                        )?);
                        self.tool_calls.insert(tool_index, state);
                    }
                    let state = self
                        .tool_calls
                        .get_mut(&tool_index)
                        .expect("tool call state inserted");
                    if let Some(call_id) = call_id {
                        state.call_id = call_id.to_string();
                    }
                    if let Some(name) = name {
                        state.name = name.to_string();
                    }
                    state.arguments.push_str(arguments);
                    if !arguments.is_empty() {
                        let item_id = state.item_id.clone();
                        let output_index = state.output_index;
                        output.push(self.event_bytes(
                            "response.function_call_arguments.delta",
                            json!({
                                "type": "response.function_call_arguments.delta",
                                "response_id": self.response_id,
                                "item_id": item_id,
                                "output_index": output_index,
                                "delta": arguments,
                            }),
                        )?);
                    }
                }
            }
        }
        Ok(output)
    }

    fn message_output_index(&mut self) -> i64 {
        if let Some(output_index) = self.message_output_index {
            return output_index;
        }
        let output_index = self.take_output_index();
        self.message_output_index = Some(output_index);
        output_index
    }

    fn take_output_index(&mut self) -> i64 {
        let output_index = self.next_output_index;
        self.next_output_index = self.next_output_index.saturating_add(1);
        output_index
    }

    fn ensure_created(&mut self) -> Result<Vec<Bytes>, ProxyFailure> {
        if self.created {
            return Ok(Vec::new());
        }
        self.created = true;
        Ok(vec![self.event_bytes(
            "response.created",
            json!({
                "type": "response.created",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created": crate::services::time::now_millis_for_services() / 1000,
                    "model": self.model,
                    "status": "in_progress",
                }
            }),
        )?])
    }

    fn complete_once(&mut self) -> Result<Vec<Bytes>, ProxyFailure> {
        if self.completed {
            return Ok(Vec::new());
        }
        let mut output = self.ensure_created()?;
        self.completed = true;
        let tool_calls = self.tool_calls.values().cloned().collect::<Vec<_>>();
        for tool_call in &tool_calls {
            output.push(self.event_bytes(
                "response.function_call_arguments.done",
                json!({
                    "type": "response.function_call_arguments.done",
                    "response_id": self.response_id,
                    "item_id": tool_call.item_id,
                    "output_index": tool_call.output_index,
                    "name": tool_call.name,
                    "arguments": tool_call.arguments,
                }),
            )?);
            output.push(self.event_bytes(
                "response.output_item.done",
                json!({
                    "type": "response.output_item.done",
                    "response_id": self.response_id,
                    "output_index": tool_call.output_index,
                    "item": function_call_item(tool_call, "completed"),
                }),
            )?);
        }
        let mut response_output = tool_calls
            .iter()
            .map(|tool_call| {
                (
                    tool_call.output_index,
                    function_call_item(tool_call, "completed"),
                )
            })
            .collect::<Vec<_>>();
        if !self.text.is_empty() || response_output.is_empty() {
            response_output.push((
                self.message_output_index(),
                json!({
                    "id": crate::services::proxy::adapters::openai::generate_response_id("output"),
                    "type": "message",
                    "status": "completed",
                    "role": "assistant",
                    "content": [{
                        "type": "output_text",
                        "text": self.text,
                        "annotations": [],
                    }],
                }),
            ));
        }
        response_output.sort_by_key(|(output_index, _)| *output_index);
        let response_output = response_output
            .into_iter()
            .map(|(_, item)| item)
            .collect::<Vec<_>>();
        output.push(self.event_bytes(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created": crate::services::time::now_millis_for_services() / 1000,
                    "model": self.model,
                    "status": "completed",
                    "output": response_output,
                    "output_text": self.text,
                    "usage": self.usage.clone().unwrap_or(Value::Null),
                }
            }),
        )?);
        Ok(output)
    }

    fn event_bytes(&mut self, event: &str, mut data: Value) -> Result<Bytes, ProxyFailure> {
        if let Some(object) = data.as_object_mut() {
            object
                .entry("sequence_number".to_string())
                .or_insert_with(|| Value::from(self.sequence_number));
        }
        self.sequence_number = self.sequence_number.saturating_add(1);
        let data = serde_json::to_string(&data)
            .map_err(|error| stream_failure(format!("serialize Responses SSE failed: {error}")))?;
        Ok(Bytes::from(format!("event: {event}\ndata: {data}\n\n")))
    }
}

fn function_call_item(tool_call: &ToolCallState, status: &str) -> Value {
    json!({
        "id": tool_call.item_id,
        "type": "function_call",
        "status": status,
        "call_id": tool_call.call_id,
        "name": tool_call.name,
        "arguments": tool_call.arguments,
    })
}

fn normalize_usage(usage: Value) -> Value {
    let input_tokens = integer(&usage, &["input_tokens", "prompt_tokens"]);
    let output_tokens = integer(&usage, &["output_tokens", "completion_tokens"]);
    let total_tokens = integer(&usage, &["total_tokens"]).or_else(|| {
        input_tokens
            .zip(output_tokens)
            .map(|(input, output)| input + output)
    });
    json!({
        "input_tokens": input_tokens,
        "output_tokens": output_tokens,
        "total_tokens": total_tokens,
        "prompt_tokens": input_tokens,
        "completion_tokens": output_tokens,
    })
}

fn integer(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
}

fn find_event_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    let lf = bytes.windows(2).position(|window| window == b"\n\n");
    let crlf = bytes.windows(4).position(|window| window == b"\r\n\r\n");
    match (lf, crlf) {
        (Some(left), Some(right)) if left <= right => Some((left, 2)),
        (Some(_), Some(right)) => Some((right, 4)),
        (Some(left), None) => Some((left, 2)),
        (None, Some(right)) => Some((right, 4)),
        (None, None) => None,
    }
}

fn stream_failure(message: impl Into<String>) -> ProxyFailure {
    ProxyFailure::new(
        ProxyFailureCode::UpstreamStreamFailed,
        FailureSource::Upstream,
        RetryClass::AfterCommitStop,
        StatusCode::BAD_GATEWAY,
        message,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chat_sse_decoder_emits_valid_responses_events_across_split_chunks() {
        let mut decoder = ResponsesChatStreamDecoder::new("gpt-test", "resp_test");

        let first = decoder
            .push(b"data: {\"choices\":[{\"delta\":{\"content\":\"Hel\"}}]}\n")
            .expect("first chunk");
        assert!(first.is_empty());

        let second = decoder
            .push(
                b"\ndata: {\"choices\":[{\"delta\":{\"content\":\"lo\"}}],\"usage\":{\"prompt_tokens\":5,\"completion_tokens\":1,\"total_tokens\":6}}\n\ndata: [DONE]\n\n",
            )
            .expect("second chunk");
        let text = String::from_utf8(second.concat()).expect("utf8");

        assert!(text.contains("response.created"));
        assert!(text.contains("response.output_text.delta"));
        assert!(text.contains("response.completed"));
        assert!(text.contains("Hello"));
        assert!(text.contains("input_tokens"));
        assert!(text.contains("output_tokens"));
        assert_eq!(text.matches("response.completed").count(), 2);
    }

    #[test]
    fn chat_sse_decoder_rejects_malformed_json() {
        let mut decoder = ResponsesChatStreamDecoder::new("gpt-test", "resp_test");

        let failure = decoder
            .push(b"data: {bad json}\n\n")
            .expect_err("malformed SSE payload should fail");

        assert_eq!(
            failure.code,
            crate::services::proxy::error::ProxyFailureCode::UpstreamStreamFailed
        );
    }

    #[test]
    fn chat_sse_decoder_emits_complete_function_call_lifecycle() {
        let mut decoder = ResponsesChatStreamDecoder::new("gpt-test", "resp_tool");

        let first = decoder
            .push(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"id\":\"call_shell\",\"type\":\"function\",\"function\":{\"name\":\"shell_command\",\"arguments\":\"{\\\"command\\\":\"}}]}}]}\n\n")
            .expect("first tool chunk");
        let second = decoder
            .push(b"data: {\"choices\":[{\"delta\":{\"tool_calls\":[{\"index\":0,\"function\":{\"arguments\":\"\\\"Get-Location\\\"}\"}}]}}]}\n\ndata: [DONE]\n\n")
            .expect("final tool chunk");
        let text = String::from_utf8([first, second].concat().concat()).expect("utf8");

        assert!(text.contains("response.output_item.added"));
        assert!(text.contains("response.function_call_arguments.delta"));
        assert!(text.contains("response.function_call_arguments.done"));
        assert!(text.contains("response.output_item.done"));
        assert!(text.contains("\"type\":\"function_call\""));
        assert!(text.contains("\"call_id\":\"call_shell\""));
        assert!(text.contains("\"name\":\"shell_command\""));
        assert!(text.contains("{\\\"command\\\":\\\"Get-Location\\\"}"));
        assert!(!text.contains("call_unknown"));
        assert!(text.contains("\"sequence_number\":"));
    }
}
