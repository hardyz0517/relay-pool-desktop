use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::collector::{CollectorEvent, CollectorSnapshot};
use crate::models::{
    capture::{CaptureSessionStatus, CapturedHttpEventInput},
    collector::{CollectorRunResult, StationLoginTestInput, StationLoginTestResult},
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_URL_BYTES: usize = 2_048;
const MAX_LOGIN_USERNAME_BYTES: usize = 512;
const MAX_LOGIN_PASSWORD_BYTES: usize = 8_192;
const MAX_CAPTURE_TEXT_BYTES: usize = 8_192;
const MAX_CAPTURE_RESPONSE_TEXT_BYTES: usize = 256 * 1024;
const MAX_CAPTURE_RESPONSE_JSON_BYTES: usize = 512 * 1024;
const MAX_HTTP_METHOD_BYTES: usize = 16;
const MAX_NON_NEGATIVE_CAPTURE_METRIC: i64 = 128 * 1024 * 1024;

pub type CaptureSessionStatusDto = CaptureSessionStatus;
pub type CollectorRunResultDto = CollectorRunResult;
pub type StationLoginTestResultDto = StationLoginTestResult;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CaptureStationIdInputDto {
    pub station_id: String,
}

impl CaptureStationIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id(&input.station_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationCollectorTaskTypeDto {
    Detect,
    Balance,
    Groups,
    Full,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationCollectorTaskInputDto {
    pub station_id: String,
    pub task_type: StationCollectorTaskTypeDto,
}

impl StationCollectorTaskInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id(&input.station_id)?;
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CapturedHttpEventInputDto {
    pub station_id: String,
    pub source_window_id: String,
    pub page_url: String,
    pub request_url: String,
    pub request_path: Option<String>,
    pub method: String,
    pub status: Option<i64>,
    pub content_type: Option<String>,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<i64>,
    pub response_kind: Option<String>,
    pub response_size: Option<i64>,
    pub response_json: Option<Value>,
    pub response_text: Option<String>,
    pub error_message: Option<String>,
}

impl CapturedHttpEventInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> CapturedHttpEventInput {
        CapturedHttpEventInput {
            station_id: self.station_id,
            source_window_id: self.source_window_id,
            page_url: self.page_url,
            request_url: self.request_url,
            request_path: self.request_path,
            method: self.method,
            status: self.status,
            content_type: self.content_type,
            started_at: self.started_at,
            finished_at: self.finished_at,
            duration_ms: self.duration_ms,
            response_kind: self.response_kind,
            response_size: self.response_size,
            response_json: self.response_json,
            response_text: self.response_text,
            error_message: self.error_message,
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        validate_id(&self.station_id)?;
        validate_id(&self.source_window_id)?;
        validate_http_url(&self.page_url, "pageUrl")?;
        validate_http_url(&self.request_url, "requestUrl")?;
        validate_optional_text("requestPath", self.request_path.as_deref(), MAX_URL_BYTES)?;
        validate_http_method(&self.method)?;
        validate_http_status(self.status)?;
        validate_optional_text(
            "contentType",
            self.content_type.as_deref(),
            MAX_CAPTURE_TEXT_BYTES,
        )?;
        validate_optional_text(
            "startedAt",
            self.started_at.as_deref(),
            MAX_CAPTURE_TEXT_BYTES,
        )?;
        validate_optional_text(
            "finishedAt",
            self.finished_at.as_deref(),
            MAX_CAPTURE_TEXT_BYTES,
        )?;
        validate_non_negative_metric("durationMs", self.duration_ms)?;
        validate_optional_text(
            "responseKind",
            self.response_kind.as_deref(),
            MAX_CAPTURE_TEXT_BYTES,
        )?;
        validate_non_negative_metric("responseSize", self.response_size)?;
        validate_optional_json("responseJson", self.response_json.as_ref())?;
        validate_optional_text(
            "responseText",
            self.response_text.as_deref(),
            MAX_CAPTURE_RESPONSE_TEXT_BYTES,
        )?;
        validate_optional_text(
            "errorMessage",
            self.error_message.as_deref(),
            MAX_CAPTURE_TEXT_BYTES,
        )
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StationLoginProviderDto {
    Sub2api,
    Newapi,
}

impl StationLoginProviderDto {
    fn into_string(self) -> String {
        match self {
            Self::Sub2api => "sub2api",
            Self::Newapi => "newapi",
        }
        .into()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationLoginTestInputDto {
    pub station_type: Option<StationLoginProviderDto>,
    pub website_url: String,
    pub login_username: String,
    pub login_password: String,
}

impl StationLoginTestInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        input.validate()?;
        Ok(input)
    }

    pub fn into_domain(self) -> StationLoginTestInput {
        StationLoginTestInput {
            station_type: self.station_type.map(StationLoginProviderDto::into_string),
            website_url: self.website_url.trim().to_owned(),
            login_username: self.login_username.trim().to_owned(),
            login_password: self.login_password,
        }
    }

    fn validate(&self) -> Result<(), crate::commands::error::CommandError> {
        if !self.website_url.trim().is_empty() {
            validate_http_url(&self.website_url, "websiteUrl")?;
        }
        validate_secret_text(
            "loginUsername",
            &self.login_username,
            MAX_LOGIN_USERNAME_BYTES,
        )?;
        validate_secret_text(
            "loginPassword",
            &self.login_password,
            MAX_LOGIN_PASSWORD_BYTES,
        )
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The station collector operation payload is invalid.",
        )
    })
}

fn validate_id(value: &str) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'));
    if !valid {
        return Err(invalid_input(
            "stationId",
            "invalid_id",
            "The station ID is invalid.",
        ));
    }
    Ok(())
}

fn validate_http_url(
    value: &str,
    field: &'static str,
) -> Result<(), crate::commands::error::CommandError> {
    if value.len() > MAX_URL_BYTES {
        return Err(invalid_input(field, "too_long", "The URL is too long."));
    }
    let parsed = url::Url::parse(value.trim())
        .map_err(|_| invalid_input(field, "invalid_url", "The URL is invalid."))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err(invalid_input(
            field,
            "invalid_scheme",
            "The URL must use HTTP or HTTPS.",
        ));
    }
    Ok(())
}

fn validate_http_method(value: &str) -> Result<(), crate::commands::error::CommandError> {
    let valid = !value.trim().is_empty()
        && value.len() <= MAX_HTTP_METHOD_BYTES
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic() || byte == b'-');
    if !valid {
        return Err(invalid_input(
            "method",
            "invalid_method",
            "The HTTP method is invalid.",
        ));
    }
    Ok(())
}

