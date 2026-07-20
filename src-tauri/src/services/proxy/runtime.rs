use std::{
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use crate::{
    application::request_finalization::RequestFinalizationService,
    models::proxy::{ProxyLifecycle, ProxyStatus},
    persistence::runtime::PersistenceRuntime,
    services::{
        database::{now_millis_for_services, AppDatabase},
        proxy::{
            execution::{ExecutionEngine, UpstreamAttemptExecutor},
            ingress::{self, IngressExecutor, IngressState},
            lifecycle::{
                delivery::DeliveryTerminal,
                ports::LifecycleWriteError,
                request::PendingFinalRequestRecord,
                writer::{LifecycleWriter, LifecycleWriterWorker, WriterAdmissionError},
            },
            limits::ProxyServerLimits,
            request::{ProxyHttpResponse, ProxyResponsePayload},
            response_body::{
                buffered_lifecycle_finalizing_stream,
                lifecycle_finalizing_stream_with_idle_timeout, FinalizationOutcome,
                LifecycleFinalizationLease, SelectedAttemptFinalization,
            },
            routing_repository::{RoutingRepository, SqliteRoutingRepository},
            server::{self, RunningServer},
            upstream::UpstreamClientPool,
        },
    },
};
use futures_util::future::BoxFuture;

#[derive(Clone)]
pub struct ProxyStartConfig {
    pub database: AppDatabase,
    pub data_key: [u8; 32],
    pub port: u16,
    pub limits: ProxyServerLimits,
}

impl ProxyStartConfig {
    pub fn new(database: AppDatabase, data_key: [u8; 32], port: u16) -> Self {
        Self {
            database,
            data_key,
            port,
            limits: ProxyServerLimits::default(),
        }
    }
}

pub struct ProxyRuntimeState {
    v2: tokio::sync::Mutex<V2RuntimeInner>,
    lifecycle_operation: tokio::sync::Mutex<()>,
    status_snapshot: RwLock<ProxyStatus>,
}

impl Default for ProxyRuntimeState {
    fn default() -> Self {
        Self {
            v2: tokio::sync::Mutex::new(V2RuntimeInner::default()),
            lifecycle_operation: tokio::sync::Mutex::new(()),
            status_snapshot: RwLock::new(default_status(0)),
        }
    }
}

impl ProxyRuntimeState {
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::default()
    }

    pub fn status(&self, default_port: u16) -> ProxyStatus {
        let snapshot = self
            .status_snapshot
            .read()
            .unwrap_or_else(|error| error.into_inner())
            .clone();
        let snapshot = if let Ok(inner) = self.v2.try_lock() {
            if let Some(server) = inner.server.as_ref() {
                ProxyStatus {
                    running: true,
                    lifecycle: ProxyLifecycle::Running,
                    bind_addr: server.local_addr.ip().to_string(),
                    port: server.local_addr.port(),
                    started_at: snapshot.started_at,
                    last_error: snapshot.last_error,
                    active_requests: server.active_requests.load(Ordering::Relaxed),
                    request_count: server.request_count.load(Ordering::Relaxed),
                }
            } else {
                snapshot
            }
        } else {
            snapshot
        };
        if snapshot.port == 0 {
            ProxyStatus {
                port: default_port,
                ..snapshot
            }
        } else {
            snapshot
        }
    }

    pub async fn start(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        self.v2_start(config).await
    }

    pub async fn stop(&self, default_port: u16) -> Result<ProxyStatus, String> {
        self.v2_stop(default_port).await
    }

    pub async fn prepare_for_update(&self, timeout: Duration) -> Result<ProxyStatus, String> {
        self.v2_prepare_for_update(timeout).await
    }

    pub async fn cleanup_before_update(&self, default_port: u16) -> Result<ProxyStatus, String> {
        self.stop(default_port).await
    }

    pub async fn restart(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        let port = config.port;
        let _ = self.v2_stop(port).await?;
        self.v2_start(config).await
    }

    async fn v2_start(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        let _operation = self.lifecycle_operation.lock().await;
        {
            let inner = self.v2.lock().await;
            if let Some(server) = inner.server.as_ref() {
                if server.local_addr.port() == config.port || config.port == 0 {
                    return Ok(self.v2_status_from_inner(&inner, server.local_addr.port()));
                }
                return Err(format!(
                    "local proxy is already running on port {}; stop it before starting port {}",
                    server.local_addr.port(),
                    config.port
                ));
            }
        }

        self.publish_status(ProxyStatus {
            running: false,
            lifecycle: ProxyLifecycle::Starting,
            bind_addr: "127.0.0.1".to_string(),
            port: config.port,
            started_at: None,
            last_error: None,
            active_requests: 0,
            request_count: 0,
        });

        let local_access_key = match config.database.ensure_secure_local_access_key() {
            Ok(key) => key,
            Err(error) => {
                let failed = failed_status(config.port, error.clone());
                self.publish_status(failed);
                return Err(error);
            }
        };

        let active_requests = Arc::new(AtomicU32::new(0));
        let request_count = Arc::new(AtomicU64::new(0));
        let repository: Arc<dyn RoutingRepository> = Arc::new(SqliteRoutingRepository::new(
            config.database.clone(),
            config.data_key,
        ));
        let upstream_pool = UpstreamClientPool::new(config.limits.clone()).map_err(|failure| {
            let message = failure.public_message.clone();
            let failed = failed_status(config.port, message.clone());
            self.publish_status(failed);
            message
        })?;
        let persistence_runtime = Arc::new(
            PersistenceRuntime::open_current(config.database.db_path())
                .await
                .map_err(|error| {
                    let message = format!("open persistence runtime failed: {error}");
                    let failed = failed_status(config.port, message.clone());
                    self.publish_status(failed);
                    message
                })?,
        );
        let lifecycle_store = Arc::new(RequestFinalizationService::new(
            persistence_runtime.handle(),
        ));
        let (lifecycle_writer, lifecycle_worker) =
            LifecycleWriter::start(lifecycle_writer_capacity(&config.limits), lifecycle_store)
                .map_err(|error| {
                    let message = format!("start lifecycle writer failed: {error:?}");
                    let failed = failed_status(config.port, message.clone());
                    self.publish_status(failed);
                    message
                })?;
        let executor = Arc::new(V2ProxyExecutor::new(
            repository,
            upstream_pool,
            config.limits.clone(),
            lifecycle_writer.clone(),
        ));
        let ingress_state = Arc::new(IngressState::with_active_requests(
            local_access_key,
            config.limits.clone(),
            executor,
            Arc::clone(&active_requests),
            Arc::clone(&request_count),
            Some(lifecycle_writer),
        ));
        let app = ingress::router(ingress_state);
        match server::spawn_server(
            config.port,
            config.limits,
            app,
            Arc::clone(&active_requests),
            Arc::clone(&request_count),
        )
        .await
        {
            Ok(server) => {
                let started = ProxyStatus {
                    running: true,
                    lifecycle: ProxyLifecycle::Running,
                    bind_addr: server.local_addr.ip().to_string(),
                    port: server.local_addr.port(),
                    started_at: Some(now_string()),
                    last_error: None,
                    active_requests: 0,
                    request_count: 0,
                };
                let mut inner = self.v2.lock().await;
                inner.server = Some(server);
                inner.lifecycle_worker = Some(lifecycle_worker);
                self.publish_status(started.clone());
                Ok(started)
            }
            Err(error) => {
                let failed = failed_status(config.port, error.clone());
                self.publish_status(failed);
                Err(error)
            }
        }
    }

    async fn v2_stop(&self, default_port: u16) -> Result<ProxyStatus, String> {
        let _operation = self.lifecycle_operation.lock().await;
        let server = {
            let mut inner = self.v2.lock().await;
            let Some(server) = inner.server.take() else {
                let stopped = default_status(default_port);
                self.publish_status(stopped.clone());
                return Ok(stopped);
            };
            self.publish_status(ProxyStatus {
                running: true,
                lifecycle: ProxyLifecycle::Stopping,
                bind_addr: server.local_addr.ip().to_string(),
                port: server.local_addr.port(),
                started_at: self.status(default_port).started_at,
                last_error: None,
                active_requests: server.active_requests.load(Ordering::Relaxed),
                request_count: server.request_count.load(Ordering::Relaxed),
            });
            server
        };
        let port = server.local_addr.port();
        let stop_result = server.stop(Duration::from_secs(1)).await;
        let stopped = match stop_result {
            Ok(()) => default_status(port),
            Err(error) => failed_status(port, error),
        };
        if let Some(worker) = self.v2.lock().await.lifecycle_worker.take() {
            let _ = worker.join().await;
        }
        self.publish_status(stopped.clone());
        if stopped.lifecycle == ProxyLifecycle::Failed {
            Err(stopped
                .last_error
                .clone()
                .unwrap_or_else(|| "proxy stop failed".to_string()))
        } else {
            Ok(stopped)
        }
    }

    async fn v2_prepare_for_update(&self, timeout: Duration) -> Result<ProxyStatus, String> {
        let _operation = self.lifecycle_operation.lock().await;
        let server = {
            let mut inner = self.v2.lock().await;
            let Some(server) = inner.server.take() else {
                let stopped = default_status(0);
                self.publish_status(stopped.clone());
                return Ok(stopped);
            };
            self.publish_status(ProxyStatus {
                running: true,
                lifecycle: ProxyLifecycle::Draining,
                bind_addr: server.local_addr.ip().to_string(),
                port: server.local_addr.port(),
                started_at: self.status(server.local_addr.port()).started_at,
                last_error: None,
                active_requests: server.active_requests.load(Ordering::Relaxed),
                request_count: server.request_count.load(Ordering::Relaxed),
            });
            server
        };
        let port = server.local_addr.port();
        let stop_result = server.stop(timeout).await;
        let stopped = match stop_result {
            Ok(()) => default_status(port),
            Err(error) => failed_status(port, error),
        };
        if let Some(worker) = self.v2.lock().await.lifecycle_worker.take() {
            let _ = worker.join().await;
        }
        self.publish_status(stopped.clone());
        if stopped.lifecycle == ProxyLifecycle::Failed {
            Err(stopped
                .last_error
                .clone()
                .unwrap_or_else(|| "proxy drain failed".to_string()))
        } else {
            Ok(stopped)
        }
    }

    fn v2_status_from_inner(&self, inner: &V2RuntimeInner, default_port: u16) -> ProxyStatus {
        if let Some(server) = inner.server.as_ref() {
            ProxyStatus {
                running: true,
                lifecycle: ProxyLifecycle::Running,
                bind_addr: server.local_addr.ip().to_string(),
                port: server.local_addr.port(),
                started_at: self.status(default_port).started_at,
                last_error: None,
                active_requests: server.active_requests.load(Ordering::Relaxed),
                request_count: server.request_count.load(Ordering::Relaxed),
            }
        } else {
            self.status(default_port)
        }
    }

    fn publish_status(&self, status: ProxyStatus) {
        *self
            .status_snapshot
            .write()
            .unwrap_or_else(|error| error.into_inner()) = status;
    }
}

