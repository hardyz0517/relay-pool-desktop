use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use reqwest::StatusCode;
use sqlx::Row;

use crate::{
    app_composition,
    application::{app_services::AppServices, pagination::PageLimit},
    models::{
        pricing::UpsertBalanceSnapshotInput,
        proxy::RequestLog,
        routing::{UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
        settings::UpdateSettingsInput,
        station_keys::CreateStationKeyInput,
        stations::CreateStationInput,
    },
    persistence::runtime::PersistenceRuntime,
    services::{
        data_store::data_directory_port::FileDataDirectoryPort,
        proxy::runtime::{ProxyRuntimeState, ProxyStartConfig},
        secrets::{
            crypto::generate_data_key, DeviceKeyId, DeviceKeyResolver, SecretKeyMaterial,
            CURRENT_SECRET_ENCRYPTION_VERSION,
        },
    },
};

const LOCAL_ACCESS_KEY: &str = "relay-local-secret";

pub struct RoutingLoopbackHarness {
    services: AppServices,
    runtime: PersistenceRuntime,
    proxy: Arc<ProxyRuntimeState>,
    _root: TempRoot,
}

impl RoutingLoopbackHarness {
    pub async fn new() -> Self {
        let root = TempRoot::new("relay-routing-loopback");
        let default_data_dir = root.path.join("default");
        let active_data_dir = root.path.join("active");
        fs::create_dir_all(&default_data_dir).expect("default data dir");
        fs::create_dir_all(&active_data_dir).expect("active data dir");
        let database_path = active_data_dir.join("relay-pool-desktop-v2.sqlite3");
        let runtime = PersistenceRuntime::initialize_new(&database_path)
            .await
            .expect("initialize V2 persistence runtime");
        let data_key = generate_data_key();
        let work_runtime = app_composition::compose_work_runtime(
            app_composition::WorkRuntimeConfig::architecture_budget(),
            tokio::runtime::Handle::current(),
        )
        .expect("compose work runtime");
        let services = app_composition::compose_app_services(
            runtime.handle(),
            DeviceKeyResolver::active(
                DeviceKeyId::new("test-device-key"),
                SecretKeyMaterial::from_bytes(data_key),
                CURRENT_SECRET_ENCRYPTION_VERSION,
            ),
            active_data_dir.to_string_lossy().into_owned(),
            None,
            Arc::new(FileDataDirectoryPort::new(
                default_data_dir,
                active_data_dir,
            )),
            work_runtime.blocking,
        );
        services
            .settings
            .update_local_access_key(LOCAL_ACCESS_KEY.to_string())
            .await
            .expect("persist local access key");
        Self {
            services,
            runtime,
            proxy: Arc::new(ProxyRuntimeState::default()),
            _root: root,
        }
    }

    pub async fn start_proxy(&self) -> ProxyEndpoint {
        let started = self
            .proxy
            .start(self.proxy_config(0))
            .await
            .expect("start loopback proxy");
        ProxyEndpoint {
            base_url: format!("http://127.0.0.1:{}", started.port),
            local_access_key: LOCAL_ACCESS_KEY.to_string(),
        }
    }

    pub async fn start_proxy_with_production_startup(&self) -> ProxyEndpoint {
        let port = next_free_port();
        self.update_proxy_port(port).await;
        let started = crate::services::proxy::startup::start_from_v2_persisted_settings(
            &self.services,
            self.proxy.as_ref(),
        )
        .await
        .expect("start proxy from production startup composition");
        assert_eq!(started.port, port);
        ProxyEndpoint {
            base_url: format!("http://127.0.0.1:{}", started.port),
            local_access_key: LOCAL_ACCESS_KEY.to_string(),
        }
    }

    pub async fn start_proxy_with_command_facade(&self) -> ProxyEndpoint {
        let port = next_free_port();
        self.update_proxy_port(port).await;
        let facade = app_composition::compose_local_proxy_command_facade(
            &self.services,
            Arc::clone(&self.proxy),
        );
        let started = facade
            .start_local_proxy()
            .await
            .expect("start proxy through production command facade");
        assert_eq!(started.port, port);
        ProxyEndpoint {
            base_url: format!("http://127.0.0.1:{}", started.port),
            local_access_key: LOCAL_ACCESS_KEY.to_string(),
        }
    }

    pub async fn stop_proxy(&self) {
        let _ = self.proxy.stop(0).await.expect("stop proxy");
    }

    pub fn proxy_status(&self) -> ProxyStatusSummary {
        let status = self.proxy.status(0);
        ProxyStatusSummary {
            running: status.running,
            active_requests: status.active_requests,
            request_count: status.request_count,
        }
    }

    pub async fn seed_candidate(
        &self,
        upstream_base_url: &str,
        suffix: &str,
        priority: i64,
    ) -> SeededCandidate {
        let station = self
            .services
            .stations
            .create(CreateStationInput {
                name: format!("Loopback station {suffix}"),
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
            .expect("create station");
        let key = self
            .services
            .credentials
            .create_station_key(CreateStationKeyInput {
                station_id: station.id.clone(),
                name: format!("Loopback key {suffix}"),
                api_key: format!("sk-loopback-{suffix}"),
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
            .expect("create station key");
        let station_id = station.id.clone();
        let station_key_id = key.id.clone();
        self.runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query("UPDATE stations SET upstream_api_format = 'auto' WHERE id = ?")
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
            .expect("update capabilities");
        SeededCandidate {
            station_id: station.id,
            station_key_id: key.id,
            api_key: format!("sk-loopback-{suffix}"),
        }
    }

    pub async fn set_routing_strategy(&self, strategy: &str) {
        let strategy = strategy.to_string();
        self.runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "UPDATE settings SET value = ?1, updated_at = strftime('%s', 'now')
                         WHERE key = 'default_routing_strategy'",
                    )
                    .bind(strategy)
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("set routing strategy");
    }

    pub async fn update_candidate_capabilities(
        &self,
        candidate: &SeededCandidate,
        config: CandidateCapabilityConfig,
    ) {
        self.services
            .credentials
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: candidate.station_key_id.clone(),
                supports_chat_completions: config.supports_chat_completions,
                supports_responses: config.supports_responses,
                supports_embeddings: config.supports_embeddings,
                supports_stream: config.supports_stream,
                supports_tools: config.supports_tools,
                supports_vision: config.supports_vision,
                supports_reasoning: config.supports_reasoning,
                model_allowlist: config.model_allowlist,
                model_blocklist: config.model_blocklist,
                preferred_models: config.preferred_models,
                only_use_as_backup: config.only_use_as_backup,
                routing_tags: config.routing_tags,
            })
            .await
            .expect("update loopback candidate capabilities");
    }

    pub async fn upsert_model_alias(&self, client_model: &str, upstream_model: &str) {
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

    pub async fn seed_balance(&self, station_id: &str, value: f64) {
        self.services
            .pricing
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some(format!("balance-{station_id}")),
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
                today_input_token_count: None,
                today_output_token_count: None,
                total_token_count: None,
                total_input_token_count: None,
                total_output_token_count: None,
                account_concurrency_limit: None,
                low_balance_threshold: None,
                status: "healthy".to_string(),
                source: "routing_loopback".to_string(),
                confidence: 1.0,
                collected_at: Some("2026-07-31T00:00:00Z".to_string()),
            })
            .await
            .expect("balance snapshot");
    }

    pub async fn request_log_summaries(&self) -> Vec<RequestLogSummary> {
        self.services
            .request_logs
            .list_recent(PageLimit::new(500).expect("bounded test limit"))
            .await
            .expect("request logs")
            .into_iter()
            .map(RequestLogSummary::from)
            .collect()
    }

    pub async fn attempt_terminal_summaries(
        &self,
        request_log_id: &str,
    ) -> Vec<AttemptTerminalSummary> {
        let mut read = self
            .runtime
            .handle()
            .begin_read()
            .await
            .expect("begin attempt terminal read");
        sqlx::query(
            "SELECT ordinal, station_key_id, terminal_kind, failure_kind, public_code, output_committed
             FROM request_attempts
             WHERE request_id = ?
             ORDER BY ordinal ASC",
        )
        .bind(request_log_id)
        .fetch_all(read.connection())
        .await
        .expect("attempt terminal rows")
        .into_iter()
        .map(|row| AttemptTerminalSummary {
            ordinal: row.get::<i64, _>("ordinal"),
            station_key_id: row.get("station_key_id"),
            terminal_kind: row.get("terminal_kind"),
            failure_kind: row.get("failure_kind"),
            public_code: row.get("public_code"),
            output_committed: row.get::<i64, _>("output_committed") != 0,
        })
        .collect()
    }

    pub async fn attempt_cost_count(&self, request_log_id: &str) -> i64 {
        let mut read = self
            .runtime
            .handle()
            .begin_read()
            .await
            .expect("begin attempt cost read");
        sqlx::query_scalar("SELECT COUNT(*) FROM routing_attempt_costs WHERE request_id = ?")
            .bind(request_log_id)
            .fetch_one(read.connection())
            .await
            .expect("attempt cost count")
    }

    pub async fn cost_aggregate_summary(
        &self,
        request_log_id: &str,
    ) -> Option<RequestCostAggregateSummary> {
        let mut read = self
            .runtime
            .handle()
            .begin_read()
            .await
            .expect("begin request cost aggregate read");
        sqlx::query(
            "SELECT status, totals_by_currency_json, compatibility_currency,
                    compatibility_total_cost_micro, incomplete_attempts_json
             FROM routing_request_cost_aggregates
             WHERE request_id = ?",
        )
        .bind(request_log_id)
        .fetch_optional(read.connection())
        .await
        .expect("request cost aggregate row")
        .map(|row| RequestCostAggregateSummary {
            status: row.get("status"),
            totals_by_currency_json: row.get("totals_by_currency_json"),
            compatibility_currency: row.get("compatibility_currency"),
            compatibility_total_cost_micro: row.get("compatibility_total_cost_micro"),
            incomplete_attempts_json: row.get("incomplete_attempts_json"),
        })
    }

    pub async fn seed_in_progress_request_lifecycle(&self, request_id: &str) {
        let request_id = request_id.to_string();
        self.runtime
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO request_logs (
                            id, request_id, started_at, method, path, endpoint, status,
                            lifecycle_status, created_at
                         ) VALUES (?1, ?1, '1000', 'POST', '/v1/chat/completions',
                                   'chat_completions', 'in_progress', 'admitted', '1000')",
                    )
                    .bind(&request_id)
                    .execute(write.connection())
                    .await?;
                    sqlx::query(
                        "INSERT INTO request_attempts (
                            request_id, ordinal, station_id, station_key_id, endpoint_revision,
                            started_at_ms, terminal_kind, health_effect, output_committed, terminal_at_ms
                         ) VALUES (?1, 0, 'station-startup', 'key-startup', 1,
                                   1000, 'succeeded', 'success', 1, 1100)",
                    )
                    .bind(&request_id)
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("seed in-progress request lifecycle");
    }

    pub async fn request_lifecycle_status(
        &self,
        request_id: &str,
    ) -> RequestLifecycleStatusSummary {
        let mut read = self
            .runtime
            .handle()
            .begin_read()
            .await
            .expect("begin request lifecycle status read");
        let row = sqlx::query(
            "SELECT status, lifecycle_status, terminal_kind, terminal_code, terminal_at_ms
             FROM request_logs WHERE request_id = ?",
        )
        .bind(request_id)
        .fetch_one(read.connection())
        .await
        .expect("request lifecycle status row");
        RequestLifecycleStatusSummary {
            status: row.get("status"),
            lifecycle_status: row.get("lifecycle_status"),
            terminal_kind: row.get("terminal_kind"),
            terminal_code: row.get("terminal_code"),
            terminal_at_ms: row.get("terminal_at_ms"),
        }
    }

    pub async fn startup_reconciliation_requests_interrupted(&self) -> i64 {
        let mut read = self
            .runtime
            .handle()
            .begin_read()
            .await
            .expect("begin reconciliation progress read");
        sqlx::query_scalar(
            "SELECT requests_interrupted
             FROM routing_lifecycle_reconciliation_progress
             WHERE singleton_key = 1",
        )
        .fetch_one(read.connection())
        .await
        .expect("reconciliation progress row")
    }

    fn proxy_config(&self, port: u16) -> ProxyStartConfig {
        let routing_repository: Arc<
            dyn crate::services::proxy::routing_repository::RoutingRepository,
        > = Arc::new(
            crate::services::proxy::routing_repository::V2RoutingRepository::new(
                self.services.routing.as_ref().clone(),
            ),
        );
        let lifecycle_store: Arc<
            dyn crate::services::proxy::lifecycle::ports::RequestLifecycleStore,
        > = self.services.request_finalization.clone();
        ProxyStartConfig::new_v2(
            routing_repository,
            self.services.credentials.clone(),
            lifecycle_store,
            LOCAL_ACCESS_KEY.to_string(),
            port,
        )
    }

    async fn update_proxy_port(&self, port: u16) {
        let settings = self.services.settings.load().await.expect("settings");
        self.services
            .settings
            .update(UpdateSettingsInput {
                local_proxy_port: port,
                default_routing_strategy: settings.default_routing_strategy,
                collector_proxy_mode: settings.collector_proxy_mode,
                collector_proxy_url: settings.collector_proxy_url,
                max_rate_multiplier: Some(settings.max_rate_multiplier),
                default_routing_group_filter: Some(settings.default_routing_group_filter),
                scheduler_advanced_settings: Some(settings.scheduler_advanced_settings),
                low_balance_threshold_cny: settings.low_balance_threshold_cny,
                collector_interval_minutes: settings.collector_interval_minutes,
                balance_interval_minutes: settings.balance_interval_minutes,
                group_rate_interval_minutes: settings.group_rate_interval_minutes,
                model_list_interval_minutes: settings.model_list_interval_minutes,
                pricing_refresh_interval_minutes: settings.pricing_refresh_interval_minutes,
                collector_timeout_seconds: settings.collector_timeout_seconds,
                collector_max_concurrency: settings.collector_max_concurrency,
                allow_depleted_fallback: settings.allow_depleted_fallback,
                developer_mode_enabled: settings.developer_mode_enabled,
                tray_behavior: Some(settings.tray_behavior),
            })
            .await
            .expect("update proxy port");
    }
}

