use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

use futures_util::future::join_all;
use http::StatusCode;

use crate::{
    models::{
        routing::{UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
        stations::CreateStationInput,
    },
    services::{
        database::AppDatabase,
        proxy::{
            runtime::{ProxyRuntimeState, ProxyStartConfig},
            test_support::{LoopbackUpstream, ScriptedResponse},
        },
        secrets::crypto::generate_data_key,
    },
};

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn lifecycle_concurrent_no_candidate_failures_return_to_zero_and_log_once_each() {
    let database = AppDatabase::new_temp_file_for_tests("lifecycle-concurrency-no-candidate")
        .expect("database");
    database
        .update_local_access_key("relay-local-secret".to_string())
        .expect("local key");
    let runtime = ProxyRuntimeState::for_tests();
    let mut config = ProxyStartConfig::new(database.clone(), generate_data_key(), 0);
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
    wait_request_logs_with_status(&database, 32, "failed").await;
    let status = runtime.status(started.port);
    assert_eq!(status.request_count, 32);
    let logs = database.list_local_proxy_request_logs().expect("logs");
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
    let database =
        AppDatabase::new_temp_file_for_tests("lifecycle-stream-capacity").expect("database");
    database
        .update_local_access_key("relay-local-secret".to_string())
        .expect("local key");
    let data_key = generate_data_key();
    seed_candidate(&database, &data_key, upstream.base_url.as_str());

    let runtime = ProxyRuntimeState::for_tests();
    let mut config = ProxyStartConfig::new(database, data_key, 0);
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

async fn wait_request_logs_with_status(database: &AppDatabase, expected: usize, status: &str) {
    for _ in 0..100 {
        let logs = database.list_local_proxy_request_logs().expect("logs");
        if logs.len() == expected && logs.iter().all(|log| log.status == status) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn seed_candidate(database: &AppDatabase, data_key: &[u8; 32], upstream_base_url: &str) {
    database
        .upsert_model_alias(UpsertModelAliasInput {
            id: None,
            client_model: "gpt-test".to_string(),
            upstream_model: "gpt-test".to_string(),
            enabled: true,
            note: None,
        })
        .expect("model alias");
    let station = database
        .create_station_with_data_key(
            CreateStationInput {
                name: "Lifecycle concurrency station".to_string(),
                station_type: "openai-compatible".to_string(),
                website_url: upstream_base_url.to_string(),
                api_base_url: format!("{}/v1", upstream_base_url.trim_end_matches('/')),
                api_key: "sk-lifecycle-concurrency".to_string(),
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            },
            Some(data_key),
        )
        .expect("station");
    let key = database
        .list_station_keys(station.id)
        .expect("station keys")
        .into_iter()
        .next()
        .expect("station key");
    database
        .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
            station_key_id: key.id,
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: true,
            supports_stream: true,
            supports_tools: true,
            supports_vision: true,
            supports_reasoning: true,
            model_allowlist: Vec::new(),
            model_blocklist: Vec::new(),
            preferred_models: Vec::new(),
            only_use_as_backup: false,
            routing_tags: Vec::new(),
        })
        .expect("capabilities");
}