#[derive(Default)]
struct V2RuntimeInner {
    server: Option<RunningServer>,
    lifecycle_worker: Option<LifecycleWriterWorker>,
}

struct V2ProxyExecutor {
    engine: ExecutionEngine,
    stream_idle_timeout: std::time::Duration,
    lifecycle_writer: LifecycleWriter,
}

impl V2ProxyExecutor {
    fn new(
        repository: Arc<dyn RoutingRepository>,
        upstream_pool: UpstreamClientPool,
        limits: ProxyServerLimits,
        lifecycle_writer: LifecycleWriter,
    ) -> Self {
        let attempts = Arc::new(UpstreamAttemptExecutor::new(upstream_pool));
        let stream_idle_timeout = limits.stream_idle_timeout;
        Self {
            engine: ExecutionEngine::new_with_limits_and_lifecycle(
                repository,
                attempts,
                &limits,
                lifecycle_writer.clone(),
            ),
            stream_idle_timeout,
            lifecycle_writer,
        }
    }
}

impl IngressExecutor for V2ProxyExecutor {
    fn execute(
        &self,
        mut request: super::request::CanonicalProxyRequest,
    ) -> BoxFuture<'static, Result<ProxyHttpResponse, super::error::ProxyFailure>> {
        let lifecycle_writer = self.lifecycle_writer.clone();
        let engine = self.engine.clone();
        let stream_idle_timeout = self.stream_idle_timeout;
        let Some(admission) = request.take_lifecycle_admission() else {
            return Box::pin(async move {
                Err(lifecycle_unavailable_failure(
                    "missing lifecycle admission for v2 request",
                ))
            });
        };
        let Some(request_lease) = request.take_request_lease() else {
            return Box::pin(async move {
                Err(lifecycle_unavailable_failure(
                    "missing request lease for v2 request",
                ))
            });
        };
        let request_context = admission.context;
        let request_model = request.model.clone();
        let request_stream = request.stream;
        let request_reasoning_effort = request.reasoning_effort.clone();
        Box::pin(async move {
            let response = match engine.execute(request).await {
                Ok(response) => response,
                Err(failure) => {
                    let finalization_lease =
                        LifecycleFinalizationLease::new(admission.terminal, None);
                    let request_id = request_context.request_id.clone();
                    let attempt_count = failure.attempt_count.unwrap_or_else(|| {
                        if failure.candidate_id.is_some() {
                            1
                        } else {
                            0
                        }
                    }) as u16;
                    let fallback_count = attempt_count.saturating_sub(1);
                    let annotations =
                        crate::services::proxy::lifecycle::request::RequestLogAnnotations {
                            model: request_model.clone(),
                            stream: request_stream,
                            selected_station_key_id: failure.candidate_id.clone(),
                            selected_station_id: failure.candidate_station_id.clone(),
                            upstream_base_url: failure.candidate_upstream_base_url.clone(),
                            route_policy: failure.route_policy.clone(),
                            route_reason: None,
                            rejected_candidates_json: None,
                            body_bytes: None,
                            route_wait_ms: Some(0),
                            upstream_headers_ms: None,
                            failure_source: Some(failure.source.as_str().to_string()),
                            attempts_json: None,
                            completion_source: Some("precommit_failure".to_string()),
                            prompt_tokens: None,
                            completion_tokens: None,
                            total_tokens: None,
                            cache_creation_tokens: None,
                            cache_read_tokens: None,
                            reasoning_effort: request_reasoning_effort.clone(),
                            first_token_ms: None,
                        };
                    finalization_lease.finalize(
                        PendingFinalRequestRecord::new(
                            request_context.clone(),
                            failure.candidate_id.as_ref().map(|_| {
                                crate::services::proxy::lifecycle::request::AttemptId::new(
                                    request_id,
                                    fallback_count,
                                )
                            }),
                            attempt_count,
                            fallback_count,
                            annotations,
                        ),
                        DeliveryTerminal::NotStarted,
                        FinalizationOutcome::Failed {
                            code: failure.code.as_str().to_string(),
                            detail: Some(failure.public_message.clone()),
                        },
                        None,
                    );
                    return Err(failure);
                }
            };
            let status = response.status;
            let headers = response.headers;
            let lifecycle = response.lifecycle;
            let selected_attempt =
                if let Some(selected_attempt) = lifecycle.selected_attempt.as_ref() {
                    Some(SelectedAttemptFinalization::new(
                        lifecycle_writer
                            .try_reserve_attempt()
                            .map_err(lifecycle_admission_failure)?,
                        selected_attempt.clone(),
                    ))
                } else {
                    None
                };
            let finalization_lease =
                LifecycleFinalizationLease::new(admission.terminal, selected_attempt);
            let pending_record = PendingFinalRequestRecord::new(
                request_context.clone(),
                lifecycle
                    .selected_attempt
                    .as_ref()
                    .map(|attempt| attempt.attempt_id.clone()),
                lifecycle.attempt_count,
                lifecycle.fallback_count,
                lifecycle.annotations,
            );
            let payload = match response.body {
                super::execution::ProxyExecutionBody::Buffered(body) => {
                    ProxyResponsePayload::Stream(buffered_lifecycle_finalizing_stream(
                        body,
                        pending_record,
                        finalization_lease,
                        request_lease,
                    ))
                }
                super::execution::ProxyExecutionBody::Stream(chunks) => {
                    ProxyResponsePayload::Stream(lifecycle_finalizing_stream_with_idle_timeout(
                        chunks,
                        pending_record,
                        finalization_lease,
                        request_lease,
                        stream_idle_timeout,
                    ))
                }
            };
            Ok(ProxyHttpResponse {
                status,
                headers,
                payload,
            })
        })
    }
}

