use std::time::Duration;

use http::StatusCode;

use crate::services::{
    database::AppDatabase,
    proxy::runtime::{ProxyRuntimeState, ProxyStartConfig},
    secrets::crypto::generate_data_key,
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn v2_soak_returns_all_resource_counters_to_zero() {
    let database = AppDatabase::new_temp_file_for_tests("soak").expect("database");
    database
        .update_local_access_key("relay-local-secret".to_string())
        .expect("local key");
    let runtime = ProxyRuntimeState::for_tests();
    let started = runtime
        .start(ProxyStartConfig::new(database, generate_data_key(), 0))
        .await
        .expect("start v2");
    let client = reqwest::Client::new();

    for _ in 0..100 {
        let response = client
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
            .expect("send soak request");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = response.json().await.expect("error json");
        assert_eq!(body["error"]["code"], "route_no_candidate");
    }

    wait_for_quiescence(&runtime, started.port).await;
    let status = runtime.status(started.port);
    assert_eq!(status.active_requests, 0);
    assert_eq!(status.request_count, 100);
    runtime.stop(started.port).await.expect("stop v2");
}

async fn wait_for_quiescence(runtime: &ProxyRuntimeState, port: u16) {
    for _ in 0..50 {
        if runtime.status(port).active_requests == 0 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}
