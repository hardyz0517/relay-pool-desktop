#[cfg(test)]
use serde_json::Value;

use crate::models::proxy::RequestLog;

use super::TypeDescriptor;

pub type RequestLogDto = RequestLog;

#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "contract=ipc-dto-type-descriptor; owner=ipc; remove_when=descriptor is registered in production binding export"
    )
)]
pub const REQUEST_LOGS_TYPE: TypeDescriptor = TypeDescriptor {
    name: "RequestLogsDto",
    typescript: include_str!("request_logs.typescript.txt"),
};

#[cfg(test)]
pub(crate) fn serialization_fixtures() -> Vec<Value> {
    vec![
        serde_json::json!({
            "command": "list_request_logs",
            "input": {},
            "output": [fixture_request_log()],
        }),
        serde_json::json!({
            "command": "clear_request_logs",
            "input": {},
            "output": null,
        }),
    ]
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
        http_status: Some(200),
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
        cost_currency: Some("USD".into()),
        pricing_source: Some("fixture".into()),
        cost_status: Some("estimated".into()),
        group_binding_id: None,
        normalization_status: Some("normalized".into()),
        balance_scope: None,
        economic_context_json: Some("{}".into()),
        created_at: "1700000000000".into(),
    }
}