fn validate_http_status(value: Option<i64>) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|status| !(100..=599).contains(&status)) {
        return Err(invalid_input(
            "status",
            "invalid_status",
            "The HTTP status is invalid.",
        ));
    }
    Ok(())
}

fn validate_non_negative_metric(
    field: &'static str,
    value: Option<i64>,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|metric| !(0..=MAX_NON_NEGATIVE_CAPTURE_METRIC).contains(&metric)) {
        return Err(invalid_input(
            field,
            "invalid_number",
            "The capture metric is invalid.",
        ));
    }
    Ok(())
}

fn validate_optional_text(
    field: &'static str,
    value: Option<&str>,
    max_bytes: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|text| text.len() > max_bytes || text.chars().any(char::is_control)) {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The capture text field is invalid.",
        ));
    }
    Ok(())
}

fn validate_optional_json(
    field: &'static str,
    value: Option<&Value>,
) -> Result<(), crate::commands::error::CommandError> {
    if value.is_some_and(|json| json.to_string().len() > MAX_CAPTURE_RESPONSE_JSON_BYTES) {
        return Err(invalid_input(
            field,
            "too_large",
            "The capture JSON payload is too large.",
        ));
    }
    Ok(())
}

fn validate_secret_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), crate::commands::error::CommandError> {
    if value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(invalid_input(
            field,
            "invalid_text",
            "The credential field is invalid.",
        ));
    }
    Ok(())
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const STATION_COLLECTOR_OPERATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "StationCollectorOperationsDto",
    typescript: include_str!("station_collector_operations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let run = fixture_run();
    vec![
        serde_json::json!({"command":"detect_sub2api_station","input":{"stationId":"station-1"},"output":run.clone()}),
        serde_json::json!({"command":"collect_sub2api_station","input":{"stationId":"station-1"},"output":run.clone()}),
        serde_json::json!({"command":"detect_station_info","input":{"stationId":"station-1"},"output":run.clone()}),
        serde_json::json!({"command":"collect_station_info","input":{"stationId":"station-1"},"output":run.clone()}),
        serde_json::json!({"command":"collect_station_task","input":{"stationId":"station-1","taskType":"groups"},"output":run.clone()}),
        serde_json::json!({"command":"test_station_login","input":{"stationId":"station-1"},"output":run}),
        serde_json::json!({
            "command":"test_station_login_input",
            "input":{"stationType":"newapi","websiteUrl":"https://example.test","loginUsername":"","loginPassword":""},
            "output":{"status":"missing_credentials","message":"Credentials are required.","diagnosis":null,"tokenPresent":false}
        }),
        serde_json::json!({
            "command":"record_capture_event",
            "input":{
                "stationId":"station-1",
                "sourceWindowId":"capture-station-1",
                "pageUrl":"https://example.test/admin",
                "requestUrl":"https://example.test/api/user",
                "requestPath":"/api/user",
                "method":"GET",
                "status":200,
                "contentType":"application/json",
                "startedAt":"2026-07-24T00:00:00Z",
                "finishedAt":"2026-07-24T00:00:01Z",
                "durationMs":25,
                "responseKind":"json",
                "responseSize":42,
                "responseJson":{"ok":true},
                "responseText":null,
                "errorMessage":null
            },
            "output":{
                "stationId":"station-1",
                "status":"capturing",
                "captureCount":1,
                "recognizedFieldCount":0,
                "pendingConfirmationCount":0,
                "webAuthorizationCandidate":false,
                "lastError":null
            }
        }),
    ]
}

