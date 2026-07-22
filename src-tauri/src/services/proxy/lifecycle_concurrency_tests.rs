use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures_util::future::join_all;
use http::StatusCode;

use crate::services::proxy::{
    runtime::ProxyRuntimeState,
    test_support::{LoopbackUpstream, ScriptedResponse, V2ProxyTestFixture},
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_concurrent_no_candidate_failures_return_to_zero_and_log_once_each() {
    let fixture = V2ProxyTestFixture::new().await;
    let runtime = ProxyRuntimeState::for_tests();
    let mut config = fixture.config(0);
    config.limits.max_in_flight_requests = 32;
    let started = runtime.start(config).await.expect("start runtime");

    let client = reqwest::Client::new();
    let requests = (0..32).map(|index| {
        client
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                started.port
            ))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({
                "model": format!("missing-model-{index}"),
                "messages": [{"role": "user", "content": "ping"}]
            }))
            .send()
    });

    let responses = join_all(requests).await;
    for response in responses {
        let response = response.expect("send request");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = response.json().await.expect("error json");
        assert_eq!(body["error"]["code"], "route_no_candidate");
    }

    wait_runtime_active_requests(&runtime, started.port, 0).await;
    wait_request_logs_with_status(&fixture, 32, "failed").await;
    let status = runtime.status(started.port);
    assert_eq!(status.request_count, 32);
    let logs = fixture.request_logs().await;
    assert_eq!(logs.len(), 32);
    let statuses = logs
        .iter()
        .map(|log| log.status.as_str())
        .collect::<Vec<_>>();
    assert!(
        statuses.iter().all(|status| *status == "failed"),
        "unexpected request log statuses: {statuses:?}"
    );

    runtime.stop(started.port).await.expect("stop runtime");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_stream_capacity_is_released_when_client_drops_body() {
    let release = Arc::new(AtomicBool::new(false));
    let upstream = LoopbackUpstream::script(vec![ScriptedResponse::PausedSse {
        first_chunk: b"data: {\"choices\":[{\"delta\":{\"content\":\"hold\"}}]}\n\n".to_vec(),
        release: Arc::clone(&release),
    }]);
    let fixture = V2ProxyTestFixture::new().await;
    fixture.seed_candidate(upstream.base_url.as_str()).await;

    let runtime = ProxyRuntimeState::for_tests();
    let mut config = fixture.config(0);
    config.limits.max_in_flight_requests = 1;
    config.limits.stream_idle_timeout = Duration::from_secs(30);
    let started = runtime.start(config).await.expect("start runtime");
    let client = reqwest::Client::new();

    let response = client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            started.port
        ))
        .bearer_auth("relay-local-secret")
        .header(http::header::ACCEPT, "text/event-stream")
        .json(&serde_json::json!({
            "model": "gpt-test",
            "messages": [{"role": "user", "content": "ping"}],
            "stream": true
        }))
        .send()
        .await
        .expect("send stream");
    assert_eq!(response.status(), StatusCode::OK);
    wait_runtime_active_requests(&runtime, started.port, 1).await;

    let busy = client
        .post(format!(
            "http://127.0.0.1:{}/v1/chat/completions",
            started.port
        ))
        .bearer_auth("relay-local-secret")
        .json(&serde_json::json!({
            "model": "gpt-test",
            "messages": [{"role": "user", "content": "ping"}]
        }))
        .send()
        .await
        .expect("send busy request");
    assert_eq!(busy.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body: serde_json::Value = busy.json().await.expect("busy json");
    assert_eq!(body["error"]["code"], "local_proxy_busy");

    drop(response);
    release.store(true, Ordering::Relaxed);
    wait_runtime_active_requests(&runtime, started.port, 0).await;
    runtime.stop(started.port).await.expect("stop runtime");
}

async fn wait_runtime_active_requests(runtime: &ProxyRuntimeState, port: u16, expected: u32) {
    for _ in 0..100 {
        if runtime.status(port).active_requests == expected {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "active request count stayed at {}, expected {expected}",
        runtime.status(port).active_requests
    );
}

async fn wait_request_logs_with_status(
    fixture: &V2ProxyTestFixture,
    expected: usize,
    status: &str,
) {
    for _ in 0..100 {
        let logs = fixture.request_logs().await;
        if logs.len() == expected && logs.iter().all(|log| log.status == status) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
