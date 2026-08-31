use std::{
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use crate::{
    application::credentials::ExecutionCredentialResolver,
    application::queries::routing_runtime::RoutingRuntimeActivity,
    models::proxy::{ProxyLifecycle, ProxyStatus},
    observability::correlation,
    services::{
        proxy::{
            execution::{ExecutionEngine, UpstreamAttemptExecutor},
            finalization::FinalizationOutcome,
            ingress::{self, IngressExecutor, IngressState},
            lifecycle::{
                delivery::DeliveryTerminal,
                ports::RequestLifecycleStore,
                request::PendingFinalRequestRecord,
                writer::{LifecycleWriter, LifecycleWriterWorker, WriterAdmissionError},
            },
            limits::ProxyStartupResourceLimits,
            request::{ProxyHttpResponse, ProxyResponsePayload},
            response_body::{
                dual_terminal_buffered_lifecycle_finalizing_stream_with_capacity_lease,
                dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory_and_capacity_lease,
            },
            routing_repository::RoutingRepository,
            server::{self, RunningServer},
            transport_policy::{TransportPolicySnapshot, TransportPolicyStore},
            upstream::UpstreamClientPool,
        },
        time::now_millis_for_services,
    },
};
use futures_util::future::BoxFuture;

#[derive(Clone)]
pub struct ProxyStartConfig {
    pub(crate) routing_repository: Arc<dyn RoutingRepository>,
    pub(crate) credential_resolver: Arc<dyn ExecutionCredentialResolver>,
    pub(crate) lifecycle_store: Arc<dyn RequestLifecycleStore>,
    pub(crate) local_access_key: String,
    pub(crate) port: u16,
    pub(crate) limits: ProxyStartupResourceLimits,
    pub(crate) transport_policy: TransportPolicySnapshot,
}

impl ProxyStartConfig {
    pub(crate) fn new_v2(
        routing_repository: Arc<dyn RoutingRepository>,
        credential_resolver: Arc<dyn ExecutionCredentialResolver>,
        lifecycle_store: Arc<dyn RequestLifecycleStore>,
        local_access_key: String,
        port: u16,
    ) -> Self {
        Self {
            routing_repository,
            credential_resolver,
            lifecycle_store,
            local_access_key,
            port,
            limits: ProxyStartupResourceLimits::default(),
            transport_policy: TransportPolicySnapshot::default(),
        }
    }

    pub(crate) fn with_transport_policy(
        mut self,
        transport_policy: TransportPolicySnapshot,
    ) -> Self {
        self.transport_policy = transport_policy;
        self
    }
}

pub struct ProxyRuntimeState {
    v2: tokio::sync::Mutex<V2RuntimeInner>,
    lifecycle_operation: tokio::sync::Mutex<()>,
    status_snapshot: RwLock<ProxyStatus>,
    effective_limits: RwLock<ProxyStartupResourceLimits>,
    transport_policy_store: TransportPolicyStore,
}

impl RoutingRuntimeActivity for ProxyRuntimeState {
    fn active_for_station<'a>(
        &'a self,
        station_type: &'a str,
        station_id: &'a str,
        station_key_id: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Option<i64>> {
        Box::pin(async move {
            ProxyRuntimeState::active_for_station(self, station_type, station_id, station_key_id)
                .await
        })
    }

    fn active_for_station_key<'a>(
        &'a self,
        station_key_id: &'a str,
    ) -> futures_util::future::BoxFuture<'a, Option<i64>> {
        Box::pin(
            async move { ProxyRuntimeState::active_for_station_key(self, station_key_id).await },
        )
    }
}

impl Default for ProxyRuntimeState {
    fn default() -> Self {
        Self {
            v2: tokio::sync::Mutex::new(V2RuntimeInner::default()),
            lifecycle_operation: tokio::sync::Mutex::new(()),
            status_snapshot: RwLock::new(default_status(0)),
            effective_limits: RwLock::new(ProxyStartupResourceLimits::default()),
            transport_policy_store: TransportPolicyStore::default(),
        }
    }
}

impl ProxyRuntimeState {
    pub(crate) fn transport_policy_snapshot(&self) -> Arc<TransportPolicySnapshot> {
        self.transport_policy_store.load()
    }

    pub(crate) async fn publish_transport_policy(
        &self,
        snapshot: TransportPolicySnapshot,
    ) -> Result<bool, String> {
        let _operation = self.lifecycle_operation.lock().await;
        self.transport_policy_store
            .publish_if_newer(snapshot)
            .map_err(|error| format!("publish transport policy failed: {error:?}"))
    }

    pub(crate) async fn active_for_station(
        &self,
        station_type: &str,
        station_id: &str,
        station_key_id: &str,
    ) -> Option<i64> {
        let inner = self.v2.lock().await;
        Some(
            inner
                .routing_runtime
                .as_ref()
                .map(|runtime| runtime.active_for_station(station_type, station_id, station_key_id))
                .unwrap_or(0),
        )
    }

    pub(crate) async fn active_for_station_key(&self, station_key_id: &str) -> Option<i64> {
        let inner = self.v2.lock().await;
        Some(
            inner
                .routing_runtime
                .as_ref()
                .map(|runtime| runtime.active_for_station_key(station_key_id))
                .unwrap_or(0),
        )
    }
    #[cfg(test)]
    pub(crate) fn for_tests() -> Self {
        Self::default()
    }

    pub(crate) async fn decision_trace_for_request(
        &self,
        request_id: &str,
    ) -> Option<crate::observability::decision_trace::RequestDecisionTraceV1> {
        let inner = self.v2.lock().await;
        inner.routing_runtime.as_ref().and_then(|runtime| {
            runtime
                .decision_trace_snapshot()
                .into_iter()
                .find(|trace| trace.request_id == request_id)
        })
    }

