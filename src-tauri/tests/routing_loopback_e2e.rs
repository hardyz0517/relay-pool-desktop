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
    // The canonical planner treats key priority as a bounded preference
    // factor, not the legacy absolute selector order. Keep the fallback
    // fixture outside the primary utility band so it deterministically
    // exercises a failed primary followed by a retry.
    let backup_candidate = harness
        .seed_candidate(&backup.base_url, "backup", 10_000)
        .await;
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
            {"request_id": log.id, "ordinal": 1, "status": "pricing_incomplete"}
        ])
    );

    harness.stop_proxy().await;
    let status = harness.proxy_status();
    assert!(!status.running);
    assert_eq!(status.active_requests, 0);
}

#[tokio::test]
async fn model_not_found_writes_exact_capability_verdict_excludes_next_snapshot_and_revision_recovers(
) {
    let rejected = LoopbackUpstream::script(vec![ScriptedResponse::Raw {
        status: 404,
        reason: "Not Found",
        content_type: "application/json",
        body: br#"{"error":{"code":"model_not_found","message":"fixture model is unavailable"}}"#
            .to_vec(),
    }]);
    let unrelated = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-unrelated","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"other model works"}}]}"#
            .to_vec(),
    )]);
    let recovered = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-recovered","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"recovered"}}]}"#
            .to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let rejected_candidate = harness
        .seed_candidate(&rejected.base_url, "model-not-found", 0)
        .await;
    let unrelated_candidate = harness
        .seed_candidate(&unrelated.base_url, "unrelated-model", 10_000)
        .await;
    harness
        .set_candidate_upstream_api_format(&rejected_candidate, "openai_chat_completions")
        .await;
    harness
        .set_candidate_upstream_api_format(&unrelated_candidate, "openai_chat_completions")
        .await;
    harness
        .update_candidate_capabilities(&rejected_candidate, support_only_model("fixture-model"))
        .await;
    harness
        .update_candidate_capabilities(&unrelated_candidate, support_only_model("other-model"))
        .await;
    harness
        .upsert_model_alias("fixture-model", "fixture-model")
        .await;
    harness
        .upsert_model_alias("other-model", "other-model")
        .await;

    let proxy = harness.start_proxy().await;
    let rejected_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "fixture-model",
                "messages": [{"role": "user", "content": "learn capability"}]
            }),
        )
        .await;
    assert_eq!(rejected_response.status, reqwest::StatusCode::NOT_FOUND);
    rejected.wait_for_requests(1);

    for _ in 0..100 {
        if harness
            .unsupported_model_verdict_count(&rejected_candidate)
            .await
            == 1
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        harness
            .unsupported_model_verdict_count(&rejected_candidate)
            .await,
        1,
        "terminal finalization must durably learn the exact unsupported model tuple"
    );

    let excluded_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "fixture-model",
                "messages": [{"role": "user", "content": "must not retry rejected target"}]
            }),
        )
        .await;
    assert_eq!(
        excluded_response.status,
        reqwest::StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        rejected.captured_requests().len(),
        1,
        "the next planning snapshot must exclude the learned key/model commitment"
    );

    let unrelated_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "other-model",
                "messages": [{"role": "user", "content": "unrelated model"}]
            }),
        )
        .await;
    assert_eq!(unrelated_response.status, reqwest::StatusCode::OK);
    unrelated.wait_for_requests(1);

    harness
        .bump_candidate_endpoint_revision(&rejected_candidate)
        .await;
    let recovered_candidate = rejected_candidate.clone();
    // The station's endpoint is switched to a fresh local upstream as part of
    // the same revision-fenced configuration change.
    harness
        .set_candidate_upstream_url(&recovered_candidate, &recovered.base_url)
        .await;
    let recovered_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "fixture-model",
                "messages": [{"role": "user", "content": "revision recovery"}]
            }),
        )
        .await;
    assert_eq!(recovered_response.status, reqwest::StatusCode::OK);
    recovered.wait_for_requests(1);

    harness.stop_proxy().await;
}

