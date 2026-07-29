use serde::Serialize;

use crate::models::{routing::StationKeyHealth, shared_capabilities::ChannelStatusWorkspace};

use super::{
    change_logs::RequestLogDto, channel_monitor_reads::ChannelStatusSummaryDto,
    station_keys::KeyPoolItemDto, TypeDescriptor,
};

pub type StationKeyHealthDto = StationKeyHealth;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelStatusWorkspaceDto {
    pub key_pool_items: Vec<KeyPoolItemDto>,
    pub request_logs: Vec<RequestLogDto>,
    pub station_key_health: Vec<StationKeyHealthDto>,
    pub channel_status_summaries: Vec<ChannelStatusSummaryDto>,
}

impl From<ChannelStatusWorkspace> for ChannelStatusWorkspaceDto {
    fn from(value: ChannelStatusWorkspace) -> Self {
        Self {
            key_pool_items: value.key_pool_items,
            request_logs: value.request_logs,
            station_key_health: value.station_key_health,
            channel_status_summaries: value.channel_status_summaries,
        }
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub const CHANNEL_MONITOR_OPERATIONS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "ChannelMonitorOperationsDto",
    typescript: include_str!("channel_monitor_operations.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<serde_json::Value> {
    let workspace = ChannelStatusWorkspaceDto {
        key_pool_items: Vec::new(),
        request_logs: Vec::new(),
        station_key_health: Vec::new(),
        channel_status_summaries:
            Vec::<crate::models::shared_capabilities::ChannelStatusSummary>::new(),
    };
    let run = crate::models::channel_monitors::ChannelMonitorRun {
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
    };
    vec![
        serde_json::json!({"command":"load_channel_status_workspace","input":{},"output":workspace}),
        serde_json::json!({"command":"run_channel_monitor_now","input":{"monitorId":"monitor-1"},"output":[run]}),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn station_key_health_uses_the_generated_camel_case_shape() {
        let value = serde_json::to_value(StationKeyHealthDto {
            station_key_id: "key-1".into(),
            last_success_at: Some("1700000000000".into()),
            last_failure_at: None,
            consecutive_failures: 0,
            success_count: 1,
            failure_count: 0,
            avg_latency_ms: Some(80),
            last_error_summary: None,
            cooldown_until: None,
            updated_at: "1700000000000".into(),
        })
        .expect("station key health fixture");

        assert_eq!(value["stationKeyId"], "key-1");
        assert_eq!(value["avgLatencyMs"], 80);
        assert!(value.get("station_key_id").is_none());
    }
}
