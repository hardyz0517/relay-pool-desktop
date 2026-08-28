use serde::Deserialize;
use serde_json::Value;

use crate::application::queries::station_published_status::{
    is_valid_overview_cursor_shape, StationPublishedStatusOverview,
    StationPublishedStatusOverviewInput, StationPublishedStatusWorkspace,
};

use super::{invalid_input, TypeDescriptor};

pub type StationPublishedStatusWorkspaceDto = StationPublishedStatusWorkspace;
pub type StationPublishedStatusOverviewDto = StationPublishedStatusOverview;

pub type StationPublishedStatusOverviewInputDto = StationPublishedStatusOverviewInput;

impl StationPublishedStatusOverviewInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = serde_json::from_value(value).map_err(|_| {
            invalid_input(
                "input",
                "invalid_shape",
                "The published status overview payload is invalid.",
            )
        })?;
        if input.limit.is_some_and(|limit| !(1..=200).contains(&limit)) {
            return Err(invalid_input(
                "limit",
                "out_of_range",
                "The overview limit is invalid.",
            ));
        }
        if input
            .filter
            .as_ref()
            .and_then(|f| f.search.as_ref())
            .is_some_and(|s| s.trim().len() > 128)
        {
            return Err(invalid_input(
                "filter.search",
                "too_long",
                "The overview search is too long.",
            ));
        }
        if input.filter.as_ref().is_some_and(|filter| {
            filter.outcome.as_deref().is_some_and(|value| {
                !matches!(value, "available" | "degraded" | "unavailable" | "unknown")
            })
        }) {
            return Err(invalid_input(
                "filter.outcome",
                "invalid_value",
                "The overview outcome is invalid.",
            ));
        }
        if input.filter.as_ref().is_some_and(|filter| {
            filter.source_state.as_deref().is_some_and(|value| {
                !matches!(
                    value,
                    "never_collected"
                        | "available"
                        | "empty"
                        | "unsupported"
                        | "authorization_required"
                        | "degraded"
                        | "failed"
                )
            })
        }) {
            return Err(invalid_input(
                "filter.sourceState",
                "invalid_value",
                "The overview source state is invalid.",
            ));
        }
        if let Some(cursor) = input.cursor.as_deref() {
            if !is_valid_overview_cursor_shape(cursor) {
                return Err(invalid_input(
                    "cursor",
                    "invalid_value",
                    "The overview cursor is invalid.",
                ));
            }
        }
        Ok(input)
    }
}

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
    vec![
        serde_json::json!({
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
        }),
        serde_json::json!({
            "command": "get_station_published_status_overview",
            "input": { "limit": 100 },
            "output": {
                "readAtMs": 1700000000000i64,
                "summary": { "stationTotal": 0, "supportedStationCount": 0, "unsupportedCapabilityStationCount": 0, "neverCollectedStationCount": 0, "monitorTotal": 0, "availableMonitorCount": 0, "degradedMonitorCount": 0, "unavailableMonitorCount": 0, "unknownMonitorCount": 0 },
                "rows": [],
                "page": { "limit": 100, "returned": 0, "nextCursor": null }
            }
        }),
    ]
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

    #[test]
    fn overview_input_rejects_invalid_filters_and_cursor() {
        for value in [
            serde_json::json!({"filter":{"outcome":"broken"}}),
            serde_json::json!({"filter":{"sourceState":"broken"}}),
            serde_json::json!({"cursor":"offset:10"}),
            serde_json::json!({"cursor":"v1:"}),
            serde_json::json!({"limit":0}),
        ] {
            let error = StationPublishedStatusOverviewInputDto::parse(value)
                .expect_err("overview input is invalid");
            assert_eq!(error.code, CommandErrorCode::InvalidInput);
        }
    }

    #[test]
    fn overview_input_accepts_versioned_cursor() {
        let fingerprint = "a".repeat(64);
        let cursor = format!("v1:100:{fingerprint}");
        let input = StationPublishedStatusOverviewInputDto::parse(serde_json::json!({
            "cursor": cursor.clone(),
            "filter": {"outcome": "available", "sourceState": "degraded"}
        }))
        .expect("valid overview input");
        assert_eq!(input.cursor.as_deref(), Some(cursor.as_str()));
    }
}