fn lifecycle_writer_capacity(limits: &ProxyServerLimits) -> usize {
    limits
        .max_in_flight_requests
        .saturating_mul(4)
        .saturating_add(16)
        .max(8)
}

fn lifecycle_admission_failure(error: WriterAdmissionError) -> super::error::ProxyFailure {
    lifecycle_unavailable_failure(format!("lifecycle writer admission rejected: {error:?}"))
}

fn lifecycle_write_failure(error: LifecycleWriteError) -> super::error::ProxyFailure {
    lifecycle_unavailable_failure(format!("lifecycle write failed: {error:?}"))
}

fn lifecycle_unavailable_failure(message: impl Into<String>) -> super::error::ProxyFailure {
    super::error::ProxyFailure::new(
        super::error::ProxyFailureCode::LocalProxyBusy,
        super::error::FailureSource::Local,
        super::error::RetryClass::Never,
        http::StatusCode::SERVICE_UNAVAILABLE,
        message,
    )
}

fn default_status(port: u16) -> ProxyStatus {
    ProxyStatus {
        running: false,
        lifecycle: ProxyLifecycle::Stopped,
        bind_addr: "127.0.0.1".to_string(),
        port,
        started_at: None,
        last_error: None,
        active_requests: 0,
        request_count: 0,
    }
}

