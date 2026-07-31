mod support;

use support::routing_loopback::{LoopbackUpstream, RoutingLoopbackHarness, ScriptedResponse};

#[tokio::test]
async fn default_v2_startup_composition_uses_dual_terminal_outcomes() {
    let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-production","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"ok"}}],"usage":{"prompt_tokens":4,"completion_tokens":3,"total_tokens":7}}"#
            .to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let candidate = harness
        .seed_candidate(&upstream.base_url, "production-default", 0)
        .await;
    harness.seed_balance(&candidate.station_id, 100.0).await;

    let proxy = harness.start_proxy_with_production_startup().await;
    let response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "gpt-production",
                "messages": [{"role": "user", "content": "hi"}]
            }),
        )
        .await;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    upstream.wait_for_requests(1);

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
    assert_eq!(log.lifecycle_status.as_deref(), Some("completed"));
    assert_eq!(
        log.station_key_id.as_deref(),
        Some(candidate.station_key_id.as_str())
    );
    assert_eq!(log.attempt_count, Some(1));
    assert_eq!(harness.attempt_cost_count(&log.id).await, 1);
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
