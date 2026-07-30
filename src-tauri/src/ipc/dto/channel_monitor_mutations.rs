use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::channel_monitors::{ChannelMonitor, ChannelMonitorRequestTemplate};
use crate::models::channel_monitors::{
    CreateChannelMonitorInput, CreateChannelMonitorTemplateInput, UpdateChannelMonitorInput,
    UpdateChannelMonitorTemplateInput,
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_NAME_BYTES: usize = 256;
const MAX_KIND_BYTES: usize = 128;
const MAX_METHOD_BYTES: usize = 16;
const MAX_PATH_BYTES: usize = 2_048;
const MAX_REQUEST_BODY_BYTES: usize = 65_536;
const MAX_NOTE_BYTES: usize = 4_096;
const MAX_FALLBACK_MODELS: usize = 3;
const MAX_MODEL_BYTES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelMonitorTargetTypeInputDto {
    StationKey,
    Station,
}

impl ChannelMonitorTargetTypeInputDto {
    fn as_str(&self) -> &'static str {
        match self {
            Self::StationKey => "station_key",
            Self::Station => "station",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorProtocolKindInputDto {
    OpenAiChat,
    OpenAiResponses,
    AnthropicMessages,
    GeminiNative,
    XaiGrok,
    GenericOpenAi,
}

impl MonitorProtocolKindInputDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::OpenAiChat => "open_ai_chat",
            Self::OpenAiResponses => "open_ai_responses",
            Self::AnthropicMessages => "anthropic_messages",
            Self::GeminiNative => "gemini_native",
            Self::XaiGrok => "xai_grok",
            Self::GenericOpenAi => "generic_open_ai",
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorClientProfileIdInputDto {
    StandardApi,
    CodexCliCompat,
    ClaudeCodeCompat,
    GeminiCliCompat,
    GrokCliCompat,
}

impl MonitorClientProfileIdInputDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::StandardApi => "standard_api",
            Self::CodexCliCompat => "codex_cli_compat",
            Self::ClaudeCodeCompat => "claude_code_compat",
            Self::GeminiCliCompat => "gemini_cli_compat",
            Self::GrokCliCompat => "grok_cli_compat",
        }
    }

    fn supports(self, protocol: MonitorProtocolKindInputDto) -> bool {
        match self {
            Self::StandardApi => true,
            Self::CodexCliCompat => matches!(
                protocol,
                MonitorProtocolKindInputDto::OpenAiChat
                    | MonitorProtocolKindInputDto::OpenAiResponses
                    | MonitorProtocolKindInputDto::GenericOpenAi
            ),
            Self::ClaudeCodeCompat => {
                matches!(protocol, MonitorProtocolKindInputDto::AnthropicMessages)
            }
            Self::GeminiCliCompat => {
                matches!(protocol, MonitorProtocolKindInputDto::GeminiNative)
            }
            Self::GrokCliCompat => false,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MonitorHealthWritebackModeInputDto {
    Disabled,
    ObserveOnly,
    Authoritative,
}

impl MonitorHealthWritebackModeInputDto {
    fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::ObserveOnly => "observe_only",
            Self::Authoritative => "authoritative",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorMutationIdInputDto {
    pub id: String,
}

impl ChannelMonitorMutationIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateChannelMonitorInputDto {
    pub name: String,
    pub target_type: ChannelMonitorTargetTypeInputDto,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub enabled: bool,
    pub protocol_kind: MonitorProtocolKindInputDto,
    pub client_profile_id: MonitorClientProfileIdInputDto,
    pub client_profile_version: i64,
    pub primary_model: String,
    pub retry_max_attempts_per_model: i64,
    pub retry_initial_backoff_ms: i64,
    pub retry_max_backoff_ms: i64,
    pub risk_daily_probe_budget: i64,
    pub health_writeback_mode: MonitorHealthWritebackModeInputDto,
    pub health_failure_threshold: i64,
    pub health_recovery_threshold: i64,
    pub attempt_timeout_ms: i64,
    pub execution_timeout_ms: i64,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub consecutive_failure_threshold: i64,
    pub fallback_models: Vec<String>,
    pub note: Option<String>,
}

impl CreateChannelMonitorInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> CreateChannelMonitorInput {
        CreateChannelMonitorInput {
            name: self.name.trim().to_owned(),
            target_type: self.target_type.as_str().to_owned(),
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            template_id: self.template_id,
            enabled: self.enabled,
            protocol_kind: self.protocol_kind.as_str().to_owned(),
            client_profile_id: self.client_profile_id.as_str().to_owned(),
            client_profile_version: self.client_profile_version,
            primary_model: self.primary_model.trim().to_owned(),
            retry_max_attempts_per_model: self.retry_max_attempts_per_model,
            retry_initial_backoff_ms: self.retry_initial_backoff_ms,
            retry_max_backoff_ms: self.retry_max_backoff_ms,
            risk_daily_probe_budget: self.risk_daily_probe_budget,
            health_writeback_mode: self.health_writeback_mode.as_str().to_owned(),
            health_failure_threshold: self.health_failure_threshold,
            health_recovery_threshold: self.health_recovery_threshold,
            attempt_timeout_ms: self.attempt_timeout_ms,
            execution_timeout_ms: self.execution_timeout_ms,
            interval_seconds: self.interval_seconds,
            jitter_seconds: self.jitter_seconds,
            timeout_seconds: self.timeout_seconds,
            max_concurrency: self.max_concurrency,
            consecutive_failure_threshold: self.consecutive_failure_threshold,
            fallback_models: normalize_models(self.fallback_models),
            note: normalize_optional(self.note),
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_monitor_fields(
            &self.name,
            &self.target_type,
            &self.station_id,
            self.station_key_id.as_deref(),
            &self.template_id,
            self.protocol_kind,
            self.client_profile_id,
            self.client_profile_version,
            &self.primary_model,
            self.retry_max_attempts_per_model,
            self.retry_initial_backoff_ms,
            self.retry_max_backoff_ms,
            self.risk_daily_probe_budget,
            self.health_writeback_mode,
            self.health_failure_threshold,
            self.health_recovery_threshold,
            self.attempt_timeout_ms,
            self.execution_timeout_ms,
            self.interval_seconds,
            self.jitter_seconds,
            self.timeout_seconds,
            self.max_concurrency,
            self.consecutive_failure_threshold,
            &self.fallback_models,
            self.note.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateChannelMonitorInputDto {
    pub id: String,
    pub name: String,
    pub target_type: ChannelMonitorTargetTypeInputDto,
    pub station_id: String,
    pub station_key_id: Option<String>,
    pub template_id: String,
    pub enabled: bool,
    pub protocol_kind: MonitorProtocolKindInputDto,
    pub client_profile_id: MonitorClientProfileIdInputDto,
    pub client_profile_version: i64,
    pub primary_model: String,
    pub retry_max_attempts_per_model: i64,
    pub retry_initial_backoff_ms: i64,
    pub retry_max_backoff_ms: i64,
    pub risk_daily_probe_budget: i64,
    pub health_writeback_mode: MonitorHealthWritebackModeInputDto,
    pub health_failure_threshold: i64,
    pub health_recovery_threshold: i64,
    pub attempt_timeout_ms: i64,
    pub execution_timeout_ms: i64,
    pub interval_seconds: i64,
    pub jitter_seconds: i64,
    pub timeout_seconds: i64,
    pub max_concurrency: i64,
    pub consecutive_failure_threshold: i64,
    pub fallback_models: Vec<String>,
    pub note: Option<String>,
}

impl UpdateChannelMonitorInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpdateChannelMonitorInput {
        UpdateChannelMonitorInput {
            id: self.id,
            name: self.name.trim().to_owned(),
            target_type: self.target_type.as_str().to_owned(),
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            template_id: self.template_id,
            enabled: self.enabled,
            protocol_kind: self.protocol_kind.as_str().to_owned(),
            client_profile_id: self.client_profile_id.as_str().to_owned(),
            client_profile_version: self.client_profile_version,
            primary_model: self.primary_model.trim().to_owned(),
            retry_max_attempts_per_model: self.retry_max_attempts_per_model,
            retry_initial_backoff_ms: self.retry_initial_backoff_ms,
            retry_max_backoff_ms: self.retry_max_backoff_ms,
            risk_daily_probe_budget: self.risk_daily_probe_budget,
            health_writeback_mode: self.health_writeback_mode.as_str().to_owned(),
            health_failure_threshold: self.health_failure_threshold,
            health_recovery_threshold: self.health_recovery_threshold,
            attempt_timeout_ms: self.attempt_timeout_ms,
            execution_timeout_ms: self.execution_timeout_ms,
            interval_seconds: self.interval_seconds,
            jitter_seconds: self.jitter_seconds,
            timeout_seconds: self.timeout_seconds,
            max_concurrency: self.max_concurrency,
            consecutive_failure_threshold: self.consecutive_failure_threshold,
            fallback_models: normalize_models(self.fallback_models),
            note: normalize_optional(self.note),
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_monitor_fields(
            &self.name,
            &self.target_type,
            &self.station_id,
            self.station_key_id.as_deref(),
            &self.template_id,
            self.protocol_kind,
            self.client_profile_id,
            self.client_profile_version,
            &self.primary_model,
            self.retry_max_attempts_per_model,
            self.retry_initial_backoff_ms,
            self.retry_max_backoff_ms,
            self.risk_daily_probe_budget,
            self.health_writeback_mode,
            self.health_failure_threshold,
            self.health_recovery_threshold,
            self.attempt_timeout_ms,
            self.execution_timeout_ms,
            self.interval_seconds,
            self.jitter_seconds,
            self.timeout_seconds,
            self.max_concurrency,
            self.consecutive_failure_threshold,
            &self.fallback_models,
            self.note.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateChannelMonitorTemplateInputDto {
    pub name: String,
    pub endpoint_kind: String,
    pub method: String,
    pub path: String,
    pub request_body_json: String,
    pub enabled: bool,
    pub note: Option<String>,
}

impl CreateChannelMonitorTemplateInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> CreateChannelMonitorTemplateInput {
        CreateChannelMonitorTemplateInput {
            name: self.name.trim().to_owned(),
            endpoint_kind: self.endpoint_kind.trim().to_owned(),
            method: self.method.trim().to_uppercase(),
            path: self.path.trim().to_owned(),
            request_body_json: self.request_body_json.trim().to_owned(),
            enabled: self.enabled,
            note: normalize_optional(self.note),
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_template_fields(
            &self.name,
            &self.endpoint_kind,
            &self.method,
            &self.path,
            &self.request_body_json,
            self.note.as_deref(),
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateChannelMonitorTemplateInputDto {
    pub id: String,
    pub name: String,
    pub endpoint_kind: String,
    pub method: String,
    pub path: String,
    pub request_body_json: String,
    pub enabled: bool,
    pub note: Option<String>,
}

impl UpdateChannelMonitorTemplateInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpdateChannelMonitorTemplateInput {
        UpdateChannelMonitorTemplateInput {
            id: self.id,
            name: self.name.trim().to_owned(),
            endpoint_kind: self.endpoint_kind.trim().to_owned(),
            method: self.method.trim().to_uppercase(),
            path: self.path.trim().to_owned(),
            request_body_json: self.request_body_json.trim().to_owned(),
            enabled: self.enabled,
            note: normalize_optional(self.note),
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_template_fields(
            &self.name,
            &self.endpoint_kind,
            &self.method,
            &self.path,
            &self.request_body_json,
            self.note.as_deref(),
        )
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_monitor_fields(
    name: &str,
    target_type: &ChannelMonitorTargetTypeInputDto,
    station_id: &str,
    station_key_id: Option<&str>,
    template_id: &str,
    protocol_kind: MonitorProtocolKindInputDto,
    client_profile_id: MonitorClientProfileIdInputDto,
    client_profile_version: i64,
    primary_model: &str,
    retry_max_attempts_per_model: i64,
    retry_initial_backoff_ms: i64,
    retry_max_backoff_ms: i64,
    risk_daily_probe_budget: i64,
    health_writeback_mode: MonitorHealthWritebackModeInputDto,
    health_failure_threshold: i64,
    health_recovery_threshold: i64,
    attempt_timeout_ms: i64,
    execution_timeout_ms: i64,
    interval_seconds: i64,
    jitter_seconds: i64,
    timeout_seconds: i64,
    max_concurrency: i64,
    failure_threshold: i64,
    fallback_models: &[String],
    note: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    validate_text("name", name, MAX_NAME_BYTES, false)?;
    validate_id("stationId", station_id)?;
    validate_id("templateId", template_id)?;
    validate_text("primaryModel", primary_model, MAX_MODEL_BYTES, false)?;
    validate_range(
        "clientProfileVersion",
        client_profile_version,
        1,
        i64::from(u32::MAX),
    )?;
    if !client_profile_id.supports(protocol_kind) {
        return Err(invalid_input(
            "clientProfileId",
            "incompatible_profile",
            "The client profile does not support the selected protocol.",
        ));
    }
    if matches!(
        health_writeback_mode,
        MonitorHealthWritebackModeInputDto::Authoritative
    ) && !matches!(
        client_profile_id,
        MonitorClientProfileIdInputDto::StandardApi
    ) {
        return Err(invalid_input(
            "healthWritebackMode",
            "untrusted_profile",
            "Authoritative health writeback requires the standard API profile.",
        ));
    }
    match (target_type, station_key_id) {
        (ChannelMonitorTargetTypeInputDto::Station, None) => {}
        (ChannelMonitorTargetTypeInputDto::StationKey, Some(id)) => {
            validate_id("stationKeyId", id)?;
        }
        _ => {
            return Err(invalid_input(
                "stationKeyId",
                "target_mismatch",
                "The station key does not match the monitor target type.",
            ));
        }
    }
    validate_range("intervalSeconds", interval_seconds, 15, 3_600)?;
    validate_range("jitterSeconds", jitter_seconds, 0, 600)?;
    if interval_seconds - jitter_seconds < 15 {
        return Err(invalid_input(
            "jitterSeconds",
            "invalid_interval",
            "The jitter leaves too little time between monitor runs.",
        ));
    }
    validate_range("timeoutSeconds", timeout_seconds, 5, 120)?;
    validate_range("maxConcurrency", max_concurrency, 1, 16)?;
    validate_range("consecutiveFailureThreshold", failure_threshold, 1, 20)?;
    validate_range(
        "retryMaxAttemptsPerModel",
        retry_max_attempts_per_model,
        1,
        3,
    )?;
    validate_range("retryInitialBackoffMs", retry_initial_backoff_ms, 0, 60_000)?;
    validate_range("retryMaxBackoffMs", retry_max_backoff_ms, 0, 60_000)?;
    if retry_max_backoff_ms < retry_initial_backoff_ms {
        return Err(invalid_input(
            "retryMaxBackoffMs",
            "invalid_backoff",
            "The maximum retry backoff is smaller than the initial backoff.",
        ));
    }
    validate_range("riskDailyProbeBudget", risk_daily_probe_budget, 1, 10_000)?;
    validate_range("healthFailureThreshold", health_failure_threshold, 1, 20)?;
    validate_range("healthRecoveryThreshold", health_recovery_threshold, 1, 20)?;
    validate_range("attemptTimeoutMs", attempt_timeout_ms, 1_000, 120_000)?;
    validate_range("executionTimeoutMs", execution_timeout_ms, 1_000, 300_000)?;
    if attempt_timeout_ms >= execution_timeout_ms {
        return Err(invalid_input(
            "executionTimeoutMs",
            "invalid_timeout",
            "The execution timeout must be greater than the attempt timeout.",
        ));
    }
    validate_models(fallback_models)?;
    if fallback_models
        .iter()
        .any(|model| model.trim() == primary_model.trim())
    {
        return Err(invalid_input(
            "fallbackModels",
            "duplicates_primary",
            "A fallback model duplicates the primary model.",
        ));
    }
    validate_optional_note(note)?;
    Ok(())
}

fn validate_template_fields(
    name: &str,
    endpoint_kind: &str,
    method: &str,
    path: &str,
    request_body_json: &str,
    note: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    validate_text("name", name, MAX_NAME_BYTES, false)?;
    validate_text("endpointKind", endpoint_kind, MAX_KIND_BYTES, false)?;
    let method = method.trim();
    if method.is_empty()
        || method.len() > MAX_METHOD_BYTES
        || !method.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return Err(invalid_input(
            "method",
            "invalid_method",
            "The HTTP method is invalid.",
        ));
    }
    if !path.trim().starts_with('/')
        || path.len() > MAX_PATH_BYTES
        || path.chars().any(char::is_control)
    {
        return Err(invalid_input(
            "path",
            "invalid_path",
            "The request path is invalid.",
        ));
    }
    if request_body_json.len() > MAX_REQUEST_BODY_BYTES
        || serde_json::from_str::<Value>(request_body_json)
            .ok()
            .is_none_or(|value| !value.is_object())
    {
        return Err(invalid_input(
            "requestBodyJson",
            "invalid_json",
            "The request body must be a bounded JSON object.",
        ));
    }
    validate_optional_note(note)?;
    Ok(())
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The channel monitor mutation payload is invalid.",
        )
    })
}

fn validate_id(
    field: &'static str,
    value: &str,
) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            field,
            "invalid_id",
            "The identifier is invalid.",
        ));
    }
    Ok(())
}

fn validate_text(
    field: &'static str,
    value: &str,
    max: usize,
    allow_empty: bool,
) -> Result<(), crate::commands::error::CommandError> {
    if (!allow_empty && value.trim().is_empty())
        || value.len() > max
        || value.chars().any(char::is_control)
    {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The text value is invalid.",
        ));
    }
    Ok(())
}

fn validate_range(
    field: &'static str,
    value: i64,
    min: i64,
    max: i64,
) -> Result<(), crate::commands::error::CommandError> {
    if !(min..=max).contains(&value) {
        return Err(invalid_input(
            field,
            "out_of_range",
            "The numeric value is outside the allowed range.",
        ));
    }
    Ok(())
}

fn validate_models(models: &[String]) -> Result<(), crate::commands::error::CommandError> {
    if models.len() > MAX_FALLBACK_MODELS {
        return Err(invalid_input(
            "fallbackModels",
            "too_many_items",
            "The fallback model list contains too many items.",
        ));
    }
    let mut unique = HashSet::with_capacity(models.len());
    for model in models {
        validate_text("fallbackModels", model, MAX_MODEL_BYTES, false)?;
        if !unique.insert(model.trim()) {
            return Err(invalid_input(
                "fallbackModels",
                "duplicate_item",
                "The fallback model list contains a duplicate.",
            ));
        }
    }
    Ok(())
}

fn validate_optional_note(note: Option<&str>) -> Result<(), crate::commands::error::CommandError> {
    if note.is_some_and(|value| {
        value.len() > MAX_NOTE_BYTES
            || value
                .chars()
                .any(|character| character.is_control() && !matches!(character, '\r' | '\n' | '\t'))
    }) {
        return Err(invalid_input(
            "note",
            "invalid_text",
            "The note is invalid.",
        ));
    }
    Ok(())
}

fn normalize_models(models: Vec<String>) -> Vec<String> {
    models
        .into_iter()
        .map(|model| model.trim().to_owned())
        .collect()
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_owned();
        (!value.is_empty()).then_some(value)
    })
}

#[cfg_attr(not(test), allow(dead_code))]
pub const CHANNEL_MONITOR_MUTATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ChannelMonitorMutationsDto",
    typescript: include_str!("channel_monitor_mutations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let monitor_input = fixture_monitor_input();
    let mut monitor_update = monitor_input.clone();
    monitor_update["id"] = serde_json::json!("monitor-1");
    let template_input = fixture_template_input();
    vec![
        serde_json::json!({"command":"create_channel_monitor","input":monitor_input,"output":fixture_monitor()}),
        serde_json::json!({"command":"update_channel_monitor","input":monitor_update,"output":fixture_monitor()}),
        serde_json::json!({"command":"delete_channel_monitor","input":{"id":"monitor-1"},"output":null}),
        serde_json::json!({"command":"create_channel_monitor_template","input":template_input,"output":fixture_template()}),
        serde_json::json!({"command":"update_channel_monitor_template","input":{"id":"template-1","name":"Fixture template","endpointKind":"chat_completions","method":"POST","path":"/v1/chat/completions","requestBodyJson":"{}","enabled":true,"note":null},"output":fixture_template()}),
        serde_json::json!({"command":"duplicate_channel_monitor_template","input":{"id":"template-1"},"output":fixture_template()}),
        serde_json::json!({"command":"delete_channel_monitor_template","input":{"id":"template-1"},"output":null}),
    ]
}

#[cfg(test)]
fn fixture_monitor_input() -> Value {
    serde_json::json!({
        "name":"Fixture monitor","targetType":"station_key","stationId":"station-1",
        "stationKeyId":"key-1","templateId":"template-1","enabled":true,
        "protocolKind":"open_ai_chat","clientProfileId":"standard_api","clientProfileVersion":1,
        "primaryModel":"fixture-model","retryMaxAttemptsPerModel":1,
        "retryInitialBackoffMs":200,"retryMaxBackoffMs":2000,"riskDailyProbeBudget":200,
        "healthWritebackMode":"observe_only","healthFailureThreshold":2,"healthRecoveryThreshold":2,
        "attemptTimeoutMs":10000,"executionTimeoutMs":30000,"intervalSeconds":60,"jitterSeconds":5,
        "timeoutSeconds":30,"maxConcurrency":1,"consecutiveFailureThreshold":2,
        "fallbackModels":["fixture-fallback"],"note":null
    })
}

#[cfg(test)]
fn fixture_template_input() -> Value {
    serde_json::json!({"name":"Fixture template","endpointKind":"chat_completions","method":"POST","path":"/v1/chat/completions","requestBodyJson":"{}","enabled":true,"note":null})
}

#[cfg(test)]
fn fixture_monitor() -> ChannelMonitor {
    ChannelMonitor {
        id: "monitor-1".into(),
        name: "Fixture monitor".into(),
        target_type: "station_key".into(),
        station_id: "station-1".into(),
        station_key_id: Some("key-1".into()),
        template_id: "template-1".into(),
        enabled: true,
        protocol_kind: "open_ai_chat".into(),
        client_profile_id: "standard_api".into(),
        client_profile_version: 1,
        primary_model: "fixture-model".into(),
        retry_max_attempts_per_model: 1,
        retry_initial_backoff_ms: 200,
        retry_max_backoff_ms: 2_000,
        risk_daily_probe_budget: 200,
        health_writeback_mode: "observe_only".into(),
        health_failure_threshold: 2,
        health_recovery_threshold: 2,
        attempt_timeout_ms: 10_000,
        execution_timeout_ms: 30_000,
        schedule_revision: 1,
        interval_seconds: 60,
        jitter_seconds: 5,
        timeout_seconds: 15,
        max_concurrency: 1,
        consecutive_failure_threshold: 2,
        fallback_models: vec!["fixture-fallback".into()],
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_template() -> ChannelMonitorRequestTemplate {
    ChannelMonitorRequestTemplate {
        id: "template-1".into(),
        name: "Fixture template".into(),
        endpoint_kind: "chat_completions".into(),
        method: "POST".into(),
        path: "/v1/chat/completions".into(),
        request_body_json: "{}".into(),
        enabled: true,
        built_in: false,
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn monitor_inputs_reject_unknown_fields_target_mismatches_and_invalid_ranges() {
        let base = fixture_monitor_input();
        let mut target_mismatch = base.clone();
        target_mismatch["targetType"] = serde_json::json!("station");
        let mut missing_key = base.clone();
        missing_key["stationKeyId"] = Value::Null;
        let mut invalid_jitter = base.clone();
        invalid_jitter["intervalSeconds"] = serde_json::json!(15);
        invalid_jitter["jitterSeconds"] = serde_json::json!(1);
        let mut duplicate_model = base.clone();
        duplicate_model["fallbackModels"] = serde_json::json!(["fixture-model"]);
        let mut incompatible_profile = base.clone();
        incompatible_profile["clientProfileId"] = serde_json::json!("claude_code_compat");
        let mut untrusted_writeback = base.clone();
        untrusted_writeback["clientProfileId"] = serde_json::json!("codex_cli_compat");
        untrusted_writeback["healthWritebackMode"] = serde_json::json!("authoritative");
        let mut invalid_timeout = base.clone();
        invalid_timeout["executionTimeoutMs"] = serde_json::json!(10_000);
        let mut unknown = base;
        unknown["unexpected"] = serde_json::json!(true);

        for value in [
            target_mismatch,
            missing_key,
            invalid_jitter,
            duplicate_model,
            incompatible_profile,
            untrusted_writeback,
            invalid_timeout,
            unknown,
        ] {
            assert_eq!(
                CreateChannelMonitorInputDto::parse(value)
                    .expect_err("invalid monitor input")
                    .code,
                CommandErrorCode::InvalidInput
            );
        }
    }

    #[test]
    fn template_and_id_inputs_reject_invalid_shapes_without_echoing_payloads() {
        let normalized = CreateChannelMonitorTemplateInputDto::parse(serde_json::json!({
            "name":" Fixture ","endpointKind":" chat_completions ","method":" POST ",
            "path":" /v1/chat/completions ","requestBodyJson":" {} ","enabled":true,"note":null
        }))
        .expect("trim-compatible template input")
        .into_domain();
        assert_eq!(normalized.method, "POST");
        assert_eq!(normalized.path, "/v1/chat/completions");

        for value in [
            serde_json::json!({"name":"Fixture","endpointKind":"chat_completions","method":"POST  BAD","path":"/v1/chat/completions","requestBodyJson":"{}","enabled":true,"note":null}),
            serde_json::json!({"name":"Fixture","endpointKind":"chat_completions","method":"POST","path":"https://example.invalid/v1","requestBodyJson":"{}","enabled":true,"note":null}),
            serde_json::json!({"name":"Fixture","endpointKind":"chat_completions","method":"POST","path":"/v1/chat/completions","requestBodyJson":"[]","enabled":true,"note":null}),
        ] {
            assert_eq!(
                CreateChannelMonitorTemplateInputDto::parse(value)
                    .expect_err("invalid template input")
                    .code,
                CommandErrorCode::InvalidInput
            );
        }
        for value in [
            serde_json::json!({"id":"bad id"}),
            serde_json::json!({"id":"monitor-1","unexpected":true}),
        ] {
            assert_eq!(
                ChannelMonitorMutationIdInputDto::parse(value)
                    .expect_err("invalid id input")
                    .code,
                CommandErrorCode::InvalidInput
            );
        }
    }
}