fn next_free_port() -> u16 {
    let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind free port");
    listener.local_addr().expect("free port address").port()
}

pub struct ProxyEndpoint {
    pub base_url: String,
    local_access_key: String,
}

impl ProxyEndpoint {
    pub async fn post_json(&self, path: &str, body: serde_json::Value) -> LoopbackHttpResponse {
        let response = reqwest::Client::new()
            .post(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.local_access_key)
            .json(&body)
            .send()
            .await
            .expect("proxy request");
        LoopbackHttpResponse::from_response(response).await
    }

    pub async fn get(&self, path: &str) -> LoopbackHttpResponse {
        let response = reqwest::Client::new()
            .get(format!("{}{}", self.base_url, path))
            .bearer_auth(&self.local_access_key)
            .send()
            .await
            .expect("proxy request");
        LoopbackHttpResponse::from_response(response).await
    }
}

pub struct LoopbackHttpResponse {
    pub status: StatusCode,
    pub body: Bytes,
}

impl LoopbackHttpResponse {
    async fn from_response(response: reqwest::Response) -> Self {
        let status = response.status();
        let body = response.bytes().await.expect("response body");
        Self { status, body }
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestLogSummary {
    pub id: String,
    pub status: String,
    pub lifecycle_status: Option<String>,
    pub station_key_id: Option<String>,
    pub fallback_count: i64,
    pub attempt_count: Option<i64>,
    pub completion_source: Option<String>,
    pub failure_source: Option<String>,
    pub route_policy: Option<String>,
    pub error_message: Option<String>,
}

impl From<RequestLog> for RequestLogSummary {
    fn from(value: RequestLog) -> Self {
        Self {
            id: value.id,
            status: value.status,
            lifecycle_status: value.lifecycle_status,
            station_key_id: value.station_key_id,
            fallback_count: value.fallback_count,
            attempt_count: value.attempt_count,
            completion_source: value.completion_source,
            failure_source: value.failure_source,
            route_policy: value.route_policy,
            error_message: value.error_message,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AttemptTerminalSummary {
    pub ordinal: i64,
    pub station_key_id: String,
    pub terminal_kind: String,
    pub failure_kind: Option<String>,
    pub public_code: Option<String>,
    pub output_committed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestCostAggregateSummary {
    pub status: String,
    pub totals_by_currency_json: String,
    pub compatibility_currency: Option<String>,
    pub compatibility_total_cost_micro: Option<i64>,
    pub incomplete_attempts_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyStatusSummary {
    pub running: bool,
    pub active_requests: u32,
    pub request_count: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestLifecycleStatusSummary {
    pub status: String,
    pub lifecycle_status: Option<String>,
    pub terminal_kind: Option<String>,
    pub terminal_code: Option<String>,
    pub terminal_at_ms: Option<i64>,
}

#[derive(Clone, PartialEq, Eq)]
pub struct SeededCandidate {
    pub station_id: String,
    pub station_key_id: String,
    pub api_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CandidateCapabilityConfig {
    pub supports_chat_completions: bool,
    pub supports_responses: bool,
    pub supports_embeddings: bool,
    pub supports_stream: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub supports_reasoning: bool,
    pub model_allowlist: Vec<String>,
    pub model_blocklist: Vec<String>,
    pub preferred_models: Vec<String>,
    pub only_use_as_backup: bool,
    pub routing_tags: Vec<String>,
}

impl Default for CandidateCapabilityConfig {
    fn default() -> Self {
        Self {
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
        }
    }
}

#[derive(Debug, Clone)]
pub struct CapturedRequest {
    pub path_and_query: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl CapturedRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .get(&name.to_ascii_lowercase())
            .map(String::as_str)
    }
}

#[derive(Debug, Clone)]
pub enum ScriptedResponse {
    Json(Vec<u8>),
    Status {
        status: u16,
        reason: &'static str,
    },
    Raw {
        status: u16,
        reason: &'static str,
        content_type: &'static str,
        body: Vec<u8>,
    },
}

pub struct LoopbackUpstream {
    pub base_url: String,
    port: u16,
    stop: Arc<AtomicBool>,
    captured: Arc<Mutex<Vec<CapturedRequest>>>,
    handle: Option<JoinHandle<()>>,
}

impl LoopbackUpstream {
    pub fn script(responses: Vec<ScriptedResponse>) -> Self {
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

    pub fn captured_requests(&self) -> Vec<CapturedRequest> {
        self.captured.lock().expect("capture lock").clone()
    }

    pub fn wait_for_requests(&self, expected: usize) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while self.captured.lock().expect("capture lock").len() < expected {
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

struct TempRoot {
    path: PathBuf,
}

impl TempRoot {
    fn new(prefix: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let now_nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after Unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "{}-{}-{}-{}",
            prefix,
            std::process::id(),
            now_nanos,
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("temp path has unicode file name");
        assert!(
            file_name.starts_with(prefix),
            "loopback temp cleanup must stay inside the owned prefix"
        );
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("temp root");
        Self { path }
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn read_captured_request(stream: &mut TcpStream) -> Result<CapturedRequest, String> {
    let mut buffer = Vec::new();
    let mut chunk = [0_u8; 4096];
    loop {
        let read = stream
            .read(&mut chunk)
            .map_err(|error| format!("read upstream request: {error}"))?;
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if buffer.len() > 2 * 1024 * 1024 {
            return Err("upstream request too large".to_string());
        }
        if captured_request_complete(&buffer) {
            break;
        }
    }
    let request = parse_captured_http_request(&buffer)?;
    Ok(CapturedRequest {
        path_and_query: request.0,
        headers: request.1,
        body: request.2,
    })
}

fn captured_request_complete(buffer: &[u8]) -> bool {
    let Some(header_end) = find_header_end(buffer) else {
        return false;
    };
    let headers = String::from_utf8_lossy(&buffer[..header_end]);
    let content_length = headers
        .lines()
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("content-length")
                .then(|| value.trim().parse::<usize>().ok())
                .flatten()
        })
        .unwrap_or(0);
    buffer.len() >= header_end + 4 + content_length
}

fn parse_captured_http_request(
    buffer: &[u8],
) -> Result<(String, HashMap<String, String>, Vec<u8>), String> {
    let header_end = find_header_end(buffer).ok_or_else(|| "missing header end".to_string())?;
    let headers_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = headers_text.lines();
    let request_line = lines
        .next()
        .ok_or_else(|| "missing request line".to_string())?;
    let target = request_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| "missing request target".to_string())?
        .to_string();
    let mut headers = HashMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    let body_end = body_start + content_length;
    if buffer.len() < body_end {
        return Err("incomplete request body".to_string());
    }
    Ok((target, headers, buffer[body_start..body_end].to_vec()))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn write_scripted_response(stream: &mut TcpStream, response: ScriptedResponse) {
    match response {
        ScriptedResponse::Json(body) => {
            write_response(stream, 200, "OK", "application/json", &body)
        }
        ScriptedResponse::Status { status, reason } => {
            write_response(stream, status, reason, "application/json", b"{}")
        }
        ScriptedResponse::Raw {
            status,
            reason,
            content_type,
            body,
        } => write_response(stream, status, reason, content_type, &body),
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
