use serde::Deserialize;
use serde_json::Value;

use crate::application::queries::station_published_status::StationPublishedStatusWorkspace;

use super::{invalid_input, TypeDescriptor};

pub type StationPublishedStatusWorkspaceDto = StationPublishedStatusWorkspace;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StationPublishedStatusWorkspaceInputDto {
    pub station_id: String,
}

impl StationPublishedStatusWorkspaceInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The published status workspace payload is invalid.",
            )
        })?;
        let valid = !input.station_id.is_empty()
            && input.station_id.len() <= 128
            && input.station_id.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':')
            });
        if !valid {
            return Err(invalid_input(
                "stationId",
                "invalid_id",
                "The station identifier is invalid.",
            ));
        }
        Ok(input)
    }
}

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const STATION_PUBLISHED_STATUS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "StationPublishedStatusDto",
    typescript: include_str!("station_published_status.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    vec![serde_json::json!({
        "command": "get_station_published_status_workspace",
        "input": { "stationId": "station-1" },
        "output": {
            "stationId": "station-1",
            "endpointRevision": 1,
            "supported": true,
            "sourceState": "available",
            "completeness": "complete",
            "lastAttemptAtMs": 1700000000000i64,
            "lastSuccessAtMs": 1700000000000i64,
            "lastCompleteAtMs": 1700000000000i64,
            "safeErrorKind": null,
            "monitorCount": 1,
            "stale": false,
            "rows": []
        }
    })]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn workspace_input_rejects_unknown_and_invalid_station_ids() {
        for value in [
            serde_json::json!({"stationId":"bad id"}),
            serde_json::json!({"stationId":"station-1","unexpected":true}),
        ] {
            let error = StationPublishedStatusWorkspaceInputDto::parse(value)
                .expect_err("input is invalid");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }
}
