mod support;

use support::routing_loopback::{LoopbackUpstream, RoutingLoopbackHarness, ScriptedResponse};

#[tokio::test]
async fn model_mapping_rewrites_chat_responses_and_embeddings_upstream_bodies() {
    let upstream = LoopbackUpstream::script(vec![
        ScriptedResponse::Json(
            br#"{"id":"chatcmpl-model-map","object":"chat.completion","choices":[{"index":0,"message":{"role":"assistant","content":"chat ok"},"finish_reason":"stop"}]}"#.to_vec(),
        ),
        ScriptedResponse::Json(
            br#"{"id":"resp-model-map","object":"response","output":[],"status":"completed"}"#.to_vec(),
        ),
        ScriptedResponse::Json(
            br#"{"object":"list","data":[{"object":"embedding","embedding":[0.1],"index":0}],"model":"embedding-native"}"#.to_vec(),
        ),
    ]);
    let harness = RoutingLoopbackHarness::new().await;
    let _candidate = harness
        .seed_candidate(&upstream.base_url, "model-map", 0)
        .await;
    harness
        .set_model_mappings(&[
            ("client-chat-model", "chat-native-model"),
            ("client-responses-model", "responses-native-model"),
            ("client-embedding-model", "embedding-native-model"),
        ])
        .await;

    let proxy = harness.start_proxy().await;
    let chat = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-chat-model",
                "messages": [{"role": "user", "content": "hi"}]
            }),
        )
        .await;
    assert_eq!(chat.status, reqwest::StatusCode::OK, "{}", chat.body_text());

    let responses = proxy
        .post_json(
            "/v1/responses",
            serde_json::json!({
                "model": "client-responses-model",
                "input": "hi"
            }),
        )
        .await;
    assert_eq!(
        responses.status,
        reqwest::StatusCode::OK,
        "{}",
        responses.body_text()
    );

    let embeddings = proxy
        .post_json(
            "/v1/embeddings",
            serde_json::json!({
                "model": "client-embedding-model",
                "input": "hi"
            }),
        )
        .await;
    assert_eq!(
        embeddings.status,
        reqwest::StatusCode::OK,
        "{}",
        embeddings.body_text()
    );

    upstream.wait_for_requests(3);
    let captured = upstream.captured_requests();
    assert_eq!(captured.len(), 3);

    let chat_body: serde_json::Value =
        serde_json::from_slice(&captured[0].body).expect("chat body");
    assert_eq!(captured[0].path_and_query, "/v1/chat/completions");
    assert_eq!(chat_body["model"], "chat-native-model");

    let responses_body: serde_json::Value =
        serde_json::from_slice(&captured[1].body).expect("responses body");
    assert_eq!(captured[1].path_and_query, "/v1/responses");
    assert_eq!(responses_body["model"], "responses-native-model");

    let embeddings_body: serde_json::Value =
        serde_json::from_slice(&captured[2].body).expect("embeddings body");
    assert_eq!(captured[2].path_and_query, "/v1/embeddings");
    assert_eq!(embeddings_body["model"], "embedding-native-model");
    harness.stop_proxy().await;
}

#[tokio::test]
async fn model_mapping_profile_precedence_rewrites_upstream_model_through_proxy() {
    let upstream = LoopbackUpstream::script(vec![
        ScriptedResponse::Json(
            br#"{"id":"profile-key","object":"chat.completion","choices":[]}"#.to_vec(),
        ),
        ScriptedResponse::Json(
            br#"{"id":"profile-station","object":"chat.completion","choices":[]}"#.to_vec(),
        ),
        ScriptedResponse::Json(
            br#"{"id":"profile-default","object":"chat.completion","choices":[]}"#.to_vec(),
        ),
    ]);
    let harness = RoutingLoopbackHarness::new().await;
    let candidate = harness
        .seed_candidate(&upstream.base_url, "profile-precedence", 0)
        .await;

    let proxy = harness.start_proxy().await;

    harness
        .set_profile_model_mapping(
            "client-profile-model",
            "profile-precedence",
            "native-profile-default",
            Some((&candidate.station_key_id, "native-key-binding")),
            Some((&candidate.station_id, "native-station-binding")),
        )
        .await;
    let key_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-profile-model",
                "messages": [{"role": "user", "content": "key"}]
            }),
        )
        .await;
    assert_eq!(key_response.status, reqwest::StatusCode::OK);

    harness
        .set_profile_model_mapping(
            "client-profile-model",
            "profile-precedence",
            "native-profile-default",
            None,
            Some((&candidate.station_id, "native-station-binding")),
        )
        .await;
    let station_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-profile-model",
                "messages": [{"role": "user", "content": "station"}]
            }),
        )
        .await;
    assert_eq!(station_response.status, reqwest::StatusCode::OK);

    harness
        .set_profile_model_mapping(
            "client-profile-model",
            "profile-precedence",
            "native-profile-default",
            None,
            None,
        )
        .await;
    let default_response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-profile-model",
                "messages": [{"role": "user", "content": "default"}]
            }),
        )
        .await;
    assert_eq!(default_response.status, reqwest::StatusCode::OK);

    upstream.wait_for_requests(3);
    let captured = upstream.captured_requests();
    assert_eq!(captured.len(), 3);
    let models = captured
        .iter()
        .map(|request| {
            serde_json::from_slice::<serde_json::Value>(&request.body)
                .expect("upstream request body")["model"]
                .as_str()
                .expect("upstream model")
                .to_string()
        })
        .collect::<Vec<_>>();
    assert_eq!(
        models,
        vec![
            "native-key-binding",
            "native-station-binding",
            "native-profile-default"
        ]
    );
    harness.stop_proxy().await;
}