#[cfg(test)]
fn fixture_run() -> CollectorRunResult {
    CollectorRunResult {
        snapshot: CollectorSnapshot {
            id: "collector-snapshot-1".into(),
            station_id: "station-1".into(),
            endpoint_revision: 1,
            source: "fixture".into(),
            status: "checked".into(),
            fetched_at: "1700000000000".into(),
            summary_json: serde_json::json!({"mode":"fixture"}),
            normalized_json: serde_json::json!({}),
            raw_json_redacted: None,
            error_message: None,
            created_at: "1700000000000".into(),
        },
        events: vec![CollectorEvent {
            event_type: "fixture".into(),
            message: "Collection completed.".into(),
            status: "checked".into(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn rejects_unknown_task_fields_invalid_ids_and_unknown_task_types() {
        for value in [
            serde_json::json!({"stationId":"station-1"}),
            serde_json::json!({"stationId":"bad id","taskType":"groups"}),
            serde_json::json!({"stationId":"station-1","taskType":"unknown"}),
            serde_json::json!({"stationId":"station-1","taskType":"groups","unexpected":true}),
        ] {
            if value.get("taskType").is_none() {
                CaptureStationIdInputDto::parse(value).expect("valid station id");
            } else {
                let error = match StationCollectorTaskInputDto::parse(value) {
                    Ok(_) => panic!("invalid collector task input"),
                    Err(error) => error,
                };
                assert_eq!(error.code, CommandErrorCode::InvalidInput);
            }
        }
    }

    #[test]
    fn rejects_invalid_login_shapes_without_echoing_credentials() {
        let credential = "sensitive-value";
        for value in [
            serde_json::json!({"stationType":"newapi","websiteUrl":"file:///private","loginUsername":"user","loginPassword":credential}),
            serde_json::json!({"stationType":"newapi","websiteUrl":"https://example.test","loginUsername":"user","loginPassword":format!("{}\n", credential)}),
            serde_json::json!({"stationType":"newapi","websiteUrl":"https://example.test","loginUsername":"user","loginPassword":credential,"unexpected":true}),
        ] {
            let error = match StationLoginTestInputDto::parse(value) {
                Ok(_) => panic!("invalid login input"),
                Err(error) => error,
            };
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
            assert!(!error.message.contains(credential));
        }
    }

    #[test]
    fn capture_event_input_rejects_unknown_invalid_and_oversized_fields() {
        let valid = serde_json::json!({
            "stationId":"station-1",
            "sourceWindowId":"capture-station-1",
            "pageUrl":"https://example.test/admin",
            "requestUrl":"https://example.test/api/user",
            "requestPath":"/api/user",
            "method":"POST",
            "status":200,
            "contentType":"application/json",
            "startedAt":"2026-07-24T00:00:00Z",
            "finishedAt":"2026-07-24T00:00:01Z",
            "durationMs":25,
            "responseKind":"json",
            "responseSize":42,
            "responseJson":{"ok":true},
            "responseText":null,
            "errorMessage":null
        });
        CapturedHttpEventInputDto::parse(valid).expect("valid capture event");

        for value in [
            serde_json::json!({"stationId":"station-1","sourceWindowId":"capture-station-1","pageUrl":"https://example.test","requestUrl":"file:///secret","method":"GET"}),
            serde_json::json!({"stationId":"station-1","sourceWindowId":"capture-station-1","pageUrl":"https://example.test","requestUrl":"https://example.test","method":"GET","status":99}),
            serde_json::json!({"stationId":"station-1","sourceWindowId":"capture-station-1","pageUrl":"https://example.test","requestUrl":"https://example.test","method":"GET","durationMs":-1}),
            serde_json::json!({"stationId":"station-1","sourceWindowId":"capture-station-1","pageUrl":"https://example.test","requestUrl":"https://example.test","method":"GET","responseText":"a".repeat(MAX_CAPTURE_RESPONSE_TEXT_BYTES + 1)}),
            serde_json::json!({"stationId":"station-1","sourceWindowId":"capture-station-1","pageUrl":"https://example.test","requestUrl":"https://example.test","method":"GET","unexpected":true}),
        ] {
            let error = CapturedHttpEventInputDto::parse(value).expect_err("invalid capture event");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }
}
