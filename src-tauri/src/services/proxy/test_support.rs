use std::{
    collections::HashMap,
    io::Write,
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    application::{app_services::AppServices, pagination::PageLimit},
    models::{
        pricing::UpsertBalanceSnapshotInput,
        proxy::RequestLog,
        routing::{StationKeyHealth, UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
        station_keys::CreateStationKeyInput,
        stations::CreateStationInput,
    },
    persistence::runtime::PersistenceRuntime,
    services::{
        proxy::{http_request::parse_http_request, runtime::ProxyStartConfig},
        secrets::crypto::generate_data_key,
    },
};

pub(crate) struct V2ProxyTestFixture {
    pub(crate) services: AppServices,
    runtime: PersistenceRuntime,
    pub(crate) data_key: [u8; 32],
    _root: tempfile::TempDir,
}

impl V2ProxyTestFixture {
    pub(crate) async fn new() -> Self {
        let root = tempfile::tempdir().expect("tempdir");
        let default_data_dir = root.path().join("default");
        let active_data_dir = root.path().join("active");
        std::fs::create_dir_all(&active_data_dir).expect("active data dir");
        let database_path = active_data_dir.join("relay-pool-desktop-v2.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&database_path)
            .await
            .expect("initialize V2 persistence runtime");
        let data_key = generate_data_key();
        let services = crate::app_composition::compose_app_services(
            runtime.handle(),
            data_key,
            active_data_dir.to_string_lossy().into_owned(),
            None,
            Arc::new(
                crate::services::data_store::data_directory_port::FileDataDirectoryPort::new(
                    default_data_dir.clone(),
                    active_data_dir.clone(),
                ),
            ),
        );
        services
            .settings
            .update_local_access_key("relay-local-secret".to_string())
            .await
            .expect("persist V2 local access key");
        Self {
            services,
            runtime,
            data_key,
            _root: root,
        }
    }

    pub(crate) fn config(&self, port: u16) -> ProxyStartConfig {
        let routing_repository: Arc<
            dyn crate::services::proxy::routing_repository::RoutingRepository,
        > = Arc::new(
            crate::services::proxy::routing_repository::V2RoutingRepository::new(
                self.services.routing.as_ref().clone(),
                self.data_key,
            ),
        );
        let lifecycle_store: Arc<
            dyn crate::services::proxy::lifecycle::ports::RequestLifecycleStore,
        > = self.services.request_finalization.clone();
        ProxyStartConfig::new_v2(
            routing_repository,
            lifecycle_store,
            "relay-local-secret".to_string(),
            port,
        )
    }

    pub(crate) fn runtime(&self) -> &PersistenceRuntime {
        &self.runtime
    }

    pub(crate) async fn request_logs(&self) -> Vec<RequestLog> {
        self.services
            .request_logs
            .list_recent(PageLimit::new(500).expect("bounded test limit"))
            .await
            .expect("request logs")
    }

    pub(crate) async fn upsert_model_alias(&self, client_model: &str, upstream_model: &str) {
        self.services
            .routing
            .upsert_model_alias(UpsertModelAliasInput {
                id: None,
                client_model: client_model.to_string(),
                upstream_model: upstream_model.to_string(),
                enabled: true,
                note: None,
            })
            .await
            .expect("model alias");
    }

    pub(crate) async fn seed_candidate(&self, upstream_base_url: &str) -> SeededV2Candidate {
        self.seed_candidate_named(upstream_base_url, "upstream", 0, "auto")
            .await
    }

    pub(crate) async fn seed_candidate_named(
        &self,
        upstream_base_url: &str,
        suffix: &str,
        priority: i64,
        upstream_api_format: &str,
    ) -> SeededV2Candidate {
        let station = self
            .services
            .stations
            .create(CreateStationInput {
                name: format!("V2 proxy station {suffix}"),
                station_type: "openai-compatible".to_string(),
                website_url: upstream_base_url.to_string(),
                api_base_url: format!("{}/v1", upstream_base_url.trim_end_matches('/')),
                api_key: String::new(),
                collector_proxy_mode: "direct".to_string(),
                collector_proxy_url: None,
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .await
            .expect("station");
        let key = self
            .services
            .credentials
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: format!("V2 proxy key {suffix}"),
                api_key: format!("sk-v2-{suffix}"),
                enabled: true,
                priority: Some(priority),
                max_concurrency: Some(8),
                load_factor: None,
                schedulable: Some(true),
                group_name: None,
                tier_label: None,
                group_binding_id: None,
                group_id_hash: None,
                rate_multiplier: None,
                manual_rate_multiplier: None,
                rate_source: None,
                balance_scope: None,
                note: None,
            })
            .await
            .expect("station key");
        let station_id = station.id.clone();
        let station_key_id = key.id.clone();
        let upstream_api_format = upstream_api_format.to_string();
        self.runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query("UPDATE stations SET upstream_api_format = ?1 WHERE id = ?2")
                        .bind(upstream_api_format)
                        .bind(station_id)
                        .execute(write.connection())
                        .await?;
                    sqlx::query(
                        "UPDATE station_keys SET priority = ?1, routing_order = ?1 WHERE id = ?2",
                    )
                    .bind(priority)
                    .bind(&station_key_id)
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("candidate routing fields");
        self.services
            .credentials
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: key.id.clone(),
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
            .await
            .expect("capabilities");
        SeededV2Candidate {
            station_id: station.id,
            station_key_id: key.id,
        }
    }

    pub(crate) async fn seed_balance(
        &self,
        station_id: &str,
        id: &str,
        value: f64,
        status: &str,
        collected_at: &str,
    ) {
        self.services
            .pricing
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some(id.to_string()),
                station_id: station_id.to_string(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(value),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                today_request_count: None,
                total_request_count: None,
                today_consumption: None,
                total_consumption: None,
                today_base_consumption: None,
                total_base_consumption: None,
                today_token_count: None,
                total_token_count: None,
                today_input_token_count: None,
                today_output_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: None,
                status: status.to_string(),
                source: "test".to_string(),
                confidence: 1.0,
                collected_at: Some(collected_at.to_string()),
            })
            .await
            .expect("balance snapshot");
    }

    pub(crate) async fn station_key_health(&self, station_key_id: &str) -> StationKeyHealth {
        self.services
            .routing
            .station_key_health_by_id(station_key_id)
            .await
            .expect("station key health")
    }
}

pub(crate) struct SeededV2Candidate {
    pub(crate) station_id: String,
    pub(crate) station_key_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct CapturedRequest {
    pub path_and_query: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl CapturedRequest {
    pub(crate) fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub(crate) enum ScriptedResponse {
    Json(Vec<u8>),
    Status {
        status: u16,
        reason: &'static str,
    },
    Redirect {
        location: String,
    },
    PausedSse {
        first_chunk: Vec<u8>,
        release: Arc<AtomicBool>,
    },
    DelayedHeaders {
        delay: Duration,
        body: Vec<u8>,
    },
}

pub(crate) struct LoopbackUpstream {
    pub(crate) base_url: String,
    port: u16,
    stop: Arc<AtomicBool>,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    handle: Option<JoinHandle<()>>,
}

impl LoopbackUpstream {
    pub(crate) fn script(responses: Vec<ScriptedResponse>) -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind loopback upstream");
        listener
            .set_nonblocking(true)
            .expect("nonblocking loopback upstream");
        let port = listener.local_addr().expect("loopback address").port();
        let stop = Arc::new(AtomicBool::new(false));
        let captured = Arc::new(Mutex::new(Vec::new()));
        let thread_stop = Arc::clone(&stop);
        let thread_captured = Arc::clone(&captured);
        let handle = thread::spawn(move || {
            let mut responses = responses.into_iter();
            let mut next = responses.next();
            while !thread_stop.load(Ordering::Relaxed) {
                let Some(response) = next.take() else {
                    break;
                };
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let _ = stream.set_nonblocking(false);
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                        let _ = stream.set_write_timeout(Some(Duration::from_secs(2)));
                        if let Ok(request) = read_captured_request(&mut stream) {
                            thread_captured.lock().expect("capture lock").push(request);
                            write_scripted_response(&mut stream, response);
                        }
                        let _ = stream.shutdown(Shutdown::Both);
                        next = responses.next();
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        next = Some(response);
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            base_url: format!("http://127.0.0.1:{port}"),
            port,
            stop,
            captured,
            handle: Some(handle),
        }
    }

    pub(crate) fn captured_requests(&self) -> Vec<CapturedRequest> {
        self.captured.lock().expect("capture lock").clone()
    }

    pub(crate) fn captured_count(&self) -> usize {
        self.captured.lock().expect("capture lock").len()
    }

    pub(crate) fn wait_for_requests(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.captured_count() < expected {
            assert!(
                Instant::now() < deadline,
                "timed out waiting for upstream requests"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }
}

impl Drop for LoopbackUpstream {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(("127.0.0.1", self.port));
        if let Some(handle) = self.handle.take() {
            handle.join().expect("loopback upstream joins");
        }
    }
}

fn read_captured_request(stream: &mut TcpStream) -> Result<CapturedRequest, String> {
    stream
        .set_nonblocking(false)
        .map_err(|error| format!("configure upstream request reader: {error}"))?;
    let request = parse_http_request(stream, 2 * 1024 * 1024)?;
    Ok(CapturedRequest {
        path_and_query: request.target,
        headers: request.headers,
        body: request.body,
    })
}

fn write_scripted_response(stream: &mut TcpStream, response: ScriptedResponse) {
    match response {
        ScriptedResponse::Json(body) => {
            write_response(stream, 200, "OK", "application/json", &body)
        }
        ScriptedResponse::Status { status, reason } => {
            write_response(stream, status, reason, "application/json", b"{}")
        }
        ScriptedResponse::Redirect { location } => {
            let header = format!(
                "HTTP/1.1 302 Found\r\nlocation: {location}\r\ncontent-length: 0\r\nconnection: close\r\n\r\n"
            );
            let _ = stream.write_all(header.as_bytes());
        }
        ScriptedResponse::PausedSse {
            first_chunk,
            release,
        } => {
            let header =
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&first_chunk);
            let _ = stream.flush();
            while !release.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(10));
            }
        }
        ScriptedResponse::DelayedHeaders { delay, body } => {
            thread::sleep(delay);
            write_response(stream, 200, "OK", "application/json", &body);
        }
    }
}

fn write_response(
    stream: &mut TcpStream,
    status: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) {
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}