#[tokio::test]
async fn model_mapping_fallback_rewrites_target_before_output_for_json() {
    let primary = LoopbackUpstream::script(vec![ScriptedResponse::Status {
        status: 429,
        reason: "Too Many Requests",
    }]);
    let fallback = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"id":"fallback-json","object":"chat.completion","choices":[{"message":{"role":"assistant","content":"json ok"}}]}"#.to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let primary_candidate = harness
        .seed_candidate(&primary.base_url, "model-fallback-primary", 0)
        .await;
    let fallback_candidate = harness
        .seed_candidate(&fallback.base_url, "model-fallback-backup", 10_000)
        .await;
    harness
        .update_candidate_capabilities(
            &primary_candidate,
            support_only_model("native-primary-model"),
        )
        .await;
    harness
        .update_candidate_capabilities(
            &fallback_candidate,
            support_only_model("native-fallback-model"),
        )
        .await;
    harness
        .set_model_fallback_mapping(
            "client-fallback-model",
            &["native-primary-model", "native-fallback-model"],
        )
        .await;

    let proxy = harness.start_proxy().await;
    let response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-fallback-model",
                "messages": [{"role": "user", "content": "json"}]
            }),
        )
        .await;
    assert_eq!(
        response.status,
        reqwest::StatusCode::OK,
        "{}",
        response.body_text()
    );
    assert!(response.body_text().contains("json ok"));
    primary.wait_for_requests(1);
    fallback.wait_for_requests(1);
    let primary_body: serde_json::Value =
        serde_json::from_slice(&primary.captured_requests()[0].body).expect("primary body");
    let fallback_body: serde_json::Value =
        serde_json::from_slice(&fallback.captured_requests()[0].body).expect("fallback body");
    assert_eq!(primary_body["model"], "native-primary-model");
    assert_eq!(fallback_body["model"], "native-fallback-model");

    for _ in 0..100 {
        if harness
            .request_log_summaries()
            .await
            .first()
            .is_some_and(|log| log.fallback_count == 1 && log.attempt_count == Some(2))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let logs = harness.request_log_summaries().await;
    let log = logs.first().expect("request log");
    assert_eq!(log.fallback_count, 1);
    assert_eq!(log.attempt_count, Some(2));
    let attempts = harness.attempt_terminal_summaries(&log.id).await;
    assert_eq!(attempts.len(), 2);
    assert!(!attempts[0].output_committed);
    assert!(attempts[1].output_committed);
    harness.stop_proxy().await;
}

