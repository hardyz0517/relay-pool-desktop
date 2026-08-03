mod support;

use support::routing_loopback::{LoopbackUpstream, RoutingLoopbackHarness, ScriptedResponse};

#[tokio::test]
async fn auth_failure_then_backup_success_persists_dual_terminal_outcome() {
    let primary = LoopbackUpstream::script(vec![ScriptedResponse::Status {
        status: 429,
        reason: "Too Many Requests",
    }]);
    let backup = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-loopback","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"ok"}}],"usage":{"prompt_tokens":3,"completion_tokens":2,"total_tokens":5}}"#
            .to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let primary_candidate = harness
        .seed_candidate(&primary.base_url, "primary", 0)
        .await;
    let backup_candidate = harness.seed_candidate(&backup.base_url, "backup", 1).await;
    harness
        .seed_balance(&primary_candidate.station_id, 100.0)
        .await;
    harness
        .seed_balance(&backup_candidate.station_id, 100.0)
        .await;

    let proxy = harness.start_proxy().await;
    let response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "gpt-loopback",
                "messages": [{"role": "user", "content": "hi"}]
            }),
        )
        .await;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    assert!(response.body_text().contains("chatcmpl-loopback"));

    primary.wait_for_requests(1);
    backup.wait_for_requests(1);
    let primary_request = primary.captured_requests().pop().expect("primary request");
    let backup_request = backup.captured_requests().pop().expect("backup request");
    let expected_primary_auth = format!("Bearer {}", primary_candidate.api_key);
    let expected_backup_auth = format!("Bearer {}", backup_candidate.api_key);
    assert_eq!(
        primary_request.header("authorization"),
        Some(expected_primary_auth.as_str())
    );
    assert_eq!(
        backup_request.header("authorization"),
        Some(expected_backup_auth.as_str())
    );
    assert!(
        !String::from_utf8_lossy(&backup_request.body).contains(&primary_candidate.api_key),
        "backup request must not contain the primary key"
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
    assert_eq!(log.lifecycle_status.as_deref(), Some("partial_success"));
    assert_eq!(
        log.station_key_id.as_deref(),
        Some(backup_candidate.station_key_id.as_str())
    );
    assert_eq!(log.fallback_count, 1);
    assert_eq!(log.attempt_count, Some(2));
    assert_eq!(log.completion_source.as_deref(), Some("upstream"));

    let attempts = harness.attempt_terminal_summaries(&log.id).await;
    assert_eq!(attempts.len(), 2);
    assert_eq!(attempts[0].ordinal, 0);
    assert_eq!(attempts[0].station_key_id, primary_candidate.station_key_id);
    assert_eq!(attempts[0].terminal_kind, "failed");
    assert_eq!(attempts[0].failure_kind.as_deref(), Some("RateLimit"));
    assert_eq!(
        attempts[0].public_code.as_deref(),
        Some("upstream_rate_limited")
    );
    assert!(!attempts[0].output_committed);
    assert_eq!(attempts[1].ordinal, 1);
    assert_eq!(attempts[1].station_key_id, backup_candidate.station_key_id);
    assert_eq!(attempts[1].terminal_kind, "succeeded");
    assert!(attempts[1].output_committed);

    assert_eq!(harness.attempt_cost_count(&log.id).await, 2);
    let aggregate = harness
        .cost_aggregate_summary(&log.id)
        .await
        .expect("request cost aggregate");
    assert_eq!(aggregate.status, "incomplete");
    let incomplete_attempts: serde_json::Value =
        serde_json::from_str(&aggregate.incomplete_attempts_json)
            .expect("incomplete attempt gaps json");
    assert_eq!(
        incomplete_attempts,
        serde_json::json!([
            {"request_id": log.id, "ordinal": 0, "status": "missing_usage"},
            {"request_id": log.id, "ordinal": 1, "status": "missing_usage"}
        ])
    );

    harness.stop_proxy().await;
    let status = harness.proxy_status();
    assert!(!status.running);
    assert_eq!(status.active_requests, 0);
}
