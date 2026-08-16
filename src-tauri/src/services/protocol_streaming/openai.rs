use std::collections::HashSet;

use serde_json::Value;

use super::{SseEvent, StreamError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum OpenAiStreamEventKind {
    ResponsesCreated,
    ResponsesOutputTextDelta,
    ResponsesOutputTextDone,
    ResponsesCompleted,
    ResponsesFailed,
    ResponsesIncomplete,
    ChatChunk,
    ChatDone,
    Error,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct OpenAiStreamSummary {
    pub output_text: String,
    pub model: Option<String>,
    pub usage: Option<OpenAiUsage>,
    pub terminal_seen: bool,
    pub last_event_kind: Option<OpenAiStreamEventKind>,
    pub ignored_event_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenAiResponsesReducer {
    max_output_bytes: usize,
    output_text: String,
    model: Option<String>,
    usage: Option<OpenAiUsage>,
    output_text_delta_parts: HashSet<OutputTextPartKey>,
    state: ReducerState,
    last_event_kind: Option<OpenAiStreamEventKind>,
    ignored_event_count: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct OpenAiChatReducer {
    max_output_bytes: usize,
    output_text: String,
    model: Option<String>,
    usage: Option<OpenAiUsage>,
    state: ReducerState,
    done_seen: bool,
    finish_reason_seen: bool,
    last_event_kind: Option<OpenAiStreamEventKind>,
    ignored_event_count: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ReducerState {
    Active,
    Completed,
    Failed(StreamError),
}

/// Typed event indexes are sufficient to identify an output text part while
/// avoiding retention of provider-controlled IDs or text.
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
struct OutputTextPartKey {
    output_index: Option<u64>,
    content_index: Option<u64>,
}

impl OpenAiResponsesReducer {
    pub(crate) fn new(max_output_bytes: usize) -> Self {
        Self {
            max_output_bytes,
            output_text: String::new(),
            model: None,
            usage: None,
            output_text_delta_parts: HashSet::new(),
            state: ReducerState::Active,
            last_event_kind: None,
            ignored_event_count: 0,
        }
    }

    pub(crate) fn push(&mut self, event: &SseEvent) -> Result<(), StreamError> {
        if let ReducerState::Failed(error) = self.state {
            return Err(error);
        }
        if self.state == ReducerState::Completed {
            return Ok(());
        }

        let value = match serde_json::from_str::<Value>(&event.data) {
            Ok(value) => value,
            Err(_) => return Err(self.fail(StreamError::InvalidEventJson)),
        };
        if value.get("error").is_some()
            || value.get("type").and_then(Value::as_str) == Some("error")
        {
            self.last_event_kind = Some(OpenAiStreamEventKind::Error);
            return Err(self.fail(StreamError::UpstreamFailedEvent));
        }

        match value.get("type").and_then(Value::as_str) {
            Some("response.created") => {
                self.last_event_kind = Some(OpenAiStreamEventKind::ResponsesCreated);
                self.capture_response_metadata(value.get("response").unwrap_or(&value));
            }
            Some("response.output_text.delta") => {
                self.last_event_kind = Some(OpenAiStreamEventKind::ResponsesOutputTextDelta);
                if let Some(delta) = value.get("delta").and_then(Value::as_str) {
                    self.append_output(delta)?;
                    self.output_text_delta_parts
                        .insert(output_text_part_key(&value));
                }
            }
            Some("response.output_text.done") => {
                self.last_event_kind = Some(OpenAiStreamEventKind::ResponsesOutputTextDone);
                // Standard streams provide deltas before `done`, in which case
                // `done.text` is a duplicate. Some compatible providers emit only
                // `done`; retain that text as a safe per-output-part fallback.
                if let Some(text) = value.get("text").and_then(Value::as_str) {
                    let part = output_text_part_key(&value);
                    if !self.output_text_delta_parts.contains(&part) {
                        self.append_output(text)?;
                    }
                }
            }
            Some("response.completed") => {
                self.last_event_kind = Some(OpenAiStreamEventKind::ResponsesCompleted);
                self.capture_response_metadata(value.get("response").unwrap_or(&value));
                self.state = ReducerState::Completed;
            }
            Some("response.failed") => {
                self.last_event_kind = Some(OpenAiStreamEventKind::ResponsesFailed);
                return Err(self.fail(StreamError::UpstreamFailedEvent));
            }
            Some("response.incomplete") => {
                self.last_event_kind = Some(OpenAiStreamEventKind::ResponsesIncomplete);
                return Err(self.fail(StreamError::UpstreamIncompleteEvent));
            }
            _ => {
                self.last_event_kind = Some(OpenAiStreamEventKind::Unknown);
                self.ignored_event_count = self.ignored_event_count.saturating_add(1);
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<OpenAiStreamSummary, StreamError> {
        match self.state {
            ReducerState::Completed => Ok(OpenAiStreamSummary {
                output_text: self.output_text,
                model: self.model,
                usage: self.usage,
                terminal_seen: true,
                last_event_kind: self.last_event_kind,
                ignored_event_count: self.ignored_event_count,
            }),
            ReducerState::Active => Err(StreamError::MissingTerminalEvent),
            ReducerState::Failed(error) => Err(error),
        }
    }

    fn append_output(&mut self, text: &str) -> Result<(), StreamError> {
        if self.output_text.len().saturating_add(text.len()) > self.max_output_bytes {
            return Err(self.fail(StreamError::OutputLimit));
        }
        self.output_text.push_str(text);
        Ok(())
    }

    fn capture_response_metadata(&mut self, response: &Value) {
        self.model = self
            .model
            .take()
            .or_else(|| bounded_string_field(response, "model"));
        self.usage = self
            .usage
            .take()
            .or_else(|| responses_usage(response.get("usage")));
    }

    fn fail(&mut self, error: StreamError) -> StreamError {
        self.state = ReducerState::Failed(error);
        error
    }
}

/// Reduces a non-streaming OpenAI Responses JSON body into the same bounded
/// output/usage contract used by the streaming reducer. HTTP status and
/// content-type classification intentionally stay with the caller.
pub(crate) fn parse_openai_responses_json(
    body: &[u8],
    max_output_bytes: usize,
) -> Result<OpenAiStreamSummary, StreamError> {
    let value = serde_json::from_slice::<Value>(body).map_err(|_| StreamError::InvalidEventJson)?;
    if value.get("error").is_some() {
        return Err(StreamError::UpstreamFailedEvent);
    }

    match value.get("status").and_then(Value::as_str) {
        Some("failed") => Err(StreamError::UpstreamFailedEvent),
        Some("incomplete") => Err(StreamError::UpstreamIncompleteEvent),
        Some("completed") => {
            let mut reducer = OpenAiResponsesReducer::new(max_output_bytes);
            reducer.capture_response_metadata(&value);
            append_responses_json_output(&mut reducer, &value)?;
            reducer.last_event_kind = Some(OpenAiStreamEventKind::ResponsesCompleted);
            reducer.state = ReducerState::Completed;
            reducer.finish()
        }
        _ => Err(StreamError::MissingTerminalEvent),
    }
}

impl OpenAiChatReducer {
    pub(crate) fn new(max_output_bytes: usize) -> Self {
        Self {
            max_output_bytes,
            output_text: String::new(),
            model: None,
            usage: None,
            state: ReducerState::Active,
            done_seen: false,
            finish_reason_seen: false,
            last_event_kind: None,
            ignored_event_count: 0,
        }
    }

    pub(crate) fn push(&mut self, event: &SseEvent) -> Result<(), StreamError> {
        if let ReducerState::Failed(error) = self.state {
            return Err(error);
        }
        if self.done_seen {
            return Ok(());
        }
        if event.data.trim() == "[DONE]" {
            self.done_seen = true;
            self.last_event_kind = Some(OpenAiStreamEventKind::ChatDone);
            if self.finish_reason_seen {
                self.state = ReducerState::Completed;
            }
            return Ok(());
        }

        let value = match serde_json::from_str::<Value>(&event.data) {
            Ok(value) => value,
            Err(_) => return Err(self.fail(StreamError::InvalidEventJson)),
        };
        if value.get("error").is_some() {
            self.last_event_kind = Some(OpenAiStreamEventKind::Error);
            return Err(self.fail(StreamError::UpstreamFailedEvent));
        }

        self.last_event_kind = Some(OpenAiStreamEventKind::ChatChunk);
        self.model = self
            .model
            .take()
            .or_else(|| bounded_string_field(&value, "model"));
        self.usage = self.usage.take().or_else(|| chat_usage(value.get("usage")));

        let Some(choices) = value.get("choices").and_then(Value::as_array) else {
            return Err(self.fail(StreamError::InvalidSseFraming));
        };
        for choice in choices {
            if let Some(content) = choice
                .get("delta")
                .and_then(|delta| delta.get("content"))
                .and_then(Value::as_str)
            {
                self.append_output(content)?;
            }
            if choice
                .get("finish_reason")
                .is_some_and(|reason| !reason.is_null())
            {
                self.finish_reason_seen = true;
            }
        }
        Ok(())
    }

    pub(crate) fn finish(self) -> Result<OpenAiStreamSummary, StreamError> {
        match self.state {
            ReducerState::Completed if self.done_seen && self.finish_reason_seen => {
                Ok(OpenAiStreamSummary {
                    output_text: self.output_text,
                    model: self.model,
                    usage: self.usage,
                    terminal_seen: true,
                    last_event_kind: self.last_event_kind,
                    ignored_event_count: self.ignored_event_count,
                })
            }
            ReducerState::Failed(error) => Err(error),
            ReducerState::Active | ReducerState::Completed => {
                Err(StreamError::MissingTerminalEvent)
            }
        }
    }

    fn append_output(&mut self, text: &str) -> Result<(), StreamError> {
        if self.output_text.len().saturating_add(text.len()) > self.max_output_bytes {
            return Err(self.fail(StreamError::OutputLimit));
        }
        self.output_text.push_str(text);
        Ok(())
    }

    fn fail(&mut self, error: StreamError) -> StreamError {
        self.state = ReducerState::Failed(error);
        error
    }
}

fn bounded_string_field(value: &Value, field: &str) -> Option<String> {
    let value = value.get(field)?.as_str()?;
    (value.len() <= 256).then(|| value.to_string())
}

fn responses_usage(value: Option<&Value>) -> Option<OpenAiUsage> {
    let value = value?;
    Some(OpenAiUsage {
        input_tokens: integer_field(value, "input_tokens"),
        output_tokens: integer_field(value, "output_tokens"),
        total_tokens: integer_field(value, "total_tokens"),
        cache_creation_tokens: integer_field(value, "cache_creation_input_tokens"),
        cache_read_tokens: integer_field(value, "cached_input_tokens"),
    })
}

fn chat_usage(value: Option<&Value>) -> Option<OpenAiUsage> {
    let value = value?;
    Some(OpenAiUsage {
        input_tokens: integer_field(value, "prompt_tokens"),
        output_tokens: integer_field(value, "completion_tokens"),
        total_tokens: integer_field(value, "total_tokens"),
        cache_creation_tokens: integer_field(value, "cache_creation_tokens"),
        cache_read_tokens: integer_field(value, "cache_read_tokens"),
    })
}

fn integer_field(value: &Value, field: &str) -> Option<i64> {
    value.get(field).and_then(Value::as_i64)
}

fn output_text_part_key(value: &Value) -> OutputTextPartKey {
    OutputTextPartKey {
        output_index: value.get("output_index").and_then(Value::as_u64),
        content_index: value.get("content_index").and_then(Value::as_u64),
    }
}

fn append_responses_json_output(
    reducer: &mut OpenAiResponsesReducer,
    value: &Value,
) -> Result<(), StreamError> {
    let mut found_output_text = false;
    if let Some(items) = value.get("output").and_then(Value::as_array) {
        for item in items {
            if let Some(content) = item.get("content").and_then(Value::as_array) {
                for block in content {
                    if block.get("type").and_then(Value::as_str) == Some("output_text") {
                        if let Some(text) = block.get("text").and_then(Value::as_str) {
                            reducer.append_output(text)?;
                            found_output_text = true;
                        }
                    }
                }
            }
        }
    }
    if !found_output_text {
        if let Some(text) = value.get("output_text").and_then(Value::as_str) {
            reducer.append_output(text)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{OpenAiChatReducer, OpenAiResponsesReducer, OpenAiStreamEventKind};
    use crate::services::protocol_streaming::{SseDecoder, SseEvent, SseLimits, StreamError};

    fn event(data: &str) -> SseEvent {
        SseEvent {
            data: data.to_string(),
        }
    }

    #[test]
    fn responses_reducer_ignores_reasoning_and_unknown_events_but_keeps_output_and_usage() {
        let mut reducer = OpenAiResponsesReducer::new(128);
        for data in [
            r#"{"type":"response.created","response":{"model":"model-test"}}"#,
            r#"{"type":"response.reasoning_summary_text.delta","delta":"private reasoning"}"#,
            r#"{"type":"response.output_text.delta","delta":"RP_"}"#,
            r#"{"type":"response.output_text.done","text":"RP_ANSWER=42"}"#,
            r#"{"type":"response.output_text.delta","delta":"ANSWER=42"}"#,
            r#"{"type":"provider.extension"}"#,
            r#"{"type":"response.completed","response":{"usage":{"input_tokens":2,"output_tokens":3,"total_tokens":5}}}"#,
        ] {
            reducer.push(&event(data)).expect("legal event");
        }

        let summary = reducer.finish().expect("completed response");
        assert_eq!(summary.output_text, "RP_ANSWER=42");
        assert_eq!(summary.model.as_deref(), Some("model-test"));
        assert_eq!(summary.usage.expect("usage").total_tokens, Some(5));
        assert_eq!(summary.ignored_event_count, 2);
        assert_eq!(
            summary.last_event_kind,
            Some(OpenAiStreamEventKind::ResponsesCompleted)
        );
    }

    #[test]
    fn responses_terminal_failure_is_irreversible() {
        let mut reducer = OpenAiResponsesReducer::new(128);
        assert_eq!(
            reducer.push(&event(r#"{"type":"response.failed"}"#)),
            Err(StreamError::UpstreamFailedEvent)
        );
        assert_eq!(
            reducer.push(&event(r#"{"type":"response.completed"}"#)),
            Err(StreamError::UpstreamFailedEvent)
        );
        assert_eq!(reducer.finish(), Err(StreamError::UpstreamFailedEvent));
    }

    #[test]
    fn responses_requires_explicit_success_terminal_and_classifies_incomplete() {
        let mut unfinished = OpenAiResponsesReducer::new(128);
        unfinished
            .push(&event(
                r#"{"type":"response.output_text.delta","delta":"answer"}"#,
            ))
            .expect("delta");
        assert_eq!(unfinished.finish(), Err(StreamError::MissingTerminalEvent));

        let mut incomplete = OpenAiResponsesReducer::new(128);
        assert_eq!(
            incomplete.push(&event(r#"{"type":"response.incomplete"}"#)),
            Err(StreamError::UpstreamIncompleteEvent)
        );
    }

    #[test]
    fn responses_output_limit_counts_output_not_reasoning_or_event_size() {
        let mut reducer = OpenAiResponsesReducer::new(2);
        reducer
            .push(&event(
                r#"{"type":"response.reasoning_summary_text.delta","delta":"many private bytes"}"#,
            ))
            .expect("reasoning is ignored");
        assert_eq!(
            reducer.push(&event(
                r#"{"type":"response.output_text.delta","delta":"abc"}"#
            )),
            Err(StreamError::OutputLimit)
        );
    }

    #[test]
    fn responses_done_text_is_a_fallback_only_when_its_part_has_no_delta() {
        let mut reducer = OpenAiResponsesReducer::new(128);
        reducer
            .push(&event(
                r#"{"type":"response.output_text.done","output_index":0,"content_index":0,"text":"fallback answer"}"#,
            ))
            .expect("done-only provider event");
        reducer
            .push(&event(r#"{"type":"response.completed"}"#))
            .expect("completed");
        assert_eq!(
            reducer.finish().expect("terminal summary").output_text,
            "fallback answer"
        );

        let mut with_delta = OpenAiResponsesReducer::new(128);
        with_delta
            .push(&event(
                r#"{"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":"delta answer"}"#,
            ))
            .expect("delta");
        with_delta
            .push(&event(
                r#"{"type":"response.output_text.done","output_index":0,"content_index":0,"text":"delta answer"}"#,
            ))
            .expect("done");
        with_delta
            .push(&event(r#"{"type":"response.completed"}"#))
            .expect("completed");
        assert_eq!(
            with_delta.finish().expect("terminal summary").output_text,
            "delta answer"
        );
    }

    #[test]
    fn non_stream_responses_uses_the_same_bounded_output_and_usage_contract() {
        let summary = super::parse_openai_responses_json(
            br#"{"status":"completed","model":"model-test","output":[{"content":[{"type":"output_text","text":"RP_ANSWER=42"}]}],"usage":{"input_tokens":2,"output_tokens":3,"total_tokens":5}}"#,
            128,
        )
        .expect("completed response");
        assert_eq!(summary.output_text, "RP_ANSWER=42");
        assert_eq!(summary.usage.expect("usage").total_tokens, Some(5));

        assert_eq!(
            super::parse_openai_responses_json(
                br#"{"status":"completed","output_text":"too large"}"#,
                3,
            ),
            Err(StreamError::OutputLimit)
        );
    }

    #[test]
    fn chat_requires_finish_reason_and_done_and_rejects_errors() {
        let mut reducer = OpenAiChatReducer::new(128);
        reducer
            .push(&event(r#"{"model":"model-test","choices":[{"delta":{"content":"RP_ANSWER=42"},"finish_reason":"stop"}]}"#))
            .expect("chat chunk");
        reducer.push(&event("[DONE]")).expect("done marker");
        let summary = reducer.finish().expect("complete chat");
        assert_eq!(summary.output_text, "RP_ANSWER=42");

        let mut missing_finish = OpenAiChatReducer::new(128);
        missing_finish.push(&event("[DONE]")).expect("done marker");
        assert_eq!(
            missing_finish.finish(),
            Err(StreamError::MissingTerminalEvent)
        );

        let mut failed = OpenAiChatReducer::new(128);
        assert_eq!(
            failed.push(&event(r#"{"error":{"code":"bad"},"choices":[]}"#)),
            Err(StreamError::UpstreamFailedEvent)
        );
    }

    #[test]
    fn decoder_and_responses_reducer_accept_a_large_reasoning_stream() {
        let limits = SseLimits {
            max_pending_event_bytes: 512,
            max_total_stream_bytes: 256 * 1024,
            max_sse_events: 1_000,
        };
        let mut decoder = SseDecoder::new(limits);
        let mut reducer = OpenAiResponsesReducer::new(64);
        let reasoning = b"data: {\"type\":\"response.reasoning_summary_text.delta\",\"delta\":\"untrusted reasoning must not become answer padding-padding\"}\n\n";
        for _ in 0..600 {
            for event in decoder.push(reasoning).expect("complete event") {
                reducer.push(&event).expect("ignored reasoning event");
            }
        }
        for event in decoder
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"ok\"}\n\ndata: {\"type\":\"response.completed\"}\n\n")
            .expect("terminal events")
        {
            reducer.push(&event).expect("legal response event");
        }
        decoder.finish().expect("complete framing");
        let summary = reducer.finish().expect("completed reducer");

        assert!(decoder.stats().total_stream_bytes > 64 * 1024);
        assert_eq!(summary.output_text, "ok");
        assert_eq!(summary.ignored_event_count, 600);
    }
}