#[tokio::test]
async fn model_mapping_fallback_rewrites_target_before_output_for_stream() {
    let primary = LoopbackUpstream::script(vec![ScriptedResponse::Status {
        status: 429,
        reason: "Too Many Requests",
    }]);
    let fallback = LoopbackUpstream::script(vec![ScriptedResponse::Raw {
        status: 200,
        reason: "OK",
        content_type: "text/event-stream",
        body: br#"data: {"id":"fallback-stream","object":"chat.completion.chunk","choices":[{"delta":{"content":"stream ok"},"finish_reason":null}]}

data: {"id":"fallback-stream","object":"chat.completion.chunk","choices":[],"finish_reason":"stop"}

data: [DONE]

"#
            .to_vec(),
    }]);
    let harness = RoutingLoopbackHarness::new().await;
    let primary_candidate = harness
        .seed_candidate(&primary.base_url, "stream-fallback-primary", 0)
        .await;
    let fallback_candidate = harness
        .seed_candidate(&fallback.base_url, "stream-fallback-backup", 10_000)
        .await;
    harness
        .update_candidate_capabilities(
            &primary_candidate,
            support_only_model("native-primary-model"),
        )
        .await;
    harness
        .update_candidate_capabilities(
            &fallback_candidate,
            support_only_model("native-fallback-model"),
        )
        .await;
    harness
        .set_model_fallback_mapping(
            "client-fallback-model",
            &["native-primary-model", "native-fallback-model"],
        )
        .await;

    let proxy = harness.start_proxy().await;
    let response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-fallback-model",
                "messages": [{"role": "user", "content": "stream"}],
                "stream": true
            }),
        )
        .await;
    assert_eq!(
        response.status,
        reqwest::StatusCode::OK,
        "{}",
        response.body_text()
    );
    assert!(response.body_text().contains("stream ok"));
    primary.wait_for_requests(1);
    fallback.wait_for_requests(1);
    let primary_body: serde_json::Value =
        serde_json::from_slice(&primary.captured_requests()[0].body).expect("primary body");
    let fallback_body: serde_json::Value =
        serde_json::from_slice(&fallback.captured_requests()[0].body).expect("fallback body");
    assert_eq!(primary_body["model"], "native-primary-model");
    assert_eq!(fallback_body["model"], "native-fallback-model");

    for _ in 0..100 {
        if harness
            .request_log_summaries()
            .await
            .first()
            .is_some_and(|log| log.fallback_count == 1 && log.attempt_count == Some(2))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let logs = harness.request_log_summaries().await;
    let log = logs.first().expect("request log");
    assert_eq!(log.fallback_count, 1);
    assert_eq!(log.attempt_count, Some(2));
    let attempts = harness.attempt_terminal_summaries(&log.id).await;
    assert_eq!(attempts.len(), 2);
    assert!(!attempts[0].output_committed);
    assert!(attempts[1].output_committed);
    harness.stop_proxy().await;
}

#[tokio::test]
async fn model_mapping_stream_output_commitment_blocks_fallback_after_partial_delta() {
    let primary = LoopbackUpstream::script(vec![ScriptedResponse::Raw {
        status: 200,
        reason: "OK",
        content_type: "text/event-stream",
        body: br#"data: {"id":"partial-stream","object":"chat.completion.chunk","choices":[{"delta":{"content":"partial"},"finish_reason":null}]}

data: {"error":{"message":"stream broke after output","type":"server_error","code":"upstream_error"}}

"#
            .to_vec(),
    }]);
    let fallback = LoopbackUpstream::script(vec![ScriptedResponse::Raw {
        status: 200,
        reason: "OK",
        content_type: "text/event-stream",
        body: br#"data: {"id":"fallback-stream","object":"chat.completion.chunk","choices":[{"delta":{"content":"fallback"},"finish_reason":null}]}

data: {"id":"fallback-stream","object":"chat.completion.chunk","choices":[],"finish_reason":"stop"}

data: [DONE]

"#
            .to_vec(),
    }]);
    let harness = RoutingLoopbackHarness::new().await;
    let primary_candidate = harness
        .seed_candidate(&primary.base_url, "stream-commit-primary", 0)
        .await;
    let fallback_candidate = harness
        .seed_candidate(&fallback.base_url, "stream-commit-backup", 10_000)
        .await;
    harness
        .update_candidate_capabilities(
            &primary_candidate,
            support_only_model("native-primary-model"),
        )
        .await;
    harness
        .update_candidate_capabilities(
            &fallback_candidate,
            support_only_model("native-fallback-model"),
        )
        .await;
    harness
        .set_model_fallback_mapping(
            "client-fallback-model",
            &["native-primary-model", "native-fallback-model"],
        )
        .await;

    let proxy = harness.start_proxy().await;
    let response = proxy
        .post_json(
            "/v1/chat/completions",
            serde_json::json!({
                "model": "client-fallback-model",
                "messages": [{"role": "user", "content": "partial"}],
                "stream": true
            }),
        )
        .await;
    assert!(
        response.body_text().contains("partial"),
        "partial upstream output must reach the client: {}",
        response.body_text()
    );
    primary.wait_for_requests(1);
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert_eq!(
        fallback.captured_requests().len(),
        0,
        "fallback must not be called after output commitment"
    );

    for _ in 0..100 {
        if harness
            .request_log_summaries()
            .await
            .first()
            .is_some_and(|log| log.attempt_count == Some(1))
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    let logs = harness.request_log_summaries().await;
    let log = logs.first().expect("request log");
    assert_eq!(log.fallback_count, 0);
    assert_eq!(log.attempt_count, Some(1));
    let attempts = harness.attempt_terminal_summaries(&log.id).await;
    assert_eq!(attempts.len(), 1);
    assert!(
        attempts[0].output_committed,
        "partial stream output must commit the attempt before EOF failure"
    );
    harness.stop_proxy().await;
}

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