#[tokio::test]
async fn group_subscription_failure_blocks_exact_group_and_group_revision_recovers() {
    let rejected = LoopbackUpstream::script(vec![
        ScriptedResponse::Raw {
            status: 403,
            reason: "Forbidden",
            content_type: "application/json",
            body: br#"{"error":{"code":"SUBSCRIPTION_NOT_FOUND","message":"fixture group subscription is unavailable"}}"#
                .to_vec(),
        },
        ScriptedResponse::Json(
            br#"{"id":"chatcmpl-group-recovered","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"group recovered"}}]}"#
                .to_vec(),
        ),
    ]);
    let unrelated = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"chatcmpl-other-group","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"other group works"}}]}"#
            .to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let rejected_candidate = harness
        .seed_candidate(&rejected.base_url, "group-rejected", 0)
        .await;
    let unrelated_candidate = harness
        .seed_candidate(&unrelated.base_url, "group-unrelated", 10_000)
        .await;
    harness
        .set_candidate_station_type(&rejected_candidate, "sub2api")
        .await;
    harness
        .set_candidate_station_type(&unrelated_candidate, "sub2api")
        .await;
    harness
        .set_candidate_upstream_api_format(&rejected_candidate, "openai_chat_completions")
        .await;
    harness
        .set_candidate_upstream_api_format(&unrelated_candidate, "openai_chat_completions")
        .await;
    harness
        .seed_balance(&rejected_candidate.station_id, 100.0)
        .await;
    harness
        .seed_balance(&unrelated_candidate.station_id, 100.0)
        .await;
    harness
        .seed_station_account_concurrency(&rejected_candidate.station_id, 8)
        .await;
    harness
        .seed_station_account_concurrency(&unrelated_candidate.station_id, 8)
        .await;
    let rejected_group = harness
        .bind_candidate_to_group(&rejected_candidate, "group-rejected")
        .await;
    let _unrelated_group = harness
        .bind_candidate_to_group(&unrelated_candidate, "group-unrelated")
        .await;

    let proxy = harness.start_proxy().await;
    let rejected_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "gpt-loopback",
                "messages": [{"role": "user", "content": "learn group subscription"}]
            }),
        )
        .await;
    assert_eq!(
        rejected_response.status,
        reqwest::StatusCode::SERVICE_UNAVAILABLE,
        "{}",
        rejected_response.body_text()
    );
    rejected.wait_for_requests(1);

    for _ in 0..100 {
        if harness
            .blocked_group_subscription_verdict_count(&rejected_group)
            .await
            == 1
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    assert_eq!(
        harness
            .blocked_group_subscription_verdict_count(&rejected_group)
            .await,
        1,
        "terminal finalization must durably learn the exact rejected group binding"
    );

    let unrelated_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "gpt-loopback",
                "messages": [{"role": "user", "content": "unrelated group remains selectable"}]
            }),
        )
        .await;
    assert_eq!(unrelated_response.status, reqwest::StatusCode::OK);
    unrelated.wait_for_requests(1);
    assert_eq!(
        rejected.captured_requests().len(),
        1,
        "the next planning snapshot must exclude only the learned group commitment"
    );

    harness.bump_group_revision(&rejected_group).await;
    let recovered_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "gpt-loopback",
                "messages": [{"role": "user", "content": "group revision recovery"}]
            }),
        )
        .await;
    assert_eq!(recovered_response.status, reqwest::StatusCode::OK);
    rejected.wait_for_requests(2);

    harness.stop_proxy().await;
}

fn support_only_model(model: &str) -> support::routing_loopback::CandidateCapabilityConfig {
    support::routing_loopback::CandidateCapabilityConfig {
        model_allowlist: vec![model.to_string()],
        ..Default::default()
    }
}
