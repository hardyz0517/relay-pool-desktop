#[path = "../src/models/monitoring/read_model.rs"]
mod read_model;

use std::fs;

use read_model::{
    ChannelStatusWorkspaceInput, ChannelStatusWorkspaceV2, ChannelStatusWorkspaceWindow,
};

#[test]
fn workspace_input_is_parameterized_bounded_and_rejects_unknown_fields() {
    let input: ChannelStatusWorkspaceInput = serde_json::from_value(serde_json::json!({
        "window": "last30d",
        "timezoneId": "Asia/Shanghai",
        "filter": {
            "search": "openai",
            "enabled": true,
            "outcome": "available",
            "protocolKind": "open_ai_chat",
            "clientProfileId": "codex_cli_compat"
        },
        "sort": {
            "field": "availability",
            "direction": "desc"
        },
        "cursor": { "rowKey": "monitor-1|key-1" },
        "limit": 500
    }))
    .expect("parameterized input");

    assert_eq!(input.window, ChannelStatusWorkspaceWindow::Last30d);
    assert_eq!(input.timezone_id.as_deref(), Some("Asia/Shanghai"));
    assert_eq!(input.limit, Some(500));

    let defaulted: ChannelStatusWorkspaceInput =
        serde_json::from_value(serde_json::json!({})).expect("default input");
    assert_eq!(defaulted.window, ChannelStatusWorkspaceWindow::Last24h);
    assert_eq!(defaulted.limit, Some(200));

    assert!(
        serde_json::from_value::<ChannelStatusWorkspaceInput>(serde_json::json!({
            "limit": 50,
            "rawRequestLogs": true
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<ChannelStatusWorkspaceInput>(serde_json::json!({
            "filter": { "legacyRuns": true }
        }))
        .is_err()
    );
}

#[test]
fn workspace_output_contract_contains_backend_owned_buckets_and_freshness() {
    let workspace = serde_json::to_value(ChannelStatusWorkspaceV2 {
        schema_version: 2,
        generated_at_ms: 1_700_000_000_000,
        window: ChannelStatusWorkspaceWindow::Last24h,
        timezone: read_model::ChannelStatusTimezone {
            id: "UTC".into(),
            source: read_model::ChannelStatusTimezoneSource::Iana,
            requested_id: None,
        },
        bucket_layout: read_model::ChannelStatusBucketLayout {
            recent_limit: 60,
            hourly: vec![read_model::ChannelStatusBucketBoundary {
                kind: read_model::ChannelStatusBucketKind::Hour,
                start_ms: 1,
                end_ms: 2,
                label: "00:00".into(),
            }],
            daily: vec![read_model::ChannelStatusBucketBoundary {
                kind: read_model::ChannelStatusBucketKind::Day,
                start_ms: 0,
                end_ms: 86_400_000,
                label: "01-01".into(),
            }],
        },
        aggregate: read_model::ChannelStatusAggregate::default(),
        freshness: read_model::ChannelStatusFreshness {
            newest_result_at_ms: Some(2),
            oldest_result_at_ms: Some(1),
            has_dirty_rollups: false,
            has_corrupt_rollups: false,
            running_execution_count: 0,
        },
        page: read_model::ChannelStatusPage {
            limit: 50,
            returned: 0,
            next_cursor: None,
        },
        rows: Vec::new(),
    })
    .expect("workspace serialization");

    assert_eq!(workspace["schemaVersion"], 2);
    assert_eq!(workspace["bucketLayout"]["recentLimit"], 60);
    assert_eq!(workspace["bucketLayout"]["hourly"][0]["startMs"], 1);
    assert_eq!(workspace["bucketLayout"]["daily"][0]["endMs"], 86_400_000);
    assert_eq!(workspace["freshness"]["newestResultAtMs"], 2);
    assert!(workspace.get("requestLogs").is_none());
    assert!(workspace.get("stationKeyHealth").is_none());
    assert!(workspace.get("channelStatusSummaries").is_none());
}

#[test]
fn v2_read_model_source_does_not_synthesize_status_from_raw_logs_or_legacy_runs() {
    let query_source =
        fs::read_to_string("src/application/monitoring/queries.rs").expect("query source");
    let migration_facade =
        fs::read_to_string("src/application/queries/channel_status.rs").expect("facade source");

    for forbidden in [
        "RequestLogStore",
        "request_logs",
        "list_recent",
        "station_key_health",
        "recent_status_runs",
        "window_aggregates",
        "channel_monitor_runs",
    ] {
        assert!(
            !query_source.contains(forbidden),
            "V2 query source must not depend on {forbidden}"
        );
    }
    assert!(
        !migration_facade.contains("RequestLogStore") && !migration_facade.contains("request_logs"),
        "compatibility facade must not reintroduce raw request log status synthesis"
    );
}
