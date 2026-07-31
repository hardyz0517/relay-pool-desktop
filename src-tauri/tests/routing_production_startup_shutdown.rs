mod support;

use support::routing_loopback::{LoopbackUpstream, RoutingLoopbackHarness, ScriptedResponse};

#[tokio::test]
async fn command_facade_reconciles_interrupted_lifecycle_before_proxy_admission() {
    let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
        br#"{"object":"list","data":[{"id":"gpt-startup","object":"model"}]}"#.to_vec(),
    )]);
    let harness = RoutingLoopbackHarness::new().await;
    let candidate = harness
        .seed_candidate(&upstream.base_url, "startup-ready", 0)
        .await;
    harness.seed_balance(&candidate.station_id, 100.0).await;
    harness
        .seed_in_progress_request_lifecycle("req-startup-interrupted")
        .await;

    let proxy = harness.start_proxy_with_command_facade().await;
    let reconciled = harness
        .request_lifecycle_status("req-startup-interrupted")
        .await;
    assert_eq!(reconciled.status, "interrupted");
    assert_eq!(reconciled.lifecycle_status.as_deref(), Some("interrupted"));
    assert_eq!(reconciled.terminal_kind.as_deref(), Some("interrupted"));
    assert_eq!(
        reconciled.terminal_code.as_deref(),
        Some("startup_interrupted")
    );
    assert!(reconciled.terminal_at_ms.is_some());
    assert_eq!(
        harness.startup_reconciliation_requests_interrupted().await,
        1
    );

    let response = proxy.get("/v1/models").await;
    assert_eq!(response.status, reqwest::StatusCode::OK);
    upstream.wait_for_requests(1);

    harness.stop_proxy().await;
    let status = harness.proxy_status();
    assert!(!status.running);
    assert_eq!(status.active_requests, 0);
}