fn failed_status(port: u16, error: String) -> ProxyStatus {
    ProxyStatus {
        running: false,
        lifecycle: ProxyLifecycle::Failed,
        bind_addr: "127.0.0.1".to_string(),
        port,
        started_at: None,
        last_error: Some(error),
        active_requests: 0,
        request_count: 0,
    }
}

fn now_string() -> String {
    now_millis_for_services().to_string()
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering as AtomicOrdering},
            Arc,
        },
        time::Duration,
    };

    use http::StatusCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::{
        models::{
            pricing::UpsertBalanceSnapshotInput,
            routing::{UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
            stations::CreateStationInput,
        },
        services::proxy::test_support::{LoopbackUpstream, ScriptedResponse},
    };

    use super::*;

    #[tokio::test]
    async fn v2_runtime_transitions_start_run_drain_stop() {
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(test_start_config(0)).await.expect("start");
        assert_eq!(started.lifecycle, ProxyLifecycle::Running);
        assert_ne!(started.port, 0);

        let draining = runtime
            .prepare_for_update(Duration::from_millis(250))
            .await
            .expect("drain");
        assert_eq!(draining.lifecycle, ProxyLifecycle::Stopped);
        assert!(!draining.running);
    }

    #[tokio::test]
    async fn v2_runtime_reports_bind_failure_and_recovers() {
        let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = occupied.local_addr().unwrap().port();
        let runtime = ProxyRuntimeState::for_tests();
        assert!(runtime.start(test_start_config(port)).await.is_err());
        assert_eq!(runtime.status(port).lifecycle, ProxyLifecycle::Failed);
        drop(occupied);
        assert_eq!(
            runtime
                .start(test_start_config(port))
                .await
                .unwrap()
                .lifecycle,
            ProxyLifecycle::Running
        );
        runtime.stop(port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_runtime_is_idempotent_for_same_port_and_rejects_port_change() {
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(test_start_config(0)).await.unwrap();
        let same = runtime
            .start(test_start_config(started.port))
            .await
            .unwrap();
        assert_eq!(same.port, started.port);
        let different = next_free_port().await;
        assert!(runtime.start(test_start_config(different)).await.is_err());
        runtime.stop(started.port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_runtime_33rd_request_receives_busy_response() {
        let runtime = ProxyRuntimeState::for_tests();
        let mut config = test_start_config(0);
        config.limits.max_in_flight_requests = 1;
        let started = runtime.start(config).await.unwrap();
        let mut first = tokio::net::TcpStream::connect(("127.0.0.1", started.port))
            .await
            .unwrap();
        first
            .write_all(
                b"POST /v1/responses HTTP/1.1\r\nhost: 127.0.0.1\r\nauthorization: Bearer relay-local-secret\r\ncontent-type: application/json\r\ncontent-length: 999\r\n\r\n{}",
            )
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(25)).await;

        let client = reqwest::Client::new();
        let response = client
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({}))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let body: serde_json::Value = response.json().await.unwrap();
        assert_eq!(body["error"]["code"], "local_proxy_busy");

        drop(first);
        runtime.stop(started.port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_buffered_chat_routes_through_real_listener_and_logs_once() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"chatcmpl-v2","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_vec(),
        )]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate(&database, &data_key, upstream.base_url.as_str());
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let client = reqwest::Client::new();
        let response = client
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                started.port
            ))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({
                "model": "gpt-test",
                "messages": [{"role": "user", "content": "ping"}],
                "stream": false,
            }))
            .send()
            .await
            .expect("send v2 chat");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("chat json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "chatcmpl-v2");
        upstream.wait_for_requests(1);
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "success");
        assert_eq!(logs[0].path, "/v1/chat/completions");
    }

    #[tokio::test]
    async fn v2_streaming_request_lease_survives_handler_return_until_body_drop() {
        let release = Arc::new(AtomicBool::new(false));
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::PausedSse {
            first_chunk: b"data: {\"choices\":[{\"delta\":{\"content\":\"hold\"}}]}\n\n".to_vec(),
            release: Arc::clone(&release),
        }]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate(&database, &data_key, upstream.base_url.as_str());
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                started.port
            ))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({
                "model": "gpt-test",
                "messages": [{"role": "user", "content": "ping"}],
                "stream": true,
            }))
            .send()
            .await
            .expect("send streaming chat");
        assert_eq!(response.status(), StatusCode::OK);
        upstream.wait_for_requests(1);
        wait_runtime_active_requests(&runtime, started.port, 1).await;

        drop(response);
        release.store(true, AtomicOrdering::Relaxed);
        wait_runtime_active_requests(&runtime, started.port, 0).await;
        runtime.stop(started.port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_request_log_preserves_nested_reasoning_effort() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"chatcmpl-reasoning","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_vec(),
        )]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate(&database, &data_key, upstream.base_url.as_str());
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                started.port
            ))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({
                "model": "gpt-test",
                "messages": [{"role": "user", "content": "ping"}],
                "reasoning": {"effort": "high"},
                "stream": false,
            }))
            .send()
            .await
            .expect("send v2 reasoning request");
        assert_eq!(response.status(), StatusCode::OK);
        response.bytes().await.expect("consume response");
        runtime.stop(started.port).await.unwrap();

        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].reasoning_effort.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn v2_buffered_usage_returns_local_balance_summary_without_upstream() {
        let upstream = LoopbackUpstream::script(Vec::new());
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let seeded = seed_v2_candidate(&database, &data_key, upstream.base_url.as_str());
        seed_v2_balance(
            &database,
            &seeded.station_id,
            "usage-old",
            4.0,
            "low",
            "1000",
        );
        seed_v2_balance(
            &database,
            &seeded.station_id,
            "usage-new",
            12.5,
            "normal",
            "2000",
        );
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/v1/usage", started.port))
            .bearer_auth("relay-local-secret")
            .send()
            .await
            .expect("send usage");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("usage json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["remaining"], 12.5);
        assert_eq!(body["stations"], 1);
        assert_eq!(upstream.captured_count(), 0);
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].path, "/v1/usage");
    }

    #[tokio::test]
    async fn v2_buffered_models_aggregates_and_deduplicates_upstreams() {
        let upstream = LoopbackUpstream::script(vec![
            ScriptedResponse::Json(
                br#"{"object":"list","data":[{"id":"gpt-a","object":"model"},{"id":"shared","object":"model"}]}"#.to_vec(),
            ),
            ScriptedResponse::Json(
                br#"{"object":"list","data":[{"id":"shared","object":"model"},{"id":"gpt-b","object":"model"}]}"#.to_vec(),
            ),
        ]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "models-a",
            0,
            "auto",
        );
        seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "models-b",
            1,
            "auto",
        );
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/v1/models", started.port))
            .bearer_auth("relay-local-secret")
            .send()
            .await
            .expect("send models");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("models json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        let ids = body["data"]
            .as_array()
            .expect("model data")
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect::<Vec<_>>();
        assert_eq!(ids, vec!["gpt-a", "shared", "gpt-b"]);
        upstream.wait_for_requests(2);
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].path, "/v1/models");
    }

    #[tokio::test]
    async fn v2_buffered_alias_rewrites_model_and_falls_back_before_output() {
        let upstream = LoopbackUpstream::script(vec![
            ScriptedResponse::Status {
                status: 429,
                reason: "Too Many Requests",
            },
            ScriptedResponse::Json(
                br#"{"id":"chatcmpl-v2-fallback","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}]}"#.to_vec(),
            ),
        ]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        database
            .upsert_model_alias(UpsertModelAliasInput {
                id: None,
                client_model: "alias-model".to_string(),
                upstream_model: "mapped-model".to_string(),
                enabled: true,
                note: None,
            })
            .expect("model alias");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "first",
            0,
            "auto",
        );
        seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "second",
            1,
            "auto",
        );
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                started.port
            ))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({
                "model": "alias-model",
                "messages": [{"role": "user", "content": "ping"}],
            }))
            .send()
            .await
            .expect("send chat");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("chat json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "chatcmpl-v2-fallback");
        upstream.wait_for_requests(2);
        let captured = upstream.captured_requests();
        assert_eq!(captured[0].path_and_query, "/v1/chat/completions");
        assert_eq!(captured[1].path_and_query, "/v1/chat/completions");
        assert_eq!(
            captured[1].header("authorization"),
            Some("Bearer sk-v2-second")
        );
        let upstream_body: serde_json::Value =
            serde_json::from_slice(&captured[1].body).expect("upstream body");
        assert_eq!(upstream_body["model"], "mapped-model");
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].fallback_count, 1);
    }

    #[tokio::test]
    async fn v2_uses_persisted_stable_first_routing_strategy() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"chatcmpl-stable","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}]}"#.to_vec(),
        )]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let flaky = seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "flaky",
            0,
            "auto",
        );
        let stable = seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "stable",
            1,
            "auto",
        );
        {
            let connection = database
                .connection_for_repository_tests()
                .expect("test connection");
            connection
                .execute(
                    "UPDATE settings SET value = 'stable_first' WHERE key = 'default_routing_strategy'",
                    [],
                )
                .expect("routing strategy");
            connection
                .execute(
                    "INSERT INTO station_key_health (
                        station_key_id, consecutive_failures, success_count, failure_count,
                        total_duration_ms, avg_latency_ms, updated_at, endpoint_revision
                     ) VALUES (?1, 2, 1, 2, 16000, 8000, '1000', 1)",
                    rusqlite::params![&flaky.station_key_id],
                )
                .expect("flaky health");
            connection
                .execute(
                    "INSERT INTO station_key_health (
                        station_key_id, consecutive_failures, success_count, failure_count,
                        total_duration_ms, avg_latency_ms, updated_at, endpoint_revision
                     ) VALUES (?1, 0, 100, 0, 8000, 80, '1000', 1)",
                    rusqlite::params![&stable.station_key_id],
                )
                .expect("stable health");
        }
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .post(format!(
                "http://127.0.0.1:{}/v1/chat/completions",
                started.port
            ))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({
                "model": "gpt-test",
                "messages": [{"role": "user", "content": "ping"}],
            }))
            .send()
            .await
            .expect("send chat");
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.bytes().await.expect("response body");
        runtime.stop(started.port).await.unwrap();

        upstream.wait_for_requests(1);
        assert_eq!(
            upstream.captured_requests()[0].header("authorization"),
            Some("Bearer sk-v2-stable")
        );
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].route_policy.as_deref(), Some("stable_first"));
    }

    #[tokio::test]
    async fn v2_connect_failure_falls_back_to_next_candidate_before_output() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"resp-fallback","output_text":"ok"}"#.to_vec(),
        )]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate_named(
            &database,
            &data_key,
            "http://127.0.0.1:9",
            "offline",
            0,
            "auto",
        );
        seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "ready",
            1,
            "auto",
        );
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send responses");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("responses json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "resp-fallback");
        upstream.wait_for_requests(1);
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].fallback_count, 1);
    }

    #[tokio::test]
    async fn v2_precommit_failure_finalizes_request_log_and_key_health() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Status {
            status: 502,
            reason: "Bad Gateway",
        }]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        let seeded = seed_v2_candidate(&database, &data_key, upstream.base_url.as_str());
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send responses");
        assert_eq!(response.status(), StatusCode::BAD_GATEWAY);
        let failure_body: serde_json::Value = response.json().await.expect("failure json");
        assert_eq!(failure_body["error"]["code"], "upstream_http_error");
        assert_eq!(failure_body["error"]["message"], "upstream HTTP 502");
        runtime.stop(started.port).await.unwrap();

        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 1, "failed v2 requests must be observable");
        assert_eq!(logs[0].status, "failed");
        assert_eq!(logs[0].failure_source.as_deref(), Some("upstream"));
        assert_eq!(logs[0].attempt_count, Some(1));
        let health = database
            .get_station_key_health(seeded.station_key_id)
            .expect("key health");
        assert_eq!(health.failure_count, 1);
        assert_eq!(health.consecutive_failures, 1);
    }

    #[tokio::test]
    async fn v2_honors_configured_precommit_timeout() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::DelayedHeaders {
            delay: Duration::from_secs(1),
            body: br#"{"id":"too-late"}"#.to_vec(),
        }]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate(&database, &data_key, upstream.base_url.as_str());
        let runtime = ProxyRuntimeState::for_tests();
        let mut config = ProxyStartConfig::new(database, data_key, 0);
        config.limits.precommit_timeout = Duration::from_millis(50);
        let started = runtime.start(config).await.expect("start v2");

        let request_started = std::time::Instant::now();
        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send responses");
        let elapsed = request_started.elapsed();
        runtime.stop(started.port).await.unwrap();

        assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
        assert!(
            elapsed < Duration::from_millis(500),
            "configured precommit timeout was ignored: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn v2_buffered_responses_bridge_and_embeddings_use_real_upstream() {
        let upstream = LoopbackUpstream::script(vec![
            ScriptedResponse::Json(
                br#"{"id":"chatcmpl-bridge","choices":[{"message":{"role":"assistant","content":"bridged"},"finish_reason":"stop","index":0}],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}"#.to_vec(),
            ),
            ScriptedResponse::Json(
                br#"{"object":"list","data":[{"embedding":[0.1],"index":0}],"usage":{"prompt_tokens":1,"total_tokens":1}}"#.to_vec(),
            ),
        ]);
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        let data_key = crate::services::secrets::crypto::generate_data_key();
        seed_v2_candidate_named(
            &database,
            &data_key,
            upstream.base_url.as_str(),
            "bridge",
            0,
            "openai_chat_completions",
        );
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(ProxyStartConfig::new(database.clone(), data_key, 0))
            .await
            .expect("start v2");
        let client = reqwest::Client::new();

        let responses = client
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send responses");
        let responses_status = responses.status();
        let responses_body: serde_json::Value = responses.json().await.expect("responses json");
        let embeddings = client
            .post(format!("http://127.0.0.1:{}/v1/embeddings", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send embeddings");
        let embeddings_status = embeddings.status();
        let embeddings_body: serde_json::Value = embeddings.json().await.expect("embeddings json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(responses_status, StatusCode::OK);
        assert_eq!(responses_body["object"], "response");
        assert_eq!(responses_body["output_text"], "bridged");
        assert_eq!(embeddings_status, StatusCode::OK);
        assert_eq!(embeddings_body["object"], "list");
        upstream.wait_for_requests(2);
        let captured = upstream.captured_requests();
        assert_eq!(captured[0].path_and_query, "/v1/chat/completions");
        assert_eq!(captured[1].path_and_query, "/v1/embeddings");
        let logs = database.list_local_proxy_request_logs().expect("logs");
        assert_eq!(logs.len(), 2);
    }

    #[tokio::test]
    async fn v2_runtime_65th_raw_connection_closes_without_http_response() {
        let runtime = ProxyRuntimeState::for_tests();
        let mut config = test_start_config(0);
        config.limits.max_connections = 1;
        let started = runtime.start(config).await.unwrap();
        let _held = tokio::net::TcpStream::connect(("127.0.0.1", started.port))
            .await
            .unwrap();

        let mut rejected = tokio::net::TcpStream::connect(("127.0.0.1", started.port))
            .await
            .unwrap();
        let mut buffer = [0_u8; 1];
        let read = tokio::time::timeout(Duration::from_secs(1), rejected.read(&mut buffer))
            .await
            .expect("rejected connection closes")
            .expect("read rejected connection");

        assert_eq!(read, 0);
        runtime.stop(started.port).await.unwrap();
    }

    fn test_start_config(port: u16) -> ProxyStartConfig {
        let database = AppDatabase::new_temp_file_for_tests("runtime").expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        ProxyStartConfig::new(
            database,
            crate::services::secrets::crypto::generate_data_key(),
            port,
        )
    }

    struct SeededV2Candidate {
        station_id: String,
        station_key_id: String,
    }

    fn seed_v2_candidate(
        database: &AppDatabase,
        data_key: &[u8; 32],
        upstream_base_url: &str,
    ) -> SeededV2Candidate {
        seed_v2_candidate_named(database, data_key, upstream_base_url, "upstream", 0, "auto")
    }

    fn seed_v2_candidate_named(
        database: &AppDatabase,
        data_key: &[u8; 32],
        upstream_base_url: &str,
        suffix: &str,
        priority: i64,
        upstream_api_format: &str,
    ) -> SeededV2Candidate {
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: format!("V2 buffered station {suffix}"),
                    station_type: "openai-compatible".to_string(),
                    website_url: upstream_base_url.to_string(),
                    api_base_url: format!("{}/v1", upstream_base_url.trim_end_matches('/')),
                    api_key: format!("sk-v2-{suffix}"),
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
        {
            let connection = database
                .connection_for_repository_tests()
                .expect("test connection");
            connection
                .execute(
                    "UPDATE stations SET upstream_api_format = ?1 WHERE id = ?2",
                    rusqlite::params![upstream_api_format, &station.id],
                )
                .expect("upstream api format");
        }
        let key = database
            .list_station_keys(station.id.clone())
            .expect("station keys")
            .into_iter()
            .next()
            .expect("station key");
        {
            let connection = database
                .connection_for_repository_tests()
                .expect("test connection");
            connection
                .execute(
                    "UPDATE station_keys SET priority = ?1, routing_order = ?1 WHERE id = ?2",
                    rusqlite::params![priority, &key.id],
                )
                .expect("station key priority");
        }
        database
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
            .expect("capabilities");
        SeededV2Candidate {
            station_id: station.id,
            station_key_id: key.id,
        }
    }

    fn seed_v2_balance(
        database: &AppDatabase,
        station_id: &str,
        id: &str,
        value: f64,
        status: &str,
        collected_at: &str,
    ) {
        database
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
            .expect("balance snapshot");
    }

    async fn next_free_port() -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }

    async fn wait_runtime_active_requests(runtime: &ProxyRuntimeState, port: u16, expected: u32) {
        for _ in 0..100 {
            if runtime.status(port).active_requests == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(runtime.status(port).active_requests, expected);
    }
}
