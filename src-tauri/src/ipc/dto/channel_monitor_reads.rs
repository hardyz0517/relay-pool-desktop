use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::models::{
    channel_monitors::{ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun},
    monitoring::{
        ChannelMonitorAttemptHistoryInput, ChannelMonitorAttemptPage,
        ChannelMonitorExecutionDetail, ChannelMonitorExecutionIdInput,
        ChannelMonitorExecutionListInput, ChannelMonitorExecutionPage, ChannelStatusWorkspaceInput,
        ChannelStatusWorkspaceV2, MonitoringCapabilityCatalog, RunChannelMonitorNowInputV2,
        RunChannelMonitorReceipt,
    },
    shared_capabilities::{ChannelMonitorSummary, ChannelStatusSummary},
};

use super::{invalid_input, TypeDescriptor};

const MAX_ID_BYTES: usize = 128;
const MAX_TIMESTAMP_BYTES: usize = 128;
const MAX_RUN_LIMIT: usize = 500;

pub type ChannelMonitorDto = ChannelMonitor;
pub type ChannelMonitorRequestTemplateDto = ChannelMonitorRequestTemplate;
pub type ChannelMonitorRunDto = ChannelMonitorRun;
pub type ChannelMonitorSummaryDto = ChannelMonitorSummary;
pub type ChannelStatusSummaryDto = ChannelStatusSummary;
pub type ChannelStatusWorkspaceInputDto = ChannelStatusWorkspaceInput;
pub type ChannelStatusWorkspaceV2Dto = ChannelStatusWorkspaceV2;
pub type ChannelMonitorExecutionIdInputDto = ChannelMonitorExecutionIdInput;
pub type ChannelMonitorExecutionListInputDto = ChannelMonitorExecutionListInput;
pub type ChannelMonitorExecutionPageDto = ChannelMonitorExecutionPage;
pub type ChannelMonitorExecutionDetailDto = ChannelMonitorExecutionDetail;
pub type ChannelMonitorAttemptHistoryInputDto = ChannelMonitorAttemptHistoryInput;
pub type ChannelMonitorAttemptPageDto = ChannelMonitorAttemptPage;
pub type MonitoringCapabilityCatalogDto = MonitoringCapabilityCatalog;
pub type RunChannelMonitorNowInputDto = RunChannelMonitorNowInputV2;
pub type RunChannelMonitorReceiptDto = RunChannelMonitorReceipt;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorIdInputDto {
    pub monitor_id: String,
}

impl ChannelMonitorIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("monitorId", &input.monitor_id)?;
        Ok(input)
    }
}

impl ChannelStatusWorkspaceInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_workspace_input(&input)?;
        Ok(input)
    }
}

impl ChannelMonitorExecutionIdInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("executionId", &input.execution_id)?;
        Ok(input)
    }
}

impl ChannelMonitorExecutionListInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_optional_id("monitorId", input.monitor_id.as_deref())?;
        validate_optional_id("stationKeyId", input.station_key_id.as_deref())?;
        validate_optional_status(input.status.as_deref())?;
        if input.limit.is_some_and(|limit| limit == 0 || limit > 200) {
            return Err(invalid_input(
                "limit",
                "invalid_limit",
                "The execution history limit is outside the allowed range.",
            ));
        }
        if let Some(cursor) = &input.cursor {
            validate_id("cursor.executionId", &cursor.execution_id)?;
            if cursor.started_at_ms < 0 {
                return Err(invalid_input(
                    "cursor.startedAtMs",
                    "invalid_cursor",
                    "The execution cursor timestamp is invalid.",
                ));
            }
        }
        Ok(input)
    }
}

impl ChannelMonitorAttemptHistoryInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("executionId", &input.execution_id)?;
        validate_optional_id("stationKeyId", input.station_key_id.as_deref())?;
        if input.limit.is_some_and(|limit| limit == 0 || limit > 200) {
            return Err(invalid_input(
                "limit",
                "invalid_limit",
                "The attempt history limit is outside the allowed range.",
            ));
        }
        if let Some(cursor) = &input.cursor {
            validate_id("cursor.attemptId", &cursor.attempt_id)?;
            if cursor.started_at_ms < 0 {
                return Err(invalid_input(
                    "cursor.startedAtMs",
                    "invalid_cursor",
                    "The attempt cursor timestamp is invalid.",
                ));
            }
        }
        Ok(input)
    }
}