    #[cfg(test)]
    pub(crate) async fn decision_traces(
        &self,
    ) -> Vec<crate::observability::decision_trace::RequestDecisionTraceV1> {
        let inner = self.v2.lock().await;
        inner
            .routing_runtime
            .as_ref()
            .map(|runtime| runtime.decision_trace_snapshot())
            .unwrap_or_default()
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

    pub async fn drain_for_data_maintenance(
        &self,
        timeout: Duration,
    ) -> Result<ProxyStatus, String> {
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
        let lifecycle_store = Arc::clone(&config.lifecycle_store);
        self.v2_start_with_lifecycle_store(config, lifecycle_store)
            .await
    }

    async fn v2_start_with_lifecycle_store(
        &self,
        config: ProxyStartConfig,
        lifecycle_store: Arc<dyn RequestLifecycleStore>,
    ) -> Result<ProxyStatus, String> {
        crate::observability::runtime::bootstrap::emit(
            crate::services::proxy::runtime_events::lifecycle_start_started(),
        );
        let _operation = self.lifecycle_operation.lock().await;
        *self
            .effective_limits
            .write()
            .unwrap_or_else(|error| error.into_inner()) = config.limits.clone();
        {
            let inner = self.v2.lock().await;
            if let Some(server) = inner.server.as_ref() {
                if server.local_addr.port() == config.port || config.port == 0 {
                    crate::observability::runtime::bootstrap::emit(
                        crate::services::proxy::runtime_events::lifecycle_already_running(),
                    );
                    return Ok(self.v2_status_from_inner(&inner, server.local_addr.port()));
                }
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::lifecycle_start_failed(),
                );
                return Err(format!(
                    "local proxy is already running on port {}; stop it before starting port {}",
                    server.local_addr.port(),
                    config.port
                ));
            }
        }

        self.transport_policy_store
            .install(config.transport_policy.clone())
            .map_err(|error| format!("invalid transport execution policy: {error:?}"))?;

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

        let local_access_key = config.local_access_key.clone();
        let runtime_max_concurrency = config.limits.max_in_flight_requests as u32;
        // Compose the proxy-instance routing owner before the execution
        // engine. Every request and every planner snapshot must observe this
        // exact runtime identity; creating it after the engine leaves a
        // production chain with an unowned mutable overlay.
        let routing_runtime = Arc::new(super::routing_runtime::RoutingRuntimeState::new(
            runtime_max_concurrency,
            runtime_max_concurrency / 20,
        ));

        let active_requests = Arc::new(AtomicU32::new(0));
        let request_count = Arc::new(AtomicU64::new(0));
        let repository = Arc::clone(&config.routing_repository);
        let credential_resolver = Arc::clone(&config.credential_resolver);
        let transport_policy = self.transport_policy_store.load();
        let upstream_pool = UpstreamClientPool::new_with_transport_policy(
            (*transport_policy).clone(),
            config.limits.max_buffered_body_bytes,
            routing_runtime.diagnostic_memory_budget(),
        )
        .map_err(|failure| {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::lifecycle_start_failed(),
            );
            let message = failure.public_message.clone();
            let failed = failed_status(config.port, message.clone());
            self.publish_status(failed);
            message
        })?;
        let (lifecycle_writer, lifecycle_worker) =
            LifecycleWriter::start(lifecycle_writer_capacity(&config.limits), lifecycle_store)
                .map_err(|error| {
                    crate::observability::runtime::bootstrap::emit(
                        crate::services::proxy::runtime_events::lifecycle_start_failed(),
                    );
                    let message = format!("start lifecycle writer failed: {error:?}");
                    let failed = failed_status(config.port, message.clone());
                    self.publish_status(failed);
                    message
                })?;
        let executor = Arc::new(ProxyExecutor::new(
            repository,
            credential_resolver,
            upstream_pool,
            (*transport_policy).clone(),
            lifecycle_writer.clone(),
            Arc::clone(&routing_runtime),
        ));
        let ingress_state = Arc::new(IngressState::with_active_requests_and_policy(
            local_access_key,
            config.limits.clone(),
            executor,
            Arc::clone(&active_requests),
            Arc::clone(&request_count),
            Some(lifecycle_writer),
            self.transport_policy_store.clone(),
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
                inner.routing_runtime = Some(routing_runtime);
                self.publish_status(started.clone());
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::lifecycle_start_succeeded(),
                );
                Ok(started)
            }
            Err(error) => {
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::lifecycle_start_failed(),
                );
                let error = match lifecycle_worker.join().await {
                    Ok(()) => error,
                    Err(_) => format!(
                        "{error}; lifecycle writer task failed while rolling back proxy startup"
                    ),
                };
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
            inner.routing_runtime.take();
            let Some(server) = inner.server.take() else {
                let stopped = default_status(default_port);
                self.publish_status(stopped.clone());
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::lifecycle_stop_succeeded(),
                );
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
        let worker = self.v2.lock().await.lifecycle_worker.take();
        let worker_result = match worker {
            Some(worker) => worker
                .join()
                .await
                .map_err(|_| "lifecycle writer task failed during proxy stop".to_string()),
            None => Ok(()),
        };
        let stopped = combined_shutdown_status(port, stop_result, worker_result);
        self.publish_status(stopped.clone());
        if stopped.lifecycle == ProxyLifecycle::Failed {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::lifecycle_stop_failed(),
            );
            Err(stopped
                .last_error
                .clone()
                .unwrap_or_else(|| "proxy stop failed".to_string()))
        } else {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::lifecycle_stop_succeeded(),
            );
            Ok(stopped)
        }
    }

    async fn v2_prepare_for_update(&self, timeout: Duration) -> Result<ProxyStatus, String> {
        let _operation = self.lifecycle_operation.lock().await;
        let server = {
            let mut inner = self.v2.lock().await;
            inner.routing_runtime.take();
            let Some(server) = inner.server.take() else {
                let stopped = default_status(0);
                self.publish_status(stopped.clone());
                crate::observability::runtime::bootstrap::emit(
                    crate::services::proxy::runtime_events::lifecycle_drain_succeeded(),
                );
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
        if stop_result
            .as_ref()
            .err()
            .map(|error| error == "proxy server shutdown timed out")
            .unwrap_or(false)
        {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::lifecycle_drain_timeout(),
            );
        }
        let worker = self.v2.lock().await.lifecycle_worker.take();
        let worker_result = match worker {
            Some(worker) => worker
                .join()
                .await
                .map_err(|_| "lifecycle writer task failed during proxy drain".to_string()),
            None => Ok(()),
        };
        let stopped = combined_shutdown_status(port, stop_result, worker_result);
        self.publish_status(stopped.clone());
        if stopped.lifecycle == ProxyLifecycle::Failed {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::lifecycle_drain_failed(),
            );
            Err(stopped
                .last_error
                .clone()
                .unwrap_or_else(|| "proxy drain failed".to_string()))
        } else {
            crate::observability::runtime::bootstrap::emit(
                crate::services::proxy::runtime_events::lifecycle_drain_succeeded(),
            );
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
    routing_runtime: Option<Arc<super::routing_runtime::RoutingRuntimeState>>,
}

struct ProxyExecutor {
    engine: ExecutionEngine,
    lifecycle_writer: LifecycleWriter,
}

impl ProxyExecutor {
    fn new(
        repository: Arc<dyn RoutingRepository>,
        credential_resolver: Arc<dyn ExecutionCredentialResolver>,
        upstream_pool: UpstreamClientPool,
        transport_policy: TransportPolicySnapshot,
        lifecycle_writer: LifecycleWriter,
        routing_runtime: Arc<super::routing_runtime::RoutingRuntimeState>,
    ) -> Self {
        let attempts = Arc::new(UpstreamAttemptExecutor::new(upstream_pool));
        Self {
            engine: ExecutionEngine::new_with_transport_policy_and_lifecycle(
                repository,
                credential_resolver,
                attempts,
                transport_policy,
                lifecycle_writer.clone(),
                routing_runtime,
            ),
            lifecycle_writer,
        }
    }
}

impl IngressExecutor for ProxyExecutor {
    fn execute(
        &self,
        mut request: super::request::CanonicalProxyRequest,
    ) -> BoxFuture<'static, Result<ProxyHttpResponse, super::error::ProxyFailure>> {
        let proxy_correlation = correlation::CorrelationId::for_proxy_request(&request.request_id);
        let lifecycle_writer = self.lifecycle_writer.clone();
        let engine = self.engine.clone();
        let stream_idle_timeout = request.transport_policy().stream_idle_timeout;
        let Some(admission) = request.take_lifecycle_admission() else {
            return Box::pin(correlation::in_scope(
                "proxy.request",
                proxy_correlation,
                async move {
                    Err(lifecycle_unavailable_failure(
                        "missing lifecycle admission for v2 request",
                    ))
                },
            ));
        };
        let Some(request_lease) = request.take_request_lease() else {
            return Box::pin(correlation::in_scope(
                "proxy.request",
                proxy_correlation,
                async move {
                    Err(lifecycle_unavailable_failure(
                        "missing request lease for v2 request",
                    ))
                },
            ));
        };
        let request_context = admission.context;
        let mut request_terminal = Some(admission.terminal);
        let mut request_lease = Some(request_lease);
        let request_model = request.model.clone();
        let request_stream = request.stream;
        let request_reasoning_effort = request.reasoning_effort.clone();
        Box::pin(correlation::in_scope(
            "proxy.request",
            proxy_correlation,
            async move {
                let response = match engine.execute(request).await {
                    Ok(response) => response,
                    Err(failure) => {
                        let request_id = request_context.request_id.clone();
                        let attempt_count = failure.attempt_count().unwrap_or_else(|| {
                            if failure.candidate_id().is_some() {
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
                                http_status: Some(failure.http_status.as_u16()),
                                selected_station_key_id: failure.candidate_id().map(str::to_owned),
                                selected_station_id: failure
                                    .candidate_station_id()
                                    .map(str::to_owned),
                                upstream_base_url: failure
                                    .candidate_upstream_base_url()
                                    .map(str::to_owned),
                                route_policy: failure.route_policy().map(str::to_owned),
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
                                billing_mode: None,
                            };
                        let mut pending_record = PendingFinalRequestRecord::new(
                            request_context.clone(),
                            failure.candidate_id().map(|_| {
                                crate::services::proxy::lifecycle::request::AttemptId::new(
                                    request_id,
                                    fallback_count,
                                )
                            }),
                            attempt_count,
                            fallback_count,
                            annotations,
                        );
                        if let Some(outcome) = failure.routing_outcome_facts() {
                            pending_record.set_routing_outcome(outcome);
                        }
                        let outcome = FinalizationOutcome::Failed {
                            code: failure.code.as_str().to_string(),
                            detail: Some(failure.public_message.clone()),
                        };
                        let join = super::attempt::DualTerminalFinalizationLease::new(
                            super::attempt::DownstreamRequestFinalizationLease::new(
                                request_terminal
                                    .take()
                                    .expect("request terminal reservation available"),
                                request_lease.take().expect("request lease available"),
                            ),
                            None,
                            None,
                        )
                        .finalize(
                            pending_record,
                            DeliveryTerminal::NotStarted,
                            outcome,
                            None,
                            false,
                        );
                        // The HTTP error is returned only after the durable
                        // request terminal has been handed to the lifecycle
                        // writer. Otherwise callers can observe an
                        // `in_progress` row immediately after a precommit
                        // failure, and restart recovery must guess whether
                        // finalization was ever scheduled.
                        if let Some(join) = join {
                            let _ = join.await;
                        }
                        return Err(failure);
                    }
                };
                let status = response.status;
                let headers = response.headers;
                let capacity_lease = response.capacity_lease;
                let mut lifecycle = response.lifecycle;
                lifecycle.annotations.http_status = Some(status.as_u16());
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
                let selected_attempt = match dual_selected_attempt_finalization(
                    &lifecycle_writer,
                    lifecycle.selected_attempt.as_ref(),
                ) {
                    Ok(selected_attempt) => selected_attempt,
                    Err(failure) => {
                        finalize_lifecycle_admission_failure(
                            request_terminal
                                .take()
                                .expect("request terminal reservation available"),
                            request_lease.take().expect("request lease available"),
                            pending_record,
                            &failure,
                        );
                        return Err(failure);
                    }
                };
                let costs = match dual_cost_finalization(
                    &lifecycle_writer,
                    lifecycle.attempt_count,
                    lifecycle.selected_attempt_cost,
                ) {
                    Ok(costs) => costs,
                    Err(failure) => {
                        finalize_lifecycle_admission_failure(
                            request_terminal
                                .take()
                                .expect("request terminal reservation available"),
                            request_lease.take().expect("request lease available"),
                            pending_record,
                            &failure,
                        );
                        return Err(failure);
                    }
                };
                let payload = match response.body {
                    super::execution::ProxyExecutionBody::Buffered(body) => {
                        ProxyResponsePayload::Stream(
                            dual_terminal_buffered_lifecycle_finalizing_stream_with_capacity_lease(
                                body,
                                pending_record,
                                request_terminal
                                    .take()
                                    .expect("request terminal reservation available"),
                                selected_attempt,
                                Some(costs),
                                request_lease.take().expect("request lease available"),
                                capacity_lease,
                            ),
                        )
                    }
                    super::execution::ProxyExecutionBody::Stream {
                        chunks,
                        diagnostic_memory,
                    } => {
                        ProxyResponsePayload::Stream(
                            dual_terminal_lifecycle_finalizing_stream_with_idle_timeout_and_diagnostic_memory_and_capacity_lease(
                                chunks,
                                pending_record,
                                request_terminal
                                    .take()
                                    .expect("request terminal reservation available"),
                                selected_attempt,
                                Some(costs),
                                request_lease.take().expect("request lease available"),
                                capacity_lease,
                                stream_idle_timeout,
                                diagnostic_memory,
                            ),
                        )
                    }
                };
                Ok(ProxyHttpResponse {
                    status,
                    headers,
                    payload,
                })
            },
        ))
    }
}

fn finalize_lifecycle_admission_failure(
    terminal: crate::services::proxy::lifecycle::writer::RequestTerminalReservation,
    request_lease: crate::services::proxy::limits::RequestLease,
    mut record: PendingFinalRequestRecord,
    failure: &super::error::ProxyFailure,
) {
    record.annotations_mut().http_status = Some(failure.http_status.as_u16());
    record.annotations_mut().failure_source = Some(failure.source.as_str().to_string());
    record.annotations_mut().completion_source = Some("lifecycle_admission_failure".to_string());
    let outcome = FinalizationOutcome::Failed {
        code: failure.code.as_str().to_string(),
        detail: Some(failure.public_message.clone()),
    };
    let _ = super::attempt::DualTerminalFinalizationLease::new(
        super::attempt::DownstreamRequestFinalizationLease::new(terminal, request_lease),
        None,
        None,
    )
    .finalize(record, DeliveryTerminal::NotStarted, outcome, None, false);
}

fn dual_selected_attempt_finalization(
    lifecycle_writer: &LifecycleWriter,
    selected_attempt: Option<&crate::services::proxy::lifecycle::attempt::AttemptContext>,
) -> Result<
    Option<(
        crate::services::proxy::lifecycle::writer::AttemptWriteReservation,
        crate::services::proxy::lifecycle::attempt::AttemptContext,
    )>,
    super::error::ProxyFailure,
> {
    selected_attempt
        .map(|selected_attempt| {
            Ok((
                lifecycle_writer
                    .try_reserve_attempt()
                    .map_err(lifecycle_admission_failure)?,
                selected_attempt.clone(),
            ))
        })
        .transpose()
}

fn dual_cost_finalization(
    lifecycle_writer: &LifecycleWriter,
    attempt_count: u16,
    selected_attempt_cost: Option<crate::services::proxy::attempt::SelectedAttemptCostSnapshot>,
) -> Result<crate::services::proxy::attempt::CostFinalizationReservations, super::error::ProxyFailure>
{
    let mut attempt_costs = Vec::with_capacity(usize::from(attempt_count));
    for ordinal in 0..attempt_count {
        attempt_costs.push((
            ordinal,
            lifecycle_writer
                .try_reserve_attempt_cost()
                .map_err(lifecycle_admission_failure)?,
        ));
    }
    let aggregate = lifecycle_writer
        .try_reserve_request_cost_aggregate()
        .map_err(lifecycle_admission_failure)?;
    Ok(
        crate::services::proxy::attempt::CostFinalizationReservations::new(
            attempt_costs,
            aggregate,
            selected_attempt_cost,
        ),
    )
}

fn lifecycle_writer_capacity(limits: &ProxyStartupResourceLimits) -> usize {
    limits
        .max_in_flight_requests
        .saturating_mul(4)
        .saturating_add(16)
        .max(8)
}

fn lifecycle_admission_failure(error: WriterAdmissionError) -> super::error::ProxyFailure {
    let mut failure = lifecycle_unavailable_failure("local proxy lifecycle writer unavailable");
    failure.internal_detail = Some(format!("lifecycle writer admission rejected: {error:?}"));
    failure
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

fn combined_shutdown_status(
    port: u16,
    server: Result<(), String>,
    lifecycle_writer: Result<(), String>,
) -> ProxyStatus {
    match (server, lifecycle_writer) {
        (Ok(()), Ok(())) => default_status(port),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => failed_status(port, error),
        (Err(server), Err(lifecycle_writer)) => {
            failed_status(port, format!("{server}; {lifecycle_writer}"))
        }
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
            Arc, Mutex,
        },
        time::Duration,
    };

    use futures_util::future::BoxFuture;
    use http::StatusCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use crate::{
        application::routing_execution_reader::RoutingExecutionReadError,
        application::{
            credentials::{
                ExecutionCredentialError, ExecutionCredentialResolver, SecretBytes, SecretRef,
            },
            observation_ingestion::ObservationIngestion,
            routing_generation::{
                canonical_json_sha256, policy_generation_id, ROUTING_GENERATION_ALGORITHM_VERSION,
            },
            routing_generation_coordinator::RoutingGenerationCoordinator,
        },
        background_tasks::routing_generation_cutover_runner::build_ready_once,
        models::{
            pricing::UpsertModelBasePriceInput,
            routing::RouteEndpointKind,
            routing_generation::RoutingGenerationQualification,
            routing_observation::{
                EventTimeStatus, ObservationOrder, ObservationOutcome, ObservationScope,
                ObservationSource, RoutingObservation, TrafficEquivalence,
            },
            routing_policy::RoutingPolicyConfigV3,
        },
        services::proxy::{
            lifecycle::{
                attempt::AttemptTerminalRecord,
                ports::{AttemptCommitAck, LifecycleWriteError, RequestCommitAck, RequestStartAck},
                request::{FinalRequestRecord, RequestContextSnapshot, RequestStartRecord},
            },
            limits::{BodyBudget, RequestLease},
            request::{CanonicalProxyRequest, RequestLifecycleAdmission, RequestRequirements},
            routing_repository::OperationalRouteSnapshot,
            test_support::{LoopbackUpstream, ScriptedResponse, V2ProxyTestFixture},
        },
    };

    use super::*;

    async fn seed_token_base_price(fixture: &V2ProxyTestFixture, model: &str) {
        fixture
            .services
            .pricing
            .upsert_model_base_price(UpsertModelBasePriceInput {
                id: Some(format!("test-price-{model}")),
                provider: "fixture".to_string(),
                model: model.to_string(),
                input_price: Some(5.0),
                output_price: Some(30.0),
                input_price_priority: None,
                output_price_priority: None,
                cache_creation_price: None,
                cache_creation_price_priority: None,
                cache_creation_price_above_1hr: None,
                cache_read_price: None,
                cache_read_price_priority: None,
                long_context_input_token_threshold: None,
                long_context_input_cost_multiplier: None,
                long_context_output_cost_multiplier: None,
                supports_service_tier: false,
                supports_prompt_caching: false,
                currency: "USD".to_string(),
                unit: "per_1m_tokens".to_string(),
                source_url: "https://fixture.invalid/pricing".to_string(),
                source_label: "fixture".to_string(),
                source_checked_at: Some("1".to_string()),
                enabled: true,
                built_in: false,
                note: None,
            })
            .await
            .expect("model base price");
    }

    async fn append_real_quality_samples(
        fixture: &V2ProxyTestFixture,
        station_id: &str,
        station_key_id: &str,
        outcome: ObservationOutcome,
        sample_prefix: &str,
    ) {
        let now_ms = chrono::Utc::now().timestamp_millis().max(0);
        let handle = fixture.runtime().handle();
        let lifecycle_revision = {
            let mut read = handle.begin_read().await.expect("quality lifecycle read");
            let revision: i64 = sqlx::query_scalar(
                "SELECT revision FROM domain_revisions WHERE scope = 'station_key:' || ?1",
            )
            .bind(station_key_id)
            .fetch_one(read.connection())
            .await
            .expect("quality lifecycle revision");
            u64::try_from(revision).expect("positive lifecycle revision")
        };
        let mut write = handle.begin_write().await.expect("quality sample write");
        for index in 0_u64..15 {
            let sample_id = format!("{sample_prefix}-{index}");
            let event_at_ms = now_ms.saturating_sub((15 - index) as i64 * 100);
            ObservationIngestion::new()
                .append(
                    &mut write,
                    RoutingObservation {
                        id: sample_id.clone(),
                        order: ObservationOrder {
                            producer_id: format!("runtime-quality:{station_key_id}"),
                            producer_sequence: index + 1,
                            event_at_ms,
                            ingested_at_ms: event_at_ms,
                        },
                        scope: ObservationScope {
                            station_id: Some(station_id.to_string()),
                            station_key_id: Some(station_key_id.to_string()),
                            model: Some("gpt-test".to_string()),
                            endpoint_revision: Some(1),
                        },
                        comparability_key: None,
                        source: ObservationSource::RealRequest,
                        traffic_equivalence: TrafficEquivalence::ExactRequest,
                        outcome: outcome.clone(),
                        latency_ms: Some(100),
                        evidence_mass_basis_points: 10_000,
                        correlation_id: format!("{sample_id}-request"),
                        attempt_index: 0,
                        station_key_lifecycle_revision: lifecycle_revision,
                        cluster_finalized: true,
                        cluster_expected_attempt_count: 1,
                        boundary_crossed: true,
                        event_time_status: EventTimeStatus::Valid,
                        response_origin:
                            crate::models::routing_observation::ResponseOrigin::Upstream,
                        failure_code: None,
                        failure_attribution:
                            crate::models::routing_observation::FailureAttribution::Key,
                        recovery_origin: crate::models::routing_observation::RecoveryOrigin::Normal,
                        retry_disposition:
                            crate::models::routing_observation::ObservationRetryDisposition::End,
                        probe_state_revision: None,
                        probe_scope: None,
                    },
                )
                .await
                .expect("append quality sample");
        }
        write.commit().await.expect("commit quality samples");
    }

    fn runtime_quality_qualification(
        runtime_generation_id: &str,
        qualified_at_ms: i64,
    ) -> RoutingGenerationQualification {
        let (comparison_report, replay_report) =
            crate::models::routing_generation::test_activation_qualification_reports(
                runtime_generation_id,
            );
        RoutingGenerationQualification {
            runtime_generation_id: runtime_generation_id.to_string(),
            comparison_report_hash: canonical_json_sha256(&comparison_report)
                .expect("comparison report hash"),
            comparison_report,
            replay_report_hash: canonical_json_sha256(&replay_report).expect("replay report hash"),
            replay_report,
            qualified_at_ms,
        }
    }

    async fn stage_default_v3_policy(fixture: &V2ProxyTestFixture) {
        let policy = RoutingPolicyConfigV3::default();
        let policy_json = serde_json::to_value(&policy).expect("serialize v3 policy");
        let policy_hash = canonical_json_sha256(&policy_json).expect("v3 policy hash");
        let policy_generation_id = policy_generation_id(
            "active",
            1,
            "routing-policy-v3",
            &policy_hash,
            ROUTING_GENERATION_ALGORITHM_VERSION,
        )
        .expect("v3 policy generation id");
        let handle = fixture.runtime().handle();
        let mut write = handle.begin_write().await.expect("stage v3 policy write");
        sqlx::query("DELETE FROM routing_policy_v3_migration_audit")
            .execute(write.connection())
            .await
            .expect("clear staged policy audit fixture");
        sqlx::query("DELETE FROM routing_policy_v3_staged")
            .execute(write.connection())
            .await
            .expect("clear staged policy fixture");
        sqlx::query(
            "INSERT INTO routing_policy_v3_staged (
                 scope, source_config_revision, target_policy_revision,
                 config_revision, policy_generation_id, canonical_policy_hash,
                 policy_algorithm_version, source_policy_version, system_version,
                 target_policy_version, staged_policy_version, config_json,
                 status, created_at_ms, updated_at_ms
             ) VALUES (
                 'active', 1, 1, 1, ?1, ?2,
                 'routing-policy-v3', 'routing-policy-v3', 'routing-system-v1',
                 'routing-policy-v3', 'routing-policy-v3', ?3,
                 'staged', 1, 1
             )",
        )
        .bind(policy_generation_id)
        .bind(policy_hash)
        .bind(serde_json::to_string(&policy_json).expect("stored v3 policy JSON"))
        .execute(write.connection())
        .await
        .expect("insert staged v3 policy");
        write.commit().await.expect("commit staged v3 policy");
    }

    struct DropObservedStore {
        dropped: Arc<AtomicBool>,
    }

    impl Drop for DropObservedStore {
        fn drop(&mut self) {
            self.dropped.store(true, AtomicOrdering::Release);
        }
    }

    impl RequestLifecycleStore for DropObservedStore {
        fn start_request(
            &self,
            _record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            Box::pin(async { Ok(RequestStartAck { inserted: true }) })
        }

        fn finish_attempt(
            &self,
            _record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            Box::pin(async {
                Ok(AttemptCommitAck {
                    inserted: true,
                    health_applied: true,
                })
            })
        }

        fn finish_request(
            &self,
            _record: FinalRequestRecord,
        ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
            Box::pin(async { Ok(RequestCommitAck { finalized: true }) })
        }
    }

    struct PanicLifecycleStore;

    impl RequestLifecycleStore for PanicLifecycleStore {
        fn start_request(
            &self,
            _record: RequestStartRecord,
        ) -> BoxFuture<'static, Result<RequestStartAck, LifecycleWriteError>> {
            Box::pin(async { panic!("injected finalization worker panic") })
        }

        fn finish_attempt(
            &self,
            _record: AttemptTerminalRecord,
        ) -> BoxFuture<'static, Result<AttemptCommitAck, LifecycleWriteError>> {
            Box::pin(async { panic!("unexpected attempt write") })
        }

        fn finish_request(
            &self,
            _record: FinalRequestRecord,
        ) -> BoxFuture<'static, Result<RequestCommitAck, LifecycleWriteError>> {
            Box::pin(async { panic!("unexpected terminal write") })
        }
    }

    struct CorrelationCapturingRepository {
        captured: Arc<Mutex<Option<String>>>,
    }

    impl RoutingRepository for CorrelationCapturingRepository {
        fn load_execution_settings(
            &self,
        ) -> BoxFuture<
            'static,
            Result<crate::models::routing::RuntimeRoutingSettings, RoutingExecutionReadError>,
        > {
            Box::pin(async { Ok(crate::models::routing::RuntimeRoutingSettings::default()) })
        }

        fn load_balance_snapshots(
            &self,
        ) -> BoxFuture<
            'static,
            Result<Vec<crate::models::pricing::BalanceSnapshot>, RoutingExecutionReadError>,
        > {
            Box::pin(async { Ok(Vec::new()) })
        }

        fn load_planning_snapshot(
            &self,
            _request: crate::application::routing_engine::request::RouteRequestFacts,
            runtime: crate::application::routing_engine::planning_snapshot::RuntimeOverlaySnapshot,
            _context: crate::application::routing_engine::request::PlanningRequestContext,
        ) -> BoxFuture<
            'static,
            Result<
                Option<crate::application::routing_engine::planning_snapshot::PlanningSnapshot>,
                RoutingExecutionReadError,
            >,
        > {
            Box::pin(async move {
                Ok(Some(crate::application::routing_engine::planning_snapshot::PlanningSnapshot {
                snapshot_id: "correlation-test-planning-snapshot".to_string(),
                durable_revision: 1,
                configured_key_count: 0,
                capability_match_count: 0,
                candidate_cap_count: 0,
                routing_runtime_generation_id: None,
                routing_generation_fence_revision: 0,
                routing_policy_revision: 1,
                routing_quality_revision: 0,
                routing_health_revision: 0,
                quality_projection_backlog: 0,
                quality_projection_lag_seconds: 0,
                quality_stale: false,
                policy: crate::models::routing_policy::RoutingPolicyConfigV2::default(),
                attempt_budget: crate::application::routing_policy::AttemptBudgetProfileV1::from_policy(
                    1,
                    &crate::models::routing_policy::RetryFailoverPolicyV2::default(),
                )
                .expect("attempt budget"),
                profile: crate::application::routing_engine::algorithm_profile::DispatchAlgorithmProfile::default(),
                candidates: Vec::new(),
                model_fallback_trigger: None,
                runtime,
            }))
            })
        }

        fn load_operational_route_snapshot(
            &self,
            _request: crate::application::routing_engine::request::RouteRequestFacts,
            _planning_snapshot: crate::application::routing_engine::planning_snapshot::PlanningSnapshot,
        ) -> BoxFuture<'static, Result<OperationalRouteSnapshot, RoutingExecutionReadError>>
        {
            let captured = Arc::clone(&self.captured);
            Box::pin(async move {
                *captured.lock().expect("captured correlation lock") =
                    correlation::current_id_string();
                Ok(OperationalRouteSnapshot {
                    candidates: Vec::new(),
                    targets: Default::default(),
                    profiles: Default::default(),
                    legacy_candidates: Vec::new(),
                })
            })
        }
    }

    struct TestCredentialResolver;

    impl ExecutionCredentialResolver for TestCredentialResolver {
        fn resolve_station_key_secret_ref(
            &self,
            _station_key_id: String,
            _secret_ref: SecretRef,
        ) -> BoxFuture<'static, Result<SecretBytes, ExecutionCredentialError>> {
            Box::pin(async { Ok("test-api-key".to_string().into()) })
        }
    }

    #[tokio::test]
    async fn v2_executor_enters_proxy_request_correlation_scope() {
        let captured = Arc::new(Mutex::new(None));
        let repository = Arc::new(CorrelationCapturingRepository {
            captured: Arc::clone(&captured),
        });
        let limits = ProxyStartupResourceLimits::default();
        let transport_policy =
            TransportPolicySnapshot::from_limits(&limits).expect("test transport policy");
        let upstream_pool = UpstreamClientPool::new_with_transport_policy(
            transport_policy.clone(),
            limits.max_buffered_body_bytes,
            crate::services::proxy::diagnostic_memory::DiagnosticMemoryBudget::new(
                32 * 1024 * 1024,
            ),
        )
        .expect("upstream pool");
        let dropped = Arc::new(AtomicBool::new(false));
        let (writer, worker) = LifecycleWriter::start(
            8,
            Arc::new(DropObservedStore {
                dropped: Arc::clone(&dropped),
            }),
        )
        .expect("lifecycle writer");
        let executor = ProxyExecutor::new(
            repository,
            Arc::new(TestCredentialResolver),
            upstream_pool,
            transport_policy,
            writer.clone(),
            Arc::new(crate::services::proxy::routing_runtime::RoutingRuntimeState::new(64, 1)),
        );
        let request_id = "req_0198108c8411_00003039_0000000000000001".to_string();
        let expected = correlation::CorrelationId::for_proxy_request(&request_id);
        let context = RequestContextSnapshot {
            request_id: request_id.clone(),
            method: "POST".to_string(),
            local_path: "/v1/responses".to_string(),
            endpoint: "responses".to_string(),
            received_at_ms: now_millis_for_services() as i64,
        };
        let reservation = writer.try_reserve_request().expect("request reservation");
        let (terminal, start_ack) = reservation.send_start(RequestStartRecord {
            context: context.clone(),
        });
        start_ack
            .await
            .expect("request start ack channel")
            .expect("request start ack");
        let body_budget = BodyBudget::new(1024);
        let body_lease = body_budget.acquire(2).await.expect("body lease");
        let request_permit = Arc::new(tokio::sync::Semaphore::new(1))
            .try_acquire_owned()
            .expect("request permit");
        let request = CanonicalProxyRequest::new(
            request_id,
            "/v1/responses".to_string(),
            RouteEndpointKind::Responses,
            Some("gpt-test".to_string()),
            false,
            None,
            RequestRequirements::default(),
            bytes::Bytes::from_static(br#"{}"#),
            http::HeaderMap::new(),
            None,
            None,
            None,
            Some(RequestLifecycleAdmission { context, terminal }),
            body_lease,
            RequestLease::new(
                request_permit,
                Arc::new(std::sync::atomic::AtomicU32::new(0)),
            ),
        );

        let failure = match executor.execute(request).await {
            Ok(_) => panic!("empty repository should reject routing"),
            Err(failure) => failure,
        };

        assert_eq!(failure.code.as_str(), "no_available_key");
        assert_eq!(
            captured
                .lock()
                .expect("captured correlation lock")
                .as_deref(),
            Some(expected.as_str())
        );
        assert!(
            correlation::current_id_string().is_none(),
            "proxy request correlation must not leak after execute returns"
        );
        drop(executor);
        drop(writer);
        worker.join().await.expect("lifecycle writer joins");
        assert!(dropped.load(AtomicOrdering::Acquire));
    }

    #[tokio::test]
    async fn v2_runtime_transitions_start_run_drain_stop() {
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start");
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
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        assert!(runtime.start(fixture.config(port)).await.is_err());
        assert_eq!(runtime.status(port).lifecycle, ProxyLifecycle::Failed);
        drop(occupied);
        assert_eq!(
            runtime.start(fixture.config(port)).await.unwrap().lifecycle,
            ProxyLifecycle::Running
        );
        runtime.stop(port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_bind_failure_publishes_final_jsonl_runtime_event() {
        let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("occupy port");
        let port = occupied.local_addr().expect("occupied address").port();
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let root = tempfile::tempdir().expect("runtime log root");
        let service = Arc::new(crate::observability::runtime::RuntimeLogService::open(
            root.path(),
        ));

        crate::observability::runtime::bootstrap::with_test_service(
            Arc::clone(&service),
            || async {
                runtime
                    .start(fixture.config(port))
                    .await
                    .expect_err("occupied port must fail startup");
            },
        )
        .await;
        service.flush();

        let page = crate::observability::runtime::RuntimeLogReader::new(root.path()).read_page(
            0,
            50,
            1024 * 1024,
        );
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        let mut observed = page.lines.iter().filter_map(|line| {
            serde_json::from_slice::<crate::observability::runtime::RuntimeEvent>(line.as_bytes())
                .ok()
        });
        assert!(
            observed.any(|event| event.event_code.as_str() == "proxy.lifecycle.start_failed"),
            "proxy bind failure must reach final JSONL artifact"
        );
        drop(occupied);
    }

    #[tokio::test]
    async fn v2_bind_failure_joins_finalization_worker_before_returning() {
        let occupied = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .expect("occupy port");
        let port = occupied.local_addr().expect("occupied address").port();
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let dropped = Arc::new(AtomicBool::new(false));
        let store = Arc::new(DropObservedStore {
            dropped: Arc::clone(&dropped),
        });

        runtime
            .v2_start_with_lifecycle_store(fixture.config(port), store)
            .await
            .expect_err("occupied port must fail startup");

        assert!(
            dropped.load(AtomicOrdering::Acquire),
            "startup rollback must join the worker and drop its service before returning"
        );
    }

    #[tokio::test]
    async fn v2_stop_reports_finalization_worker_panic() {
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .v2_start_with_lifecycle_store(fixture.config(0), Arc::new(PanicLifecycleStore))
            .await
            .expect("start proxy");

        let response = reqwest::Client::new()
            .get(format!("http://127.0.0.1:{}/v1/models", started.port))
            .bearer_auth("relay-local-secret")
            .send()
            .await
            .expect("send lifecycle-triggering request");
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);

        let error = runtime
            .stop(started.port)
            .await
            .expect_err("worker panic must make shutdown fail");
        assert!(error.contains("lifecycle writer task failed"), "{error}");
        assert_eq!(
            runtime.status(started.port).lifecycle,
            ProxyLifecycle::Failed
        );
    }

    #[tokio::test]
    async fn v2_runtime_is_idempotent_for_same_port_and_rejects_port_change() {
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.unwrap();
        let same = runtime.start(fixture.config(started.port)).await.unwrap();
        assert_eq!(same.port, started.port);
        let different = next_free_port().await;
        assert!(runtime.start(fixture.config(different)).await.is_err());
        runtime.stop(started.port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_runtime_33rd_request_receives_busy_response() {
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let mut config = fixture.config(0);
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
        let fixture = V2ProxyTestFixture::new().await;
        fixture.seed_candidate(upstream.base_url.as_str()).await;
        seed_token_base_price(&fixture, "gpt-test").await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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

        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["id"], "chatcmpl-v2");
        upstream.wait_for_requests(1);
        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].status, "success");
        assert_eq!(logs[0].http_status, Some(200));
        assert_eq!(logs[0].path, "/v1/chat/completions");
        assert_eq!(logs[0].billing_mode.as_deref(), Some("token"));
        assert_eq!(
            logs[0].cost_status.as_deref(),
            Some("complete_single_currency")
        );
        assert!(
            (logs[0].estimated_total_cost.expect("priced request") - 0.000035).abs() < f64::EPSILON
        );
    }

    #[tokio::test]
    async fn v2_streaming_request_lease_survives_handler_return_until_body_drop() {
        let release = Arc::new(AtomicBool::new(false));
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::PausedSse {
            first_chunk: b"data: {\"choices\":[{\"delta\":{\"content\":\"hold\"}}]}\n\n".to_vec(),
            release: Arc::clone(&release),
        }]);
        let fixture = V2ProxyTestFixture::new().await;
        let seeded = fixture.seed_candidate(upstream.base_url.as_str()).await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
        assert_eq!(
            runtime.active_for_station_key(&seeded.station_key_id).await,
            Some(1),
            "the selected key capacity lease must survive while the stream is active"
        );
        let active_logs = fixture.request_logs().await;
        assert_eq!(active_logs.len(), 1);
        assert_eq!(active_logs[0].status, "in_progress");
        assert_eq!(
            active_logs[0].lifecycle_status.as_deref(),
            Some("attempting")
        );
        assert_eq!(
            active_logs[0].station_key_id.as_deref(),
            Some(seeded.station_key_id.as_str()),
            "the active request must expose its selected key without waiting for terminalization"
        );

        drop(response);
        release.store(true, AtomicOrdering::Relaxed);
        wait_runtime_active_requests(&runtime, started.port, 0).await;
        assert_eq!(
            runtime.active_for_station_key(&seeded.station_key_id).await,
            Some(0),
            "dropping the stream must release the selected key capacity lease"
        );
        runtime.stop(started.port).await.unwrap();
    }

    #[tokio::test]
    async fn v2_completed_stream_persists_attempt_usage_and_cost_projection() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Sse(
            br#"data: {"id":"chatcmpl-stream","choices":[{"delta":{"content":"ok"}}]}

data: {"id":"chatcmpl-stream","choices":[],"usage":{"prompt_tokens":2,"completion_tokens":3,"total_tokens":5}}

data: [DONE]

"#
            .to_vec(),
        )]);
        let fixture = V2ProxyTestFixture::new().await;
        fixture.seed_candidate(upstream.base_url.as_str()).await;
        seed_token_base_price(&fixture, "gpt-test").await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
            .expect("send completed stream");
        assert_eq!(response.status(), StatusCode::OK);
        response.bytes().await.expect("consume completed stream");
        runtime.stop(started.port).await.unwrap();

        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].prompt_tokens, Some(2));
        assert_eq!(logs[0].completion_tokens, Some(3));
        assert_eq!(logs[0].total_tokens, Some(5));
        assert_eq!(
            logs[0].cost_status.as_deref(),
            Some("complete_single_currency")
        );
        assert!((logs[0].estimated_total_cost.expect("stream cost") - 0.0001).abs() < f64::EPSILON);

        let request_id = logs[0].id.clone();
        let mut read = fixture
            .runtime()
            .handle()
            .begin_read()
            .await
            .expect("begin stream outcome read");
        let attempt_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM request_attempts WHERE request_id = ?")
                .bind(&request_id)
                .fetch_one(read.connection())
                .await
                .expect("stream attempt count");
        let cost_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM routing_attempt_costs WHERE request_id = ?")
                .bind(&request_id)
                .fetch_one(read.connection())
                .await
                .expect("stream attempt cost count");
        assert_eq!(attempt_count, 1);
        assert_eq!(cost_count, 1);
    }

    #[tokio::test]
    async fn v2_request_log_preserves_nested_reasoning_effort() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"chatcmpl-reasoning","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_vec(),
        )]);
        let fixture = V2ProxyTestFixture::new().await;
        fixture.seed_candidate(upstream.base_url.as_str()).await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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

        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].reasoning_effort.as_deref(), Some("high"));
    }

    #[tokio::test]
    async fn v2_buffered_usage_returns_local_balance_summary_without_upstream() {
        let upstream = LoopbackUpstream::script(Vec::new());
        let fixture = V2ProxyTestFixture::new().await;
        let seeded = fixture.seed_candidate(upstream.base_url.as_str()).await;
        fixture
            .seed_balance(&seeded.station_id, "usage-old", 4.0, "low", "1000")
            .await;
        fixture
            .seed_balance(&seeded.station_id, "usage-new", 12.5, "normal", "2000")
            .await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
        let logs = fixture.request_logs().await;
        assert!(
            logs.is_empty(),
            "balance queries must not create request logs"
        );
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
        let fixture = V2ProxyTestFixture::new().await;
        fixture
            .seed_candidate_named(upstream.base_url.as_str(), "models-a", 0, "auto")
            .await;
        fixture
            .seed_candidate_named(upstream.base_url.as_str(), "models-b", 1, "auto")
            .await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
        let logs = fixture.request_logs().await;
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
        let fixture = V2ProxyTestFixture::new().await;
        fixture
            .set_model_mapping("alias-model", "mapped-model")
            .await;
        fixture
            .seed_candidate_named(upstream.base_url.as_str(), "first", 0, "auto")
            .await;
        fixture
            .seed_candidate_named(upstream.base_url.as_str(), "second", 1, "auto")
            .await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
        let authorization_headers = captured
            .iter()
            .filter_map(|request| request.header("authorization"))
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            authorization_headers,
            std::collections::BTreeSet::from(["Bearer sk-v2-first", "Bearer sk-v2-second",])
        );
        let upstream_body: serde_json::Value =
            serde_json::from_slice(&captured[1].body).expect("upstream body");
        assert_eq!(upstream_body["model"], "mapped-model");
        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].fallback_count, 1);
    }

    #[tokio::test]
    async fn v3_pre_cutover_ignores_legacy_health_axes_and_uses_optimistic_ordering() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"chatcmpl-stable","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}]}"#.to_vec(),
        )]);
        let fixture = V2ProxyTestFixture::new().await;
        let flaky = fixture
            .seed_candidate_named(upstream.base_url.as_str(), "flaky", 0, "auto")
            .await;
        let stable = fixture
            .seed_candidate_named(upstream.base_url.as_str(), "stable", 1, "auto")
            .await;
        let flaky_id = flaky.station_key_id.clone();
        let stable_id = stable.station_key_id.clone();
        fixture
            .runtime()
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO routing_health_axes (
                            scope, axis, health_revision, value_basis_points, updated_at_ms
                         ) VALUES (?1, 'reliability', 1, 2000, 1000), (?2, 'reliability', 1, 9000, 1000)",
                    )
                    .bind(format!("station_key:{flaky_id}"))
                    .bind(format!("station_key:{stable_id}"))
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("routing strategy and health");
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
        // The v3 registry is still pre-cutover in this fixture. Legacy
        // `routing_health_axes` rows must not leak into production ordering;
        // both keys therefore use the deterministic optimistic score and the
        // stable station-key identity tie-breaker.
        assert_eq!(
            upstream.captured_requests()[0].header("authorization"),
            Some("Bearer sk-v2-flaky")
        );
        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].route_policy.as_deref(), Some("automatic_balanced"));
    }

    #[tokio::test]
    async fn v3_active_quality_generation_orders_real_request_samples() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"chatcmpl-quality","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}]}"#.to_vec(),
        )]);
        let fixture = V2ProxyTestFixture::new().await;
        stage_default_v3_policy(&fixture).await;
        let flaky = fixture
            .seed_candidate_named(upstream.base_url.as_str(), "flaky", 0, "auto")
            .await;
        let stable = fixture
            .seed_candidate_named(upstream.base_url.as_str(), "stable", 1, "auto")
            .await;

        append_real_quality_samples(
            &fixture,
            &flaky.station_id,
            &flaky.station_key_id,
            ObservationOutcome::RateLimited,
            "runtime-flaky",
        )
        .await;
        append_real_quality_samples(
            &fixture,
            &stable.station_id,
            &stable.station_key_id,
            ObservationOutcome::Success,
            "runtime-stable",
        )
        .await;

        let handle = fixture.runtime().handle();
        let generation_id = build_ready_once(&handle, &tokio_util::sync::CancellationToken::new())
            .await
            .expect("build quality generation")
            .expect("ready quality generation");
        let coordinator = RoutingGenerationCoordinator::new(handle.clone());
        let qualified_at_ms = chrono::Utc::now().timestamp_millis().max(1);
        coordinator
            .record_qualification(&runtime_quality_qualification(
                &generation_id,
                qualified_at_ms,
            ))
            .await
            .expect("qualify quality generation");
        let fence = coordinator
            .begin_cutover(&generation_id, None, qualified_at_ms.saturating_add(1))
            .await
            .expect("begin quality generation cutover");
        coordinator
            .complete_cutover(&fence, qualified_at_ms.saturating_add(2))
            .await
            .expect("activate quality generation");

        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime
            .start(fixture.config(0))
            .await
            .expect("start v3 quality runtime");
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
            .expect("send quality-ranked chat");
        assert_eq!(response.status(), StatusCode::OK);
        let _ = response.bytes().await.expect("quality-ranked body");
        runtime
            .stop(started.port)
            .await
            .expect("stop quality runtime");

        upstream.wait_for_requests(1);
        assert_eq!(
            upstream.captured_requests()[0].header("authorization"),
            Some("Bearer sk-v2-stable"),
            "the active generation must outrank the Key with 15 attributable 429 samples"
        );
    }

    #[tokio::test]
    async fn v2_connect_failure_falls_back_to_next_candidate_before_output() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"id":"resp-fallback","output_text":"ok"}"#.to_vec(),
        )]);
        let fixture = V2ProxyTestFixture::new().await;
        let offline = fixture
            .seed_candidate_named("http://127.0.0.1:9", "offline", 0, "auto")
            .await;
        let ready = fixture
            .seed_candidate_named(upstream.base_url.as_str(), "ready", 1, "auto")
            .await;
        let offline_id = offline.station_key_id.clone();
        let ready_id = ready.station_key_id.clone();
        fixture
            .runtime()
            .handle()
            .write(|write| {
                Box::pin(async move {
                    sqlx::query(
                        "INSERT INTO routing_health_axes (
                            scope, axis, health_revision, value_basis_points, updated_at_ms
                         ) VALUES (?1, 'reliability', 1, 9000, 1000), (?2, 'reliability', 1, 2000, 1000)",
                    )
                    .bind(format!("station_key:{offline_id}"))
                    .bind(format!("station_key:{ready_id}"))
                    .execute(write.connection())
                    .await?;
                    Ok(())
                })
            })
            .await
            .expect("routing health");
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send responses");
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("responses json");
        let traces = runtime.decision_traces().await;
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["id"], "resp-fallback");
        upstream.wait_for_requests(1);
        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].fallback_count, 1);

        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].profile_version, "DecisionTraceProfileV1");
        let kinds = traces[0]
            .events
            .iter()
            .map(|event| event.kind.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            kinds,
            vec![
                "attempt_start",
                "canonical_failure",
                "attempt_start",
                "request_terminal",
            ]
        );
        let codes = traces[0]
            .events
            .iter()
            .map(|event| event.code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            vec![
                "attempt_start",
                "upstream_transport_failure",
                "attempt_start",
                "request_completed",
            ],
            "the connect failure must be recorded from the canonical classifier"
        );
    }

    #[tokio::test]
    async fn v2_precommit_failure_finalizes_request_log_and_key_circuit() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Status {
            status: 502,
            reason: "Bad Gateway",
        }]);
        let fixture = V2ProxyTestFixture::new().await;
        let seeded = fixture.seed_candidate(upstream.base_url.as_str()).await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

        let response = reqwest::Client::new()
            .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
            .bearer_auth("relay-local-secret")
            .json(&serde_json::json!({"model":"gpt-test","input":"ping"}))
            .send()
            .await
            .expect("send responses");
        let status = response.status();
        let failure_body: serde_json::Value = response.json().await.expect("failure json");
        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(failure_body["error"]["code"], "server_error");
        assert_eq!(failure_body["error"]["message"], "upstream is unavailable");
        runtime.stop(started.port).await.unwrap();

        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 1, "failed v2 requests must be observable");
        assert_eq!(logs[0].status, "failed");
        assert_eq!(logs[0].http_status, Some(502));
        assert_eq!(logs[0].failure_source.as_deref(), Some("upstream"));
        assert_eq!(logs[0].attempt_count, Some(1));
        let mut session = fixture
            .runtime()
            .begin_read()
            .await
            .expect("routing state read session");
        let scoped_observation_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM routing_health_observations WHERE station_id = ?1",
        )
        .bind(&seeded.station_id)
        .fetch_one(session.connection())
        .await
        .expect("scoped health observations");
        assert_eq!(
            scoped_observation_count, 0,
            "ordinary upstream failures must not create a second endpoint-health protection path"
        );
        let circuit: (String, i64) = sqlx::query_as(
            "SELECT state, consecutive_failures
             FROM routing_circuit_state_v3
             WHERE station_key_id = ?1",
        )
        .bind(&seeded.station_key_id)
        .fetch_one(session.connection())
        .await
        .expect("station key circuit state");
        assert_eq!(circuit, ("closed".to_string(), 1));
    }

    #[tokio::test]
    async fn v2_buffered_two_xx_error_envelope_is_classified_and_stops_before_output() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Json(
            br#"{"error":{"message":"upstream exploded","type":"server_error","code":"server_error"}}"#
                .to_vec(),
        )]);
        let fixture = V2ProxyTestFixture::new().await;
        fixture.seed_candidate(upstream.base_url.as_str()).await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");

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
        let status = response.status();
        let failure_body: serde_json::Value = response.json().await.expect("failure json");
        let traces = runtime.decision_traces().await;
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::BAD_GATEWAY);
        assert_eq!(failure_body["error"]["code"], "server_error");
        assert_eq!(traces.len(), 1);
        let codes = traces[0]
            .events
            .iter()
            .map(|event| event.code.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            codes,
            vec!["attempt_start", "upstream_unavailable", "request_failed"],
            "a 2xx error envelope must be classified through the canonical chain and must not fall back"
        );
    }

    #[tokio::test]
    async fn v2_honors_configured_precommit_timeout() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::DelayedHeaders {
            delay: Duration::from_secs(3),
            body: br#"{"id":"too-late"}"#.to_vec(),
        }]);
        let fixture = V2ProxyTestFixture::new().await;
        fixture.seed_candidate(upstream.base_url.as_str()).await;
        let runtime = ProxyRuntimeState::for_tests();
        let mut config = fixture.config(0);
        config.transport_policy.request_deadline = Duration::from_millis(250);
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
        let status = response.status();
        let body: serde_json::Value = response.json().await.expect("timeout response json");
        runtime.stop(started.port).await.unwrap();

        assert_eq!(status, StatusCode::GATEWAY_TIMEOUT, "{body}");
        assert_eq!(body["error"]["code"], "route_deadline_exceeded");
        assert!(
            elapsed < Duration::from_secs(2),
            "configured precommit timeout was ignored: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn v2_loopback_upstream_disconnect_publishes_final_jsonl_event() {
        let upstream = LoopbackUpstream::script(vec![ScriptedResponse::Disconnect]);
        let fixture = V2ProxyTestFixture::new().await;
        fixture.seed_candidate(upstream.base_url.as_str()).await;
        let runtime = ProxyRuntimeState::for_tests();
        let root = tempfile::tempdir().expect("runtime root");
        let service = Arc::new(crate::observability::runtime::RuntimeLogService::open(
            root.path(),
        ));
        let response = crate::observability::runtime::bootstrap::with_test_service(
            Arc::clone(&service),
            || async {
                let started = runtime.start(fixture.config(0)).await.expect("start v2");
                let response = reqwest::Client::new()
                    .post(format!("http://127.0.0.1:{}/v1/responses", started.port))
                    .bearer_auth("relay-local-secret")
                    .json(&serde_json::json!({
                        "model": "gpt-test",
                        "input": "loopback-disconnect",
                    }))
                    .send()
                    .await
                    .expect("send disconnect request");
                let status = response.status();
                let body = response.bytes().await.expect("disconnect response body");
                runtime.stop(started.port).await.expect("stop v2");
                (status, body)
            },
        )
        .await;

        assert_eq!(response.0, StatusCode::BAD_GATEWAY);
        upstream.wait_for_requests(1);
        service.flush();

        let page = crate::observability::runtime::RuntimeLogReader::new(root.path()).read_page(
            0,
            100,
            1024 * 1024,
        );
        assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
        let events = page
            .lines
            .iter()
            .filter_map(|line| {
                serde_json::from_slice::<crate::observability::runtime::RuntimeEvent>(
                    line.as_bytes(),
                )
                .ok()
            })
            .collect::<Vec<_>>();
        assert!(
            events
                .iter()
                .any(|event| event.event_code.as_str() == "proxy.upstream.failed"),
            "upstream disconnect must reach the final JSONL artifact"
        );
        assert!(
            page.lines.iter().all(|line| {
                !line
                    .as_bytes()
                    .windows(b"loopback-disconnect".len())
                    .any(|window| window == b"loopback-disconnect")
            }),
            "runtime JSONL must not retain request payloads"
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
        let fixture = V2ProxyTestFixture::new().await;
        fixture
            .seed_candidate_named(
                upstream.base_url.as_str(),
                "bridge",
                0,
                "openai_chat_completions",
            )
            .await;
        let runtime = ProxyRuntimeState::for_tests();
        let started = runtime.start(fixture.config(0)).await.expect("start v2");
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
        let logs = fixture.request_logs().await;
        assert_eq!(logs.len(), 2);
    }

    #[tokio::test]
    async fn v2_runtime_65th_raw_connection_closes_without_http_response() {
        let fixture = V2ProxyTestFixture::new().await;
        let runtime = ProxyRuntimeState::for_tests();
        let mut config = fixture.config(0);
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
