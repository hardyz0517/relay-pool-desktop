use serde::{Deserialize, Serialize};
use serde_json::Value;

#[cfg(test)]
use crate::models::collector::{CollectorEvent, CollectorSnapshot};
use crate::models::collector::{CollectorRunResult, StationLoginTestInput, StationLoginTestResult};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_URL_BYTES: usize = 2_048;
const MAX_LOGIN_USERNAME_BYTES: usize = 512;
const MAX_LOGIN_PASSWORD_BYTES: usize = 8_192;

pub type CollectorRunResultDto = CollectorRunResult;
pub type StationLoginTestResultDto = StationLoginTestResult;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StationCollectorTaskTypeDto {
    Detect,
    Balance,
    Groups,
    Models,
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
            validate_http_url(&self.website_url)?;
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

fn validate_http_url(value: &str) -> Result<(), crate::commands::error::CommandError> {
    if value.len() > MAX_URL_BYTES {
        return Err(invalid_input(
            "websiteUrl",
            "too_long",
            "The website URL is too long.",
        ));
    }
    let parsed = url::Url::parse(value.trim())
        .map_err(|_| invalid_input("websiteUrl", "invalid_url", "The website URL is invalid."))?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host().is_none() {
        return Err(invalid_input(
            "websiteUrl",
            "invalid_scheme",
            "The website URL must use HTTP or HTTPS.",
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

#[cfg_attr(not(test), allow(dead_code))]
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
            serde_json::json!({"stationId":"bad id","taskType":"groups"}),
            serde_json::json!({"stationId":"station-1","taskType":"unknown"}),
            serde_json::json!({"stationId":"station-1","taskType":"groups","unexpected":true}),
        ] {
            let error = match StationCollectorTaskInputDto::parse(value) {
                Ok(_) => panic!("invalid collector task input"),
                Err(error) => error,
            };
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
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
}
