use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{
    change_events::{ChangeEvent, UpsertChangeEventInput},
    proxy::RequestLog,
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_BATCH_IDS: usize = 500;
const MAX_KIND_BYTES: usize = 128;
const MAX_TITLE_BYTES: usize = 512;
const MAX_MESSAGE_BYTES: usize = 8_192;
const MAX_JSON_BYTES: usize = 65_536;
const MAX_DEDUPE_KEY_BYTES: usize = 512;

pub type ChangeEventDto = ChangeEvent;
pub type RequestLogDto = RequestLog;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeEventIdInputDto {
    pub id: String,
}

impl ChangeEventIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("id", &input.id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChangeEventIdsInputDto {
    pub ids: Vec<String>,
}

impl ChangeEventIdsInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if input.ids.is_empty() || input.ids.len() > MAX_BATCH_IDS {
            return Err(invalid_input(
                "ids",
                "invalid_count",
                "The change event ID count is invalid.",
            ));
        }
        let mut unique = HashSet::with_capacity(input.ids.len());
        for id in &input.ids {
            validate_id("ids", id)?;
            if !unique.insert(id) {
                return Err(invalid_input(
                    "ids",
                    "duplicate_item",
                    "The change event IDs contain a duplicate.",
                ));
            }
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationIdInputDto {
    pub station_id: String,
}

impl StationIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("stationId", &input.station_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeSeverityDto {
    Critical,
    Warning,
    Info,
}

impl ChangeSeverityDto {
    fn into_string(self) -> String {
        serde_json::to_value(self)
            .expect("change severity serializes")
            .as_str()
            .expect("change severity is a string")
            .to_owned()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpsertChangeEventInputDto {
    pub severity: ChangeSeverityDto,
    pub event_type: String,
    pub title: String,
    pub message: String,
    pub object_type: String,
    pub object_id: Option<String>,
    pub station_id: Option<String>,
    pub station_key_id: Option<String>,
    pub pricing_rule_id: Option<String>,
    pub request_log_id: Option<String>,
    pub old_value_json: Option<String>,
    pub new_value_json: Option<String>,
    pub impact_json: Option<String>,
    pub dedupe_key: String,
    pub source: String,
}

impl UpsertChangeEventInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_text("eventType", &input.event_type, MAX_KIND_BYTES, false)?;
        validate_text("title", &input.title, MAX_TITLE_BYTES, false)?;
        validate_text("message", &input.message, MAX_MESSAGE_BYTES, false)?;
        validate_text("objectType", &input.object_type, MAX_KIND_BYTES, false)?;
        validate_optional_id("objectId", input.object_id.as_deref())?;
        validate_optional_id("stationId", input.station_id.as_deref())?;
        validate_optional_id("stationKeyId", input.station_key_id.as_deref())?;
        validate_optional_id("pricingRuleId", input.pricing_rule_id.as_deref())?;
        validate_optional_id("requestLogId", input.request_log_id.as_deref())?;
        validate_optional_json("oldValueJson", input.old_value_json.as_deref())?;
        validate_optional_json("newValueJson", input.new_value_json.as_deref())?;
        validate_optional_json("impactJson", input.impact_json.as_deref())?;
        validate_text("dedupeKey", &input.dedupe_key, MAX_DEDUPE_KEY_BYTES, false)?;
        validate_text("source", &input.source, MAX_KIND_BYTES, false)?;
        Ok(input)
    }

    pub fn into_domain(self) -> UpsertChangeEventInput {
        UpsertChangeEventInput {
            severity: self.severity.into_string(),
            event_type: self.event_type,
            title: self.title,
            message: self.message,
            object_type: self.object_type,
            object_id: self.object_id,
            station_id: self.station_id,
            station_key_id: self.station_key_id,
            pricing_rule_id: self.pricing_rule_id,
            request_log_id: self.request_log_id,
            old_value_json: self.old_value_json,
            new_value_json: self.new_value_json,
            impact_json: self.impact_json,
            dedupe_key: self.dedupe_key,
            source: self.source,
        }
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The changes/logs payload is invalid.",
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

fn validate_optional_id(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    if let Some(value) = value {
        validate_id(field, value)?;
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

fn validate_optional_json(
    field: &'static str,
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.len() > MAX_JSON_BYTES || serde_json::from_str::<Value>(value).is_err() {
        return Err(invalid_input(
            field,
            "invalid_json",
            "The JSON value is invalid.",
        ));
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub const CHANGE_LOGS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ChangeLogsDto",
    typescript: include_str!("change_logs.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let event = fixture_change_event();
    let log = fixture_request_log();
    let upsert = UpsertChangeEventInputDto::parse(serde_json::json!({
        "severity":"warning","eventType":"fixture.changed","title":"Fixture change",
        "message":"Fixture message","objectType":"station","objectId":"station-1",
        "stationId":"station-1","stationKeyId":null,"pricingRuleId":null,
        "requestLogId":null,"oldValueJson":null,"newValueJson":"{}",
        "impactJson":null,"dedupeKey":"fixture-change-1","source":"fixture"
    }))
    .expect("upsert change fixture");
    vec![
        serde_json::json!({"command":"list_request_logs","input":{},"output":[log]}),
        serde_json::json!({"command":"clear_request_logs","input":{},"output":null}),
        serde_json::json!({"command":"list_change_events","input":{},"output":[event.clone()]}),
        serde_json::json!({"command":"clear_change_events","input":{},"output":null}),
        serde_json::json!({"command":"list_change_events_for_station","input":{"stationId":"station-1"},"output":[event.clone()]}),
        serde_json::json!({"command":"upsert_change_event","input":upsert,"output":event.clone()}),
        serde_json::json!({"command":"mark_change_event_read","input":{"id":"change-1"},"output":event.clone()}),
        serde_json::json!({"command":"mark_change_events_read","input":{"ids":["change-1"]},"output":[event.clone()]}),
        serde_json::json!({"command":"dismiss_change_event","input":{"id":"change-1"},"output":event.clone()}),
        serde_json::json!({"command":"resolve_change_event","input":{"id":"change-1"},"output":event}),
    ]
}

#[cfg(test)]
fn fixture_change_event() -> ChangeEvent {
    ChangeEvent {
        id: "change-1".into(),
        severity: "warning".into(),
        event_type: "fixture.changed".into(),
        status: "unread".into(),
        title: "Fixture change".into(),
        message: "Fixture message".into(),
        object_type: "station".into(),
        object_id: Some("station-1".into()),
        station_id: Some("station-1".into()),
        station_name: Some("Fixture station".into()),
        station_key_id: None,
        pricing_rule_id: None,
        request_log_id: None,
        old_value_json: None,
        new_value_json: Some("{}".into()),
        impact_json: None,
        dedupe_key: "fixture-change-1".into(),
        source: "fixture".into(),
        detected_at: "1700000000000".into(),
        resolved_at: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_request_log() -> RequestLog {
    RequestLog {
        id: "request-log-1".into(),
        request_id: Some("request-1".into()),
        started_at: "1700000000000".into(),
        finished_at: Some("1700000000100".into()),
        duration_ms: Some(100),
        method: "POST".into(),
        path: "/v1/chat/completions".into(),
        model: Some("fixture-model".into()),
        stream: false,
        status: "success".into(),
        lifecycle_status: Some("completed".into()),
        station_key_id: Some("key-1".into()),
        station_id: Some("station-1".into()),
        upstream_base_url: None,
        fallback_count: 0,
        error_message: None,
        route_policy: Some("automatic_balanced".into()),
        route_reason: None,
        rejected_candidates_json: Some("[]".into()),
        body_bytes: Some(128),
        attempt_count: Some(1),
        route_wait_ms: Some(1),
        upstream_headers_ms: Some(20),
        failure_source: None,
        attempts_json: Some("[]".into()),
        completion_source: Some("upstream".into()),
        prompt_tokens: Some(10),
        completion_tokens: Some(5),
        total_tokens: Some(15),
        cache_creation_tokens: None,
        cache_read_tokens: None,
        reasoning_effort: None,
        first_token_ms: Some(30),
        billing_mode: Some("token".into()),
        estimated_input_cost: Some(0.001),
        estimated_output_cost: Some(0.002),
        estimated_total_cost: Some(0.003),
        base_input_cost: Some(0.001),
        base_output_cost: Some(0.002),
        base_fixed_cost: None,
        base_total_cost: Some(0.003),
        cost_currency: Some("USD".into()),
        pricing_rule_id: None,
        pricing_source: Some("fixture".into()),
        cost_status: Some("estimated".into()),
        group_binding_id: None,
        normalization_status: Some("normalized".into()),
        balance_scope: None,
        economic_context_json: Some("{}".into()),
        created_at: "1700000000000".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn rejects_unknown_fields_invalid_enums_duplicate_ids_and_invalid_json() {
        let unknown =
            ChangeEventIdInputDto::parse(serde_json::json!({"id":"change-1","extra":true}))
                .expect_err("unknown field");
        assert_eq!(unknown.code, CommandErrorCode::InvalidInput);
        assert!(
            ChangeEventIdsInputDto::parse(serde_json::json!({"ids":["change-1","change-1"]}))
                .is_err()
        );

        let base = serde_json::json!({
            "severity":"warning","eventType":"fixture.changed","title":"Fixture",
            "message":"Fixture","objectType":"station","objectId":null,"stationId":null,
            "stationKeyId":null,"pricingRuleId":null,"requestLogId":null,"oldValueJson":null,
            "newValueJson":"{}","impactJson":null,"dedupeKey":"fixture","source":"fixture"
        });
        let mut invalid_enum = base.clone();
        invalid_enum["severity"] = serde_json::json!("mystery");
        assert!(UpsertChangeEventInputDto::parse(invalid_enum).is_err());
        let mut invalid_json = base;
        invalid_json["newValueJson"] = serde_json::json!("{broken");
        assert!(UpsertChangeEventInputDto::parse(invalid_json).is_err());
    }
}
