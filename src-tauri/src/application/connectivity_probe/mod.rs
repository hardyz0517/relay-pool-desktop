use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::{
    models::{proxy::UpstreamApiFormat, routing::StationKeyCapabilities},
    services::{proxy::redact_error_message, station_endpoints::build_api_url},
};

pub(crate) const DEFAULT_STATION_KEY_CONNECTIVITY_MODEL: &str = "gpt-4.1-mini";
pub(crate) const STATION_KEY_CONNECTIVITY_CANDIDATE_LIMIT: usize = 2;
/// Keep the probe inexpensive while leaving enough room for compatible
/// providers that emit a short preamble or reasoning before the answer.
pub(crate) const STATION_KEY_CONNECTIVITY_RESPONSES_MAX_OUTPUT_TOKENS: u32 = 128;

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StationKeyConnectivityProbeKind {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StationKeyConnectivityClientProfile {
    StandardApi,
    CodexCliCompat,
}

impl Default for StationKeyConnectivityClientProfile {
    fn default() -> Self {
        Self::StandardApi
    }
}

impl StationKeyConnectivityClientProfile {
    pub(crate) fn for_protocol(self, kind: StationKeyConnectivityProbeKind) -> Self {
        match kind {
            StationKeyConnectivityProbeKind::Responses => self,
            StationKeyConnectivityProbeKind::ChatCompletions => Self::StandardApi,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StationKeyConnectivityRequestMode {
    Stream,
    NonStream,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(crate) enum StationKeyConnectivityResponseMode {
    Stream,
    NonStreamFallback,
}

#[derive(Debug, Clone)]
pub(crate) struct StationKeyConnectivityProbeResult {
    pub(crate) ok: bool,
    pub(crate) status_code: u16,
    pub(crate) duration_ms: i64,
    pub(crate) message: String,
    pub(crate) validated_protocol: StationKeyConnectivityProbeKind,
    pub(crate) client_profile: StationKeyConnectivityClientProfile,
    pub(crate) response_mode: StationKeyConnectivityResponseMode,
    pub(crate) stream_fallback_reason: Option<String>,
}

impl StationKeyConnectivityProbeResult {
    pub(crate) fn success(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: true,
            status_code,
            duration_ms,
            message,
            validated_protocol: StationKeyConnectivityProbeKind::Responses,
            client_profile: StationKeyConnectivityClientProfile::StandardApi,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    pub(crate) fn failure(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: false,
            status_code,
            duration_ms,
            message,
            validated_protocol: StationKeyConnectivityProbeKind::Responses,
            client_profile: StationKeyConnectivityClientProfile::StandardApi,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    pub(crate) fn with_response_mode(
        mut self,
        response_mode: StationKeyConnectivityResponseMode,
    ) -> Self {
        self.response_mode = response_mode;
        self
    }

    pub(crate) fn with_validated_protocol(
        mut self,
        validated_protocol: StationKeyConnectivityProbeKind,
    ) -> Self {
        self.validated_protocol = validated_protocol;
        self
    }

    pub(crate) fn with_client_profile(
        mut self,
        client_profile: StationKeyConnectivityClientProfile,
    ) -> Self {
        self.client_profile = client_profile;
        self
    }

    pub(crate) fn with_stream_fallback_reason(mut self, reason: Option<String>) -> Self {
        self.stream_fallback_reason = reason;
        self
    }
}

pub(crate) fn build_station_key_connectivity_probe_url(
    base_url: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Result<String, String> {
    let path = match kind {
        StationKeyConnectivityProbeKind::Responses => "/v1/responses",
        StationKeyConnectivityProbeKind::ChatCompletions => "/v1/chat/completions",
    };
    build_api_url(base_url, path)
}

pub(crate) fn build_station_key_connectivity_probe_body(
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    mode: StationKeyConnectivityRequestMode,
    client_profile: StationKeyConnectivityClientProfile,
) -> Value {
    match kind {
        StationKeyConnectivityProbeKind::Responses => match client_profile {
            StationKeyConnectivityClientProfile::StandardApi => json!({
                "model": model,
                "input": "hi",
                "store": false,
                "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
                "max_output_tokens": STATION_KEY_CONNECTIVITY_RESPONSES_MAX_OUTPUT_TOKENS,
            }),
            StationKeyConnectivityClientProfile::CodexCliCompat => json!({
                "model": model,
                "instructions": "You are Codex, based on GPT-5. You are running as a coding agent in the Codex CLI on a user's computer.",
                "input": [{
                    "type": "message",
                    "role": "user",
                    "content": [{"type": "input_text", "text": "hi"}]
                }],
                "tools": [],
                "tool_choice": "auto",
                "parallel_tool_calls": false,
                "reasoning": {"effort": "low", "summary": "auto"},
                "store": false,
                "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
            }),
        },
        StationKeyConnectivityProbeKind::ChatCompletions => json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": "hi",
            }],
            "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
            "max_tokens": 32,
        }),
    }
}

pub(crate) fn station_key_connectivity_protocol_label(
    kind: StationKeyConnectivityProbeKind,
) -> String {
    match kind {
        StationKeyConnectivityProbeKind::Responses => "responses".to_string(),
        StationKeyConnectivityProbeKind::ChatCompletions => "chat_completions".to_string(),
    }
}

pub(crate) fn redact_connectivity_error(message: &str) -> String {
    redact_error_message(&truncate_connectivity_reply(message.trim()))
}

pub(crate) fn should_try_station_key_connectivity_chat_fallback(
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
    status_code: u16,
    message: &str,
) -> bool {
    if !matches!(
        upstream_api_format,
        UpstreamApiFormat::Auto | UpstreamApiFormat::CustomOpenAiCompatible
    ) {
        return false;
    }
    if capabilities
        .map(|capabilities| !capabilities.supports_chat_completions)
        .unwrap_or(false)
    {
        return false;
    }
    matches!(status_code, 404 | 405 | 408 | 425 | 429 | 500..=599)
        || (status_code == 400 && response_error_allows_chat_fallback(message))
}

fn response_error_allows_chat_fallback(message: &str) -> bool {
    let normalized = message.to_ascii_lowercase();
    [
        "failed to parse request body",
        "unknown parameter",
        "unrecognized request argument",
        "unsupported parameter",
        "does not support responses",
        "responses api is not supported",
    ]
    .iter()
    .any(|needle| normalized.contains(needle))
}

pub(crate) fn station_key_connectivity_model_candidates(
    capabilities: Option<&StationKeyCapabilities>,
    configured_model: Option<&str>,
    discovered_models: &[String],
) -> Vec<String> {
    let mut candidates = Vec::new();
    let blocked_models = capabilities
        .map(|capabilities| {
            capabilities
                .model_blocklist
                .iter()
                .map(|model| normalize_connectivity_model(model))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    push_station_key_connectivity_model_candidate(
        &mut candidates,
        configured_model,
        &blocked_models,
    );
    if let Some(capabilities) = capabilities {
        let explicit_models = if capabilities.model_allowlist.is_empty() {
            capabilities.preferred_models.as_slice()
        } else {
            capabilities.model_allowlist.as_slice()
        };
        let mut explicit_models = explicit_models.to_vec();
        explicit_models.sort_by_key(|model| connectivity_model_priority(model));
        for model in &explicit_models {
            push_station_key_connectivity_model_candidate(
                &mut candidates,
                Some(model.as_str()),
                &blocked_models,
            );
        }
    }
    let mut discovered_models = discovered_models.iter().enumerate().collect::<Vec<_>>();
    discovered_models.sort_by_key(|(index, model)| (connectivity_model_priority(model), *index));
    for (_, model) in discovered_models {
        push_station_key_connectivity_model_candidate(
            &mut candidates,
            Some(model.as_str()),
            &blocked_models,
        );
    }
    if candidates.is_empty() {
        candidates.push(DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string());
    }
    candidates.truncate(STATION_KEY_CONNECTIVITY_CANDIDATE_LIMIT);
    candidates
}

fn push_station_key_connectivity_model_candidate(
    candidates: &mut Vec<String>,
    model: Option<&str>,
    blocked_models: &[String],
) {
    let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return;
    };
    let normalized = normalize_connectivity_model(model);
    if blocked_models.iter().any(|blocked| blocked == &normalized) {
        return;
    }
    if !candidates
        .iter()
        .any(|candidate| normalize_connectivity_model(candidate) == normalized)
    {
        candidates.push(model.to_string());
    }
}

fn connectivity_model_priority(model: &str) -> i32 {
    let normalized = normalize_connectivity_model(model);
    if normalized.contains("nano") {
        return 0;
    }
    if normalized.contains("mini") {
        return 1;
    }
    if normalized.contains("lite") {
        return 2;
    }
    if normalized.contains("flash") {
        return 3;
    }
    if normalized.contains("haiku") {
        return 4;
    }
    if normalized.contains("turbo") {
        return 5;
    }
    if normalized == "deepseek-chat" || normalized.ends_with("-chat") {
        return 6;
    }
    20
}

fn normalize_connectivity_model(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

#[cfg(test)]
pub(crate) fn run_station_key_connectivity_model_attempts<F>(
    candidates: &[String],
    mut probe: F,
) -> (String, StationKeyConnectivityProbeResult)
where
    F: FnMut(&str) -> StationKeyConnectivityProbeResult,
{
    let fallback_candidates;
    let candidates = if candidates.is_empty() {
        fallback_candidates = vec![DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string()];
        fallback_candidates.as_slice()
    } else {
        candidates
    };
    let mut last = None;
    for model in candidates {
        let result = probe(model);
        if result.ok {
            return (model.clone(), result);
        }
        last = Some((model.clone(), result));
    }
    last.unwrap_or_else(|| {
        (
            DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string(),
            StationKeyConnectivityProbeResult::failure(
                0,
                0,
                "connectivity probe did not run".to_string(),
            ),
        )
    })
}

#[cfg(test)]
pub(crate) fn run_station_key_connectivity_single_model_probe<F>(
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
    mut send_probe: F,
) -> StationKeyConnectivityProbeResult
where
    F: FnMut(StationKeyConnectivityProbeKind) -> StationKeyConnectivityProbeResult,
{
    let response_result = send_probe(StationKeyConnectivityProbeKind::Responses)
        .with_validated_protocol(StationKeyConnectivityProbeKind::Responses);
    if response_result.ok {
        return response_result;
    }
    if !should_try_station_key_connectivity_chat_fallback(
        upstream_api_format,
        capabilities,
        response_result.status_code,
        &response_result.message,
    ) {
        return response_result;
    }

    let chat_result = send_probe(StationKeyConnectivityProbeKind::ChatCompletions)
        .with_validated_protocol(StationKeyConnectivityProbeKind::ChatCompletions);
    let duration_ms = response_result
        .duration_ms
        .saturating_add(chat_result.duration_ms);
    if chat_result.ok {
        let mut chat_result = chat_result;
        chat_result.duration_ms = duration_ms;
        return chat_result;
    }

    StationKeyConnectivityProbeResult::failure(
        chat_result.status_code,
        duration_ms,
        format!(
            "Responses: {}; Chat Completions: {}",
            response_result.message, chat_result.message
        ),
    )
    .with_validated_protocol(StationKeyConnectivityProbeKind::ChatCompletions)
    .with_client_profile(StationKeyConnectivityClientProfile::StandardApi)
}

pub(crate) fn model_ids_from_models_response(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .filter(|model| !model.trim().is_empty())
        .map(|model| model.trim().to_string())
        .collect()
}

pub(crate) fn response_error_message(response_text: &str, status_code: u16) -> String {
    let parsed = serde_json::from_str::<Value>(response_text).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
        })
        .unwrap_or(response_text)
        .trim();
    let fallback = if message.is_empty() {
        format!("Responses returned HTTP {status_code}")
    } else {
        message.to_string()
    };
    redact_error_message(&fallback)
}

pub(crate) fn extract_station_key_connectivity_reply(
    response_text: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(response_text).ok()?;
    let reply = match kind {
        StationKeyConnectivityProbeKind::Responses => extract_responses_reply_text(&parsed),
        StationKeyConnectivityProbeKind::ChatCompletions => extract_chat_reply_text(&parsed),
    }?;
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(redact_error_message(&truncate_connectivity_reply(trimmed)))
    }
}

fn extract_responses_reply_text(value: &Value) -> Option<String> {
    value
        .get("output_text")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("output")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|item| {
                    item.get("content")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find_map(|content| {
                            content
                                .get("text")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                })
        })
}

fn extract_chat_reply_text(value: &Value) -> Option<String> {
    let message = value.pointer("/choices/0/message")?;
    message
        .get("content")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|content| {
                    content
                        .get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
        })
}

fn truncate_connectivity_reply(value: &str) -> String {
    const MAX_REPLY_CHARS: usize = 240;
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(MAX_REPLY_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}
