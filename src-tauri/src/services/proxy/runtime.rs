use std::{
    sync::{
        atomic::{AtomicU32, AtomicU64, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};

use crate::{
    models::proxy::{ProxyLifecycle, ProxyStatus},
    services::{
        database::{now_millis_for_services, AppDatabase},
        proxy::{
            ingress::{self, IngressState, NotWiredExecutor},
            limits::ProxyServerLimits,
            server::{self, RunningServer},
        },
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyRuntimeMode {
    Legacy,
    V2,
}

impl Default for ProxyRuntimeMode {
    fn default() -> Self {
        Self::Legacy
    }
}

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
    mode: ProxyRuntimeMode,
    legacy: Arc<super::legacy_runtime::ProxyRuntimeState>,
    v2: tokio::sync::Mutex<V2RuntimeInner>,
    lifecycle_operation: tokio::sync::Mutex<()>,
    status_snapshot: RwLock<ProxyStatus>,
}

impl Default for ProxyRuntimeState {
    fn default() -> Self {
        Self::new(ProxyRuntimeMode::default())
    }
}

impl ProxyRuntimeState {
    fn new(mode: ProxyRuntimeMode) -> Self {
        Self {
            mode,
            legacy: Arc::new(super::legacy_runtime::ProxyRuntimeState::default()),
            v2: tokio::sync::Mutex::new(V2RuntimeInner::default()),
            lifecycle_operation: tokio::sync::Mutex::new(()),
            status_snapshot: RwLock::new(default_status(0)),
        }
    }

    #[cfg(test)]
    pub(crate) fn for_tests(mode: ProxyRuntimeMode) -> Self {
        Self::new(mode)
    }

    pub fn from_environment_for_dev() -> Self {
        let mode = std::env::var("RELAY_POOL_PROXY_RUNTIME")
            .ok()
            .as_deref()
            .and_then(parse_runtime_mode)
            .unwrap_or_default();
        Self::new(mode)
    }

    pub fn status(&self, default_port: u16) -> ProxyStatus {
        match self.mode {
            ProxyRuntimeMode::Legacy => self.legacy.status(default_port),
            ProxyRuntimeMode::V2 => {
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
        }
    }

    pub async fn start(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        match self.mode {
            ProxyRuntimeMode::Legacy => self.legacy_start(config).await,
            ProxyRuntimeMode::V2 => self.v2_start(config).await,
        }
    }

    pub async fn stop(&self, default_port: u16) -> Result<ProxyStatus, String> {
        match self.mode {
            ProxyRuntimeMode::Legacy => self.legacy_stop(default_port).await,
            ProxyRuntimeMode::V2 => self.v2_stop(default_port).await,
        }
    }

    pub async fn prepare_for_update(&self, timeout: Duration) -> Result<ProxyStatus, String> {
        match self.mode {
            ProxyRuntimeMode::Legacy => {
                let default_port = self.legacy.status(0).port;
                self.legacy_prepare_for_update(default_port, timeout).await
            }
            ProxyRuntimeMode::V2 => self.v2_prepare_for_update(timeout).await,
        }
    }

    pub async fn cleanup_before_update(&self, default_port: u16) -> Result<ProxyStatus, String> {
        self.stop(default_port).await
    }

    pub async fn restart(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        match self.mode {
            ProxyRuntimeMode::Legacy => self.legacy_restart(config).await,
            ProxyRuntimeMode::V2 => {
                let port = config.port;
                let _ = self.v2_stop(port).await?;
                self.v2_start(config).await
            }
        }
    }

    async fn legacy_start(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        let legacy = self.legacy.clone();
        tauri::async_runtime::spawn_blocking(move || {
            legacy.start(config.database, config.data_key, config.port)
        })
        .await
        .map_err(|error| format!("legacy proxy start task failed: {error}"))?
    }

    async fn legacy_stop(&self, default_port: u16) -> Result<ProxyStatus, String> {
        let legacy = self.legacy.clone();
        tauri::async_runtime::spawn_blocking(move || legacy.stop(default_port))
            .await
            .map_err(|error| format!("legacy proxy stop task failed: {error}"))?
    }

    async fn legacy_prepare_for_update(
        &self,
        default_port: u16,
        timeout: Duration,
    ) -> Result<ProxyStatus, String> {
        let legacy = self.legacy.clone();
        tauri::async_runtime::spawn_blocking(move || {
            legacy.prepare_for_update(default_port, timeout)
        })
        .await
        .map_err(|error| format!("legacy proxy drain task failed: {error}"))?
    }

    async fn legacy_restart(&self, config: ProxyStartConfig) -> Result<ProxyStatus, String> {
        let legacy = self.legacy.clone();
        tauri::async_runtime::spawn_blocking(move || {
            legacy.restart(config.database, config.data_key, config.port)
        })
        .await
        .map_err(|error| format!("legacy proxy restart task failed: {error}"))?
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
        let executor = Arc::new(NotWiredExecutor);
        let ingress_state = Arc::new(IngressState::with_active_requests(
            local_access_key,
            config.limits.clone(),
            executor,
            Arc::clone(&active_requests),
            Arc::clone(&request_count),
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
}

fn parse_runtime_mode(value: &str) -> Option<ProxyRuntimeMode> {
    match value.trim().to_ascii_lowercase().as_str() {
        "legacy" => Some(ProxyRuntimeMode::Legacy),
        "v2" => Some(ProxyRuntimeMode::V2),
        _ => None,
    }
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
    use std::time::Duration;

    use http::StatusCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    use super::*;

    #[tokio::test]
    async fn v2_runtime_transitions_start_run_drain_stop() {
        let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
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
        let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
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
        let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
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
        let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
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
    async fn v2_runtime_65th_raw_connection_closes_without_http_response() {
        let runtime = ProxyRuntimeState::for_tests(ProxyRuntimeMode::V2);
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
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        database
            .update_local_access_key("relay-local-secret".to_string())
            .expect("local key");
        ProxyStartConfig::new(
            database,
            crate::services::secrets::crypto::generate_data_key(),
            port,
        )
    }

    async fn next_free_port() -> u16 {
        let listener = tokio::net::TcpListener::bind(("127.0.0.1", 0))
            .await
            .unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        port
    }
}
