use serde_json::Value;

const MAX_PENDING_SSE_BYTES: usize = 256 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RequestObservation {
    pub reasoning_effort: Option<String>,
    pub uses_reasoning: bool,
}

impl RequestObservation {
    pub fn from_json(body: &Value) -> Self {
        let reasoning_effort = body
            .pointer("/reasoning/effort")
            .or_else(|| body.get("reasoning_effort"))
            .and_then(Value::as_str)
            .and_then(normalize_reasoning_effort);
        let uses_reasoning =
            body.get("reasoning").is_some() || body.get("reasoning_effort").is_some();
        Self {
            reasoning_effort,
            uses_reasoning,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ObservedUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub total_tokens: Option<i64>,
    pub cache_creation_tokens: Option<i64>,
    pub cache_read_tokens: Option<i64>,
}

impl ObservedUsage {
    pub fn from_json(value: &Value) -> Option<Self> {
        let usage = value
            .get("usage")
            .or_else(|| value.pointer("/response/usage"))?;
        let input_tokens = integer(usage, &["input_tokens", "prompt_tokens"]);
        let output_tokens = integer(usage, &["output_tokens", "completion_tokens"]);
        let total_tokens = integer(usage, &["total_tokens"]).or_else(|| {
            input_tokens
                .zip(output_tokens)
                .map(|(input, output)| input + output)
        });
        let cache_creation_tokens = integer(
            usage,
            &[
                "cache_write_tokens",
                "cache_creation_tokens",
                "cache_creation_input_tokens",
            ],
        )
        .or_else(|| integer_at(usage, "/input_tokens_details/cache_write_tokens"))
        .or_else(|| integer_at(usage, "/prompt_tokens_details/cache_write_tokens"))
        .or_else(|| integer_at(usage, "/input_tokens_details/cache_creation_tokens"))
        .or_else(|| integer_at(usage, "/prompt_tokens_details/cache_creation_tokens"));
        let cache_read_tokens = integer(usage, &["cache_read_tokens", "cache_read_input_tokens"])
            .or_else(|| integer_at(usage, "/input_tokens_details/cached_tokens"))
            .or_else(|| integer_at(usage, "/prompt_tokens_details/cached_tokens"));

        if input_tokens.is_none()
            && output_tokens.is_none()
            && total_tokens.is_none()
            && cache_creation_tokens.is_none()
            && cache_read_tokens.is_none()
        {
            return None;
        }

        Some(Self {
            input_tokens,
            output_tokens,
            total_tokens,
            cache_creation_tokens,
            cache_read_tokens,
        })
    }
}

#[derive(Debug, Default)]
pub struct SseUsageObserver {
    pending: Vec<u8>,
    usage: Option<ObservedUsage>,
    response_id: Option<String>,
}

impl SseUsageObserver {
    pub fn push(&mut self, chunk: &[u8]) {
        self.pending.extend_from_slice(chunk);
        while let Some((boundary, delimiter_len)) = find_event_boundary(&self.pending) {
            let event = self.pending[..boundary].to_vec();
            self.pending.drain(..boundary + delimiter_len);
            self.observe_event(&event);
        }
        if self.pending.len() > MAX_PENDING_SSE_BYTES {
            self.pending.clear();
        }
    }

    pub fn usage(&self) -> Option<&ObservedUsage> {
        self.usage.as_ref()
    }

    pub fn response_id(&self) -> Option<&str> {
        self.response_id.as_deref()
    }

    fn observe_event(&mut self, event: &[u8]) {
        let event = String::from_utf8_lossy(event);
        let data = event
            .lines()
            .filter_map(|line| line.strip_prefix("data:"))
            .map(str::trim_start)
            .collect::<Vec<_>>()
            .join("\n");
        if data.is_empty() || data == "[DONE]" {
            return;
        }
        if let Ok(value) = serde_json::from_str::<Value>(&data) {
            if let Some(response_id) = value
                .pointer("/response/id")
                .and_then(Value::as_str)
                .and_then(non_empty)
            {
                self.response_id = Some(response_id.to_string());
            }
            if let Some(usage) = ObservedUsage::from_json(&value) {
                self.usage = Some(usage);
            }
        }
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalize_reasoning_effort(value: &str) -> Option<String> {
    let value = value.trim().to_ascii_lowercase();
    (!value.is_empty()
        && value.len() <= 32
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-'))
    .then_some(value)
}

fn integer(value: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(Value::as_i64))
}

fn integer_at(value: &Value, pointer: &str) -> Option<i64> {
    value.pointer(pointer).and_then(Value::as_i64)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn request_observation_reads_nested_and_flat_reasoning_effort() {
        let nested = RequestObservation::from_json(&json!({
            "reasoning": { "effort": "High" }
        }));
        let flat = RequestObservation::from_json(&json!({
            "reasoning_effort": "xhigh"
        }));

        assert_eq!(nested.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(flat.reasoning_effort.as_deref(), Some("xhigh"));
        assert!(nested.uses_reasoning);
        assert!(flat.uses_reasoning);
        assert_eq!(
            RequestObservation::from_json(&json!({})).reasoning_effort,
            None
        );
        assert!(!RequestObservation::from_json(&json!({})).uses_reasoning);
    }

    #[test]
    fn observed_usage_normalizes_responses_usage_and_cache_tokens() {
        let usage = ObservedUsage::from_json(&json!({
            "response": {
                "usage": {
                    "input_tokens": 120,
                    "output_tokens": 30,
                    "total_tokens": 150,
                    "input_tokens_details": { "cached_tokens": 80 },
                    "cache_creation_input_tokens": 12
                }
            }
        }))
        .expect("responses usage");

        assert_eq!(usage.input_tokens, Some(120));
        assert_eq!(usage.output_tokens, Some(30));
        assert_eq!(usage.total_tokens, Some(150));
        assert_eq!(usage.cache_read_tokens, Some(80));
        assert_eq!(usage.cache_creation_tokens, Some(12));
    }

    #[test]
    fn observed_usage_normalizes_chat_completions_usage() {
        let usage = ObservedUsage::from_json(&json!({
            "usage": {
                "prompt_tokens": 90,
                "completion_tokens": 10,
                "prompt_tokens_details": { "cached_tokens": 40 }
            }
        }))
        .expect("chat usage");

        assert_eq!(usage.input_tokens, Some(90));
        assert_eq!(usage.output_tokens, Some(10));
        assert_eq!(usage.total_tokens, Some(100));
        assert_eq!(usage.cache_read_tokens, Some(40));
    }

    #[test]
    fn observed_usage_reads_current_cache_write_tokens() {
        let usage = ObservedUsage::from_json(&json!({
            "usage": {
                "input_tokens": 2006,
                "output_tokens": 300,
                "input_tokens_details": {
                    "cached_tokens": 1920,
                    "cache_write_tokens": 64
                }
            }
        }))
        .expect("usage");

        assert_eq!(usage.cache_read_tokens, Some(1920));
        assert_eq!(usage.cache_creation_tokens, Some(64));
    }

    #[test]
    fn sse_observer_handles_json_split_across_chunks() {
        let mut observer = SseUsageObserver::default();
        observer.push(b"event: response.completed\ndata: {\"type\":\"response.completed\",\"response\":{\"usage\":{\"input_tokens\":7,");
        observer.push(b"\"output_tokens\":3,\"input_tokens_details\":{\"cached_tokens\":2}}}}\n\n");
        observer.push(b"data: [DONE]\n\n");

        let usage = observer.usage().expect("stream usage");
        assert_eq!(usage.input_tokens, Some(7));
        assert_eq!(usage.output_tokens, Some(3));
        assert_eq!(usage.total_tokens, Some(10));
        assert_eq!(usage.cache_read_tokens, Some(2));
    }

    #[test]
    fn sse_observer_captures_response_id_and_usage() {
        let mut observer = SseUsageObserver::default();
        observer.push(
            br#"data: {"type":"response.created","response":{"id":"resp_123"}}

"#,
        );
        observer.push(br#"data: {"type":"response.completed","response":{"id":"resp_123","usage":{"input_tokens":10,"output_tokens":2}}}

"#);

        assert_eq!(observer.response_id(), Some("resp_123"));
        assert_eq!(
            observer.usage().and_then(|usage| usage.input_tokens),
            Some(10)
        );
    }
}
