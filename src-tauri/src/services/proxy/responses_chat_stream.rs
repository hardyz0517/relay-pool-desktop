use std::{
    collections::VecDeque,
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
    created: bool,
    completed: bool,
    usage: Option<Value>,
}

impl ResponsesChatStreamDecoder {
    pub(crate) fn new(model: &str, response_id: &str) -> Self {
        Self {
            model: model.to_string(),
            response_id: response_id.to_string(),
            pending: Vec::new(),
            text: String::new(),
            created: false,
            completed: false,
            usage: None,
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
                    self.text.push_str(content);
                    output.push(self.event_bytes(
                        "response.output_text.delta",
                        json!({
                            "type": "response.output_text.delta",
                            "response_id": self.response_id,
                            "output_index": 0,
                            "content_index": 0,
                            "delta": content,
                        }),
                    )?);
                }
            }
            if let Some(tool_calls) = delta.get("tool_calls").and_then(Value::as_array) {
                for tool_call in tool_calls {
                    if let Some(arguments) = tool_call
                        .pointer("/function/arguments")
                        .and_then(Value::as_str)
                    {
                        output.extend(self.ensure_created()?);
                        output.push(self.event_bytes(
                            "response.function_call_arguments.delta",
                            json!({
                                "type": "response.function_call_arguments.delta",
                                "response_id": self.response_id,
                                "item_id": tool_call.get("id").and_then(Value::as_str).unwrap_or("call_unknown"),
                                "output_index": tool_call.get("index").and_then(Value::as_i64).unwrap_or(0),
                                "delta": arguments,
                            }),
                        )?);
                    }
                }
            }
        }
        Ok(output)
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
                    "created": crate::services::database::now_millis_for_services() / 1000,
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
        output.push(self.event_bytes(
            "response.completed",
            json!({
                "type": "response.completed",
                "response": {
                    "id": self.response_id,
                    "object": "response",
                    "created": crate::services::database::now_millis_for_services() / 1000,
                    "model": self.model,
                    "status": "completed",
                    "output": [{
                        "id": crate::services::proxy::adapters::openai::generate_response_id("output"),
                        "type": "message",
                        "role": "assistant",
                        "content": [{
                            "type": "output_text",
                            "text": self.text,
                        }],
                    }],
                    "output_text": self.text,
                    "usage": self.usage.clone().unwrap_or(Value::Null),
                }
            }),
        )?);
        Ok(output)
    }

    fn event_bytes(&self, event: &str, data: Value) -> Result<Bytes, ProxyFailure> {
        let data = serde_json::to_string(&data)
            .map_err(|error| stream_failure(format!("serialize Responses SSE failed: {error}")))?;
        Ok(Bytes::from(format!("event: {event}\ndata: {data}\n\n")))
    }
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
}
