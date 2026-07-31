mod support;

use support::routing_loopback::{LoopbackUpstream, RoutingLoopbackHarness, ScriptedResponse};

#[tokio::test]
async fn models_catalog_aggregates_partial_loopback_success_and_persists_outcomes() {
    let failing = LoopbackUpstream::script(vec![ScriptedResponse::Status {
        status: 500,
        reason: "Internal Server Error",
    }]);
    let working = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"object":"list","data":[{"id":"gpt-loopback-a","object":"model"},{"id":"gpt-loopback-b","object":"model"}]}"#
            .to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let failing_candidate = harness
        .seed_candidate(&failing.base_url, "models-a", 0)
        .await;
    let working_candidate = harness
        .seed_candidate(&working.base_url, "models-b", 1)
        .await;
    harness
        .seed_balance(&failing_candidate.station_id, 100.0)
        .await;
    harness
        .seed_balance(&working_candidate.station_id, 100.0)
        .await;

    let proxy = harness.start_proxy().await;
    let response = proxy.get("/v1/models").await;
    assert_eq!(
        response.status,
        reqwest::StatusCode::OK,
        "{}",
        response.body_text()
    );
    let body: serde_json::Value =
        serde_json::from_slice(&response.body).expect("models response json");
    let model_ids = body["data"]
        .as_array()
        .expect("models data array")
        .iter()
        .filter_map(|item| item["id"].as_str())
        .collect::<Vec<_>>();
    assert_eq!(model_ids, vec!["gpt-loopback-a", "gpt-loopback-b"]);

    failing.wait_for_requests(1);
    working.wait_for_requests(1);
    assert_eq!(failing.captured_requests()[0].path_and_query, "/v1/models");
    assert_eq!(working.captured_requests()[0].path_and_query, "/v1/models");

    for _ in 0..100 {
        let logs = harness.request_log_summaries().await;
        if logs.first().is_some_and(|log| log.status == "success") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let logs = harness.request_log_summaries().await;
    let log = logs.first().expect("request log");
    assert_eq!(log.status, "success");
    assert_eq!(log.lifecycle_status.as_deref(), Some("partial_success"));
    assert_eq!(log.fallback_count, 1);
    assert_eq!(log.attempt_count, Some(2));
    assert_eq!(
        log.completion_source.as_deref(),
        Some("models_aggregated_success")
    );

    let attempts = harness.attempt_terminal_summaries(&log.id).await;
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].station_key_id, failing_candidate.station_key_id);
    assert_eq!(attempts[0].terminal_kind, "failed");
    assert_eq!(attempts[0].failure_kind.as_deref(), Some("HttpStatus"));
    assert_eq!(attempts[1].station_key_id, working_candidate.station_key_id);
    assert_eq!(attempts[1].terminal_kind, "succeeded");
    assert_eq!(harness.attempt_cost_count(&log.id).await, 2);
    assert_eq!(
        harness
            .cost_aggregate_summary(&log.id)
            .await
            .expect("request cost aggregate")
            .status,
        "incomplete"
    );

    harness.stop_proxy().await;
    let status = harness.proxy_status();
    assert!(!status.running);
    assert_eq!(status.active_requests, 0);
}