impl RunChannelMonitorNowInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        validate_id("monitorId", &input.monitor_id)?;
        if input.trigger_request_id.as_ref().is_some_and(|value| {
            value.trim().is_empty()
                || value.len() > MAX_ID_BYTES
                || value.chars().any(char::is_control)
        }) {
            return Err(invalid_input(
                "triggerRequestId",
                "invalid_idempotency_key",
                "The trigger request id is invalid.",
            ));
        }
        Ok(input)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ChannelMonitorSummaryInputDto {
    pub run_since: Option<String>,
    pub run_limit: Option<usize>,
}

impl ChannelMonitorSummaryInputDto {
    pub fn parse(value: Value) -> Result<Self, crate::commands::error::CommandError> {
        let input: Self = parse_value(value)?;
        if input.run_since.as_ref().is_some_and(|value| {
            value.trim().is_empty()
                || value.len() > MAX_TIMESTAMP_BYTES
                || value.chars().any(char::is_control)
        }) {
            return Err(invalid_input(
                "runSince",
                "invalid_timestamp",
                "The run cursor timestamp is invalid.",
            ));
        }
        if input
            .run_limit
            .is_some_and(|value| value == 0 || value > MAX_RUN_LIMIT)
        {
            return Err(invalid_input(
                "runLimit",
                "invalid_limit",
                "The run limit is outside the allowed range.",
            ));
        }
        Ok(input)
    }
}

