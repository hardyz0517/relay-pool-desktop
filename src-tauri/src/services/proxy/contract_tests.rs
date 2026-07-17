use super::test_support::{EndpointProbe, LegacyGatewayCase};

#[test]
fn legacy_contract_authenticates_models_chat_responses_embeddings() {
    let case = LegacyGatewayCase::new();
    let local_key = case.local_key().to_string();

    for endpoint in [
        EndpointProbe::Models,
        EndpointProbe::Chat,
        EndpointProbe::Responses,
        EndpointProbe::Embeddings,
    ] {
        assert_eq!(case.request(endpoint, None).status, 401);
        assert_eq!(case.request(endpoint, Some("wrong")).status, 401);
        assert!(case.request(endpoint, Some(&local_key)).status < 400);
    }

    assert_eq!(case.upstream_requests(), 4);
}

#[test]
fn legacy_contract_preserves_query_and_safe_headers() {
    let case = LegacyGatewayCase::new();

    let observed = case.post_chat(
        "?beta=true",
        &[("accept", "text/event-stream"), ("openai-project", "project-1")],
        br#"{"model":"alias-model","stream":true}"#,
    );

    assert_eq!(observed.upstream.path_and_query, "/v1/chat/completions?beta=true");
    assert_eq!(observed.upstream.body, br#"{"model":"mapped-model","stream":true}"#);
    assert_eq!(observed.upstream.header("accept"), Some("text/event-stream"));
    assert_eq!(observed.upstream.header("openai-project"), Some("project-1"));
    assert_eq!(observed.upstream.header("authorization"), Some("Bearer upstream-key"));
    assert_ne!(observed.upstream.header("authorization"), Some(case.local_key()));
}

#[test]
fn legacy_contract_fails_over_retryable_statuses_before_output() {
    for first_status in [429, 500] {
        let observed = LegacyGatewayCase::with_statuses([first_status, 200]).post_buffered_chat();

        assert_eq!(observed.downstream_status, 200);
        assert_eq!(observed.attempted_key_ids, ["key-a", "key-b"]);
        assert_eq!(observed.selected_key_id, "key-b");
        assert_eq!(observed.fallback_count, 1);
        assert_eq!(
            observed.health_updates,
            [("key-a".to_string(), false), ("key-b".to_string(), true)]
        );
        assert_eq!(observed.request_logs_len, 1);
    }
}

#[test]
fn legacy_contract_current_raw_404_behavior_is_explicit() {
    let observed = LegacyGatewayCase::with_statuses([404, 200]).post_buffered_chat();

    assert_eq!(observed.downstream_status, 200);
    assert_eq!(observed.attempted_key_ids, ["key-a", "key-b"]);
    assert_eq!(observed.request_logs_len, 1);
}

#[test]
fn legacy_contract_never_fails_over_after_stream_output() {
    let observed = LegacyGatewayCase::stream_then_disconnect(
        b"data: {\"choices\":[{\"delta\":{\"content\":\"one\"}}]}\n\n",
    )
    .post_streaming_chat();

    assert!(observed.downstream_body.starts_with(b"data:"));
    assert_eq!(observed.attempted_key_ids, ["key-a"]);
    assert_eq!(observed.second_upstream_requests, 0);
    assert_eq!(observed.request_logs_len, 1);
}

#[test]
fn legacy_contract_update_drain_tracks_active_stream() {
    let case = LegacyGatewayCase::paused_stream();
    let stream = case.start_streaming_chat();

    case.wait_active_requests(1);
    let drain_error = case
        .prepare_for_update(std::time::Duration::from_millis(50))
        .expect_err("active stream keeps drain open");
    assert!(drain_error.contains("active request"));

    stream.release_eof();
    case.wait_active_requests(0);
    assert!(case
        .prepare_for_update(std::time::Duration::from_secs(2))
        .is_ok());
    assert_eq!(case.request_logs().len(), 1);
}
