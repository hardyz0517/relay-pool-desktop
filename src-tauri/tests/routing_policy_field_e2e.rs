mod support;

use support::routing_loopback::{
    CandidateCapabilityConfig, LoopbackUpstream, RoutingLoopbackHarness, ScriptedResponse,
};

#[tokio::test]
async fn model_alias_allowlist_and_backup_field_shape_real_route_execution() {
    let backup = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-backup","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"backup"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#
            .to_vec(),
    )]);
    let primary = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-primary","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"primary"}}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#
            .to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    harness.set_routing_strategy("backup_only").await;
    harness
        .upsert_model_alias("client-loopback-model", "upstream-loopback-model")
        .await;
    let backup_candidate = harness
        .seed_candidate(&backup.base_url, "backup-policy", 0)
        .await;
    let primary_candidate = harness
        .seed_candidate(&primary.base_url, "primary-policy", 10)
        .await;
    harness
        .seed_balance(&backup_candidate.station_id, 100.0)
        .await;
    harness
        .seed_balance(&primary_candidate.station_id, 100.0)
        .await;
    harness
        .update_candidate_capabilities(
            &backup_candidate,
            CandidateCapabilityConfig {
                model_allowlist: vec!["upstream-loopback-model".to_string()],
                only_use_as_backup: true,
                ..CandidateCapabilityConfig::default()
            },
        )
        .await;
    harness
        .update_candidate_capabilities(
            &primary_candidate,
            CandidateCapabilityConfig {
                model_allowlist: vec!["upstream-loopback-model".to_string()],
                preferred_models: vec!["upstream-loopback-model".to_string()],
                ..CandidateCapabilityConfig::default()
            },
        )
        .await;

    let proxy = harness.start_proxy().await;
    let response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-loopback-model",
                "messages": [{"role": "user", "content": "hi"}]
            }),
        )
        .await;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    assert!(response.body_text().contains("chatcmpl-primary"));

    primary.wait_for_requests(1);
    assert_eq!(backup.captured_requests().len(), 0);
    let primary_request = primary.captured_requests().pop().expect("primary request");
    let upstream_body: serde_json::Value =
        serde_json::from_slice(&primary_request.body).expect("primary upstream json body");
    assert_eq!(upstream_body["model"], "upstream-loopback-model");
    assert!(
        !String::from_utf8_lossy(&primary_request.body).contains("client-loopback-model"),
        "upstream request should use the canonical mapped model"
    );

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
        Some(primary_candidate.station_key_id.as_str())
    );
    assert_eq!(log.fallback_count, 0);
    assert_eq!(log.attempt_count, Some(1));
    assert_eq!(harness.attempt_cost_count(&log.id).await, 1);

    harness.stop_proxy().await;
    let status = harness.proxy_status();
    assert!(!status.running);
    assert_eq!(status.active_requests, 0);
}