fn parse_value<T: for<'de> Deserialize<'de>>(
    value: Value,
) -> Result<T, crate::commands::error::CommandError> {
    serde_json::from_value(value).map_err(|_| {
        invalid_input(
            "input",
            "invalid_shape",
            "The channel monitor read payload is invalid.",
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

fn validate_optional_status(
    value: Option<&str>,
) -> Result<(), crate::commands::error::CommandError> {
    let Some(value) = value else {
        return Ok(());
    };
    if matches!(
        value,
        "queued" | "running" | "completed" | "partial" | "cancelled" | "skipped" | "interrupted"
    ) {
        Ok(())
    } else {
        Err(invalid_input(
            "status",
            "invalid_status",
            "The execution status is invalid.",
        ))
    }
}

fn validate_workspace_input(
    input: &ChannelStatusWorkspaceInput,
) -> Result<(), crate::commands::error::CommandError> {
    if input.limit.is_some_and(|limit| limit == 0 || limit > 500) {
        return Err(invalid_input(
            "limit",
            "invalid_limit",
            "The channel status workspace limit is outside the allowed range.",
        ));
    }
    if input.timezone_id.as_ref().is_some_and(|value| {
        value.trim().is_empty() || value.len() > 64 || value.chars().any(char::is_control)
    }) {
        return Err(invalid_input(
            "timezoneId",
            "invalid_timezone",
            "The timezone id is invalid.",
        ));
    }
    if input
        .cursor
        .as_ref()
        .is_some_and(|cursor| cursor.row_key.is_empty() || cursor.row_key.len() > 256)
    {
        return Err(invalid_input(
            "cursor",
            "invalid_cursor",
            "The channel status cursor is invalid.",
        ));
    }
    Ok(())
}

#[cfg_attr(not(test), allow(dead_code))]
pub const CHANNEL_MONITOR_READS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ChannelMonitorReadsDto",
    typescript: include_str!("channel_monitor_reads.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    let monitor = fixture_monitor();
    let run = fixture_run();
    let summary = ChannelMonitorSummary {
        monitor: monitor.clone(),
        recent_runs: vec![run.clone()],
        runs_load_status: crate::models::shared_capabilities::ChannelMonitorRunsLoadStatus::Ok,
        latest_run: Some(run.clone()),
    };
    let status = fixture_status_summary(monitor.clone());
    vec![
        serde_json::json!({"command":"list_channel_monitors","input":{},"output":[monitor]}),
        serde_json::json!({"command":"list_channel_monitor_summaries","input":{"runSince":"1700000000000","runLimit":60},"output":[summary]}),
        serde_json::json!({"command":"list_channel_status_summaries","input":{},"output":[status]}),
        serde_json::json!({"command":"load_channel_status_workspace","input":{"window":"last24h","timezoneId":"UTC","limit":50},"output":fixture_workspace_v2()}),
        serde_json::json!({"command":"list_channel_monitor_runs","input":{"monitorId":"monitor-1"},"output":[run]}),
        serde_json::json!({"command":"list_channel_monitor_templates","input":{},"output":[fixture_template()]}),
    ]
}

#[cfg(test)]
fn fixture_workspace_v2() -> ChannelStatusWorkspaceV2 {
    use crate::models::monitoring::{
        ChannelStatusAggregate, ChannelStatusBucketLayout, ChannelStatusFreshness,
        ChannelStatusPage, ChannelStatusTimezone, ChannelStatusTimezoneSource,
        ChannelStatusWorkspaceWindow,
    };
    ChannelStatusWorkspaceV2 {
        schema_version: 2,
        generated_at_ms: 1_700_000_000_000,
        window: ChannelStatusWorkspaceWindow::Last24h,
        timezone: ChannelStatusTimezone {
            id: "UTC".into(),
            source: ChannelStatusTimezoneSource::Iana,
            requested_id: None,
        },
        bucket_layout: ChannelStatusBucketLayout {
            recent_limit: 60,
            hourly: Vec::new(),
            daily: Vec::new(),
        },
        aggregate: ChannelStatusAggregate::default(),
        freshness: ChannelStatusFreshness {
            newest_result_at_ms: None,
            oldest_result_at_ms: None,
            has_dirty_rollups: false,
            has_corrupt_rollups: false,
            running_execution_count: 0,
        },
        page: ChannelStatusPage {
            limit: 50,
            returned: 0,
            next_cursor: None,
        },
        rows: Vec::new(),
    }
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
        interval_seconds: 60,
        jitter_seconds: 5,
        timeout_seconds: 15,
        max_concurrency: 1,
        consecutive_failure_threshold: 3,
        fallback_models: vec!["fixture-model".into()],
        note: None,
        created_at: "1700000000000".into(),
        updated_at: "1700000000000".into(),
    }
}

#[cfg(test)]
fn fixture_run() -> ChannelMonitorRun {
    ChannelMonitorRun {
        id: "monitor-run-1".into(),
        monitor_id: "monitor-1".into(),
        template_id: "template-1".into(),
        station_id: "station-1".into(),
        station_key_id: Some("key-1".into()),
        status: "success".into(),
        started_at: "1700000000000".into(),
        finished_at: Some("1700000000100".into()),
        duration_ms: Some(100),
        http_status: Some(200),
        latency_ms: Some(80),
        response_model: Some("fixture-model".into()),
        fallback_model: None,
        error_message: None,
        created_at: "1700000000000".into(),
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
fn fixture_status_summary(monitor: ChannelMonitor) -> ChannelStatusSummary {
    use crate::models::shared_capabilities::{
        ChannelStatusTimelinePoint, ChannelStatusWindowSummary,
    };
    let window = |name: &str| ChannelStatusWindowSummary {
        window: name.into(),
        total_count: 1,
        success_count: 1,
        failure_count: 0,
        warning_count: 0,
        availability_percent: Some(100.0),
        avg_latency_ms: Some(80),
        avg_endpoint_ping_ms: Some(20),
        last_checked_at: Some("1700000000100".into()),
        latest_status: Some("success".into()),
        latest_error_message: None,
        timeline: vec![ChannelStatusTimelinePoint {
            status: "success".into(),
            latency_ms: Some(80),
            endpoint_ping_ms: Some(20),
            checked_at: "1700000000100".into(),
        }],
    };
    ChannelStatusSummary {
        monitor,
        recent: window("recent"),
        last24h: window("24h"),
        last7d: window("7d"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::error::CommandErrorCode;

    #[test]
    fn rejects_unknown_fields_invalid_ids_and_invalid_summary_bounds() {
        for value in [
            serde_json::json!({"monitorId":"bad id"}),
            serde_json::json!({"monitorId":"monitor-1","unexpected":true}),
        ] {
            assert_eq!(
                ChannelMonitorIdInputDto::parse(value)
                    .expect_err("invalid monitor input")
                    .code,
                CommandErrorCode::InvalidInput
            );
        }
        for value in [
            serde_json::json!({"runSince":"","runLimit":60}),
            serde_json::json!({"runSince":null,"runLimit":0}),
            serde_json::json!({"runSince":null,"runLimit":501}),
            serde_json::json!({"runSince":null,"runLimit":60,"unexpected":true}),
        ] {
            assert_eq!(
                ChannelMonitorSummaryInputDto::parse(value)
                    .expect_err("invalid summary input")
                    .code,
                CommandErrorCode::InvalidInput
            );
        }
        assert!(ChannelStatusWorkspaceInputDto::parse(serde_json::json!({
            "window": "last24h",
            "timezoneId": "Asia/Shanghai",
            "limit": 500
        }))
        .is_ok());
        assert_eq!(
            ChannelStatusWorkspaceInputDto::parse(serde_json::json!({"limit":501}))
                .expect_err("invalid workspace limit")
                .code,
            CommandErrorCode::InvalidInput
        );
        assert!(RunChannelMonitorNowInputDto::parse(serde_json::json!({
            "monitorId":"monitor-1",
            "triggerRequestId":"manual:monitor-1:click-1"
        }))
        .is_ok());
    }
}
