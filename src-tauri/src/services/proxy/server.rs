use std::{
    net::SocketAddr,
    sync::{
        atomic::{AtomicU32, AtomicU64},
        Arc,
    },
    time::Duration,
};

use axum::Router;
use hyper::server::conn::http1;
use hyper_util::{rt::TokioIo, service::TowerToHyperService};
use tokio::{
    net::TcpListener,
    sync::{OwnedSemaphorePermit, Semaphore},
    task::{JoinHandle, JoinSet},
    time::timeout,
};
use tokio_util::sync::CancellationToken;

use super::limits::ProxyServerLimits;

pub struct RunningServer {
    pub local_addr: SocketAddr,
    pub cancel: CancellationToken,
    pub active_requests: Arc<AtomicU32>,
    pub request_count: Arc<AtomicU64>,
    join: JoinHandle<Result<(), String>>,
}

impl RunningServer {
    pub async fn stop(self, timeout_duration: Duration) -> Result<(), String> {
        self.cancel.cancel();
        match timeout(timeout_duration, self.join).await {
            Ok(Ok(result)) => result,
            Ok(Err(error)) => Err(format!("proxy server task failed: {error}")),
            Err(_) => Err("proxy server shutdown timed out".to_string()),
        }
    }
}

pub async fn spawn_server(
    port: u16,
    limits: ProxyServerLimits,
    app: Router,
    active_requests: Arc<AtomicU32>,
    request_count: Arc<AtomicU64>,
) -> Result<RunningServer, String> {
    let listener = TcpListener::bind(("127.0.0.1", port))
        .await
        .map_err(|error| format!("failed to bind v2 local proxy on port {port}: {error}"))?;
    let local_addr = listener
        .local_addr()
        .map_err(|error| format!("failed to read v2 local proxy address: {error}"))?;
    let cancel = CancellationToken::new();
    let join_cancel = cancel.clone();
    let connection_semaphore = Arc::new(Semaphore::new(limits.max_connections));

    let join = tokio::spawn(async move {
        let mut connections = JoinSet::new();
        loop {
            tokio::select! {
                _ = join_cancel.cancelled() => break,
                accepted = listener.accept() => {
                    let (stream, _) = accepted.map_err(|error| format!("v2 local proxy accept failed: {error}"))?;
                    let Ok(permit) = Arc::clone(&connection_semaphore).try_acquire_owned() else {
                        drop(stream);
                        continue;
                    };
                    let service = app.clone();
                    let connection_cancel = join_cancel.clone();
                    connections.spawn(serve_connection(stream, service, permit, connection_cancel));
                }
            }
            while let Some(result) = connections.try_join_next() {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(_error)) => {}
                    Err(_join_error) => {}
                }
            }
        }

        while let Some(result) = connections.join_next().await {
            match result {
                Ok(Ok(())) => {}
                Ok(Err(_error)) => {}
                Err(_join_error) => {}
            }
        }
        Ok(())
    });

    Ok(RunningServer {
        local_addr,
        cancel,
        active_requests,
        request_count,
        join,
    })
}

async fn serve_connection(
    stream: tokio::net::TcpStream,
    service: Router,
    _permit: OwnedSemaphorePermit,
    cancel: CancellationToken,
) -> Result<(), String> {
    let io = TokioIo::new(stream);
    tokio::select! {
        result = http1::Builder::new().serve_connection(io, TowerToHyperService::new(service)) => {
            result.map_err(|error| format!("v2 local proxy connection failed: {error}"))
        }
        _ = cancel.cancelled() => Ok(()),
    }
}
