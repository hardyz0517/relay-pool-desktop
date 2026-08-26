use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::sync::CancellationToken;

#[path = "../src/models/monitoring/outcome.rs"]
pub mod model_outcome;

mod models {
    pub mod monitoring {
        pub use crate::model_outcome::FailureKind;
    }
}

mod services {
    pub mod monitoring {
        pub mod adapters {
            pub mod contract {
                #[derive(Debug, Clone, PartialEq, Eq)]
                pub struct RequestDescriptor {
                    pub method: String,
                    pub path: String,
                    pub body: Vec<u8>,
                    pub stream: bool,
                }
            }
        }
    }
}

mod outbound_shim {
    pub use relay_pool_desktop_lib::outbound::*;
}

use outbound_shim as outbound;

#[path = "../src/services/monitoring/transport.rs"]
pub mod transport;

use models::monitoring::FailureKind;
use services::monitoring::adapters::contract::RequestDescriptor;
use transport::{
    MonitoringAuthHeader, MonitoringTransport, MonitoringTransportConfig,
    MonitoringTransportFailureKind, MonitoringTransportRequest,
};

type BoxServerFuture = Pin<Box<dyn Future<Output = ServerAction> + Send>>;

#[derive(Clone)]
struct TestServer {
    url: String,
    received: Arc<Mutex<Vec<String>>>,
}

enum ServerAction {
    Respond(String),
    RespondChunks(Vec<(Duration, String)>),
    Hang,
}

async fn spawn_server(
    handler: impl Fn(String) -> BoxServerFuture + Send + Sync + 'static,
) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind server");
    let addr = listener.local_addr().expect("server address");
    let received = Arc::new(Mutex::new(Vec::new()));
    let handler = Arc::new(handler);
    let server_received = Arc::clone(&received);
    tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let handler = Arc::clone(&handler);
            let server_received = Arc::clone(&server_received);
            tokio::spawn(async move {
                let mut buffer = [0_u8; 8192];
                let size = stream.read(&mut buffer).await.unwrap_or(0);
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                server_received
                    .lock()
                    .expect("received lock")
                    .push(request.clone());
                match handler(request).await {
                    ServerAction::Respond(response) => {
                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                    ServerAction::RespondChunks(chunks) => {
                        for (delay, chunk) in chunks {
                            if !delay.is_zero() {
                                tokio::time::sleep(delay).await;
                            }
                            if stream.write_all(chunk.as_bytes()).await.is_err() {
                                break;
                            }
                        }
                    }
                    ServerAction::Hang => std::future::pending::<()>().await,
                }
            });
        }
    });
    TestServer {
        url: format!("http://{addr}"),
        received,
    }
}

fn response(status: &str, headers: &[(&str, &str)], body: &str) -> String {
    let mut output = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        output.push_str(name);
        output.push_str(": ");
        output.push_str(value);
        output.push_str("\r\n");
    }
    output.push_str("\r\n");
    output.push_str(body);
    output
}

#[tokio::test]
async fn monitoring_transport_reuses_warm_client_and_redacts_secret_evidence() {
    let canary = "rp-canary-secret-transport";
    let server = spawn_server(|_| {
        Box::pin(async {
            ServerAction::Respond(response(
                "200 OK",
                &[("content-type", "application/json")],
                "{\"ok\":true}",
            ))
        })
    })
    .await;
    let transport = MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url));

    for _ in 0..2 {
        let result = transport
            .execute_buffered(
                request(
                    "/v1/responses",
                    Some(MonitoringAuthHeader {
                        name: "authorization".to_string(),
                        value: outbound::SecretHeaderValue::new(format!("Bearer {canary}")),
                    }),
                    Instant::now() + Duration::from_secs(2),
                ),
                CancellationToken::new(),
            )
            .await
            .expect("transport response");
        assert_eq!(result.http_status, 200);
        let debug = format!("{:?}", result.evidence);
        assert!(!debug.contains(canary));
        assert!(result
            .evidence
            .header_names
            .contains(&"authorization".to_string()));
    }

    assert_eq!(transport.client_metrics().client_instances_created, 1);
    assert_eq!(transport.client_metrics().pool_size, 1);
    assert!(server
        .received
        .lock()
        .expect("received")
        .iter()
        .all(|request| request.contains(canary)));
}

#[tokio::test]
async fn monitoring_transport_maps_body_limit_and_cancellation_to_typed_failures() {
    let server = spawn_server(|_| {
        Box::pin(async {
            ServerAction::Respond(response(
                "200 OK",
                &[("content-type", "application/json")],
                "body-too-large",
            ))
        })
    })
    .await;
    let mut config = MonitoringTransportConfig::loopback_test(&server.url);
    config.success_body_max_bytes = 4;
    let transport = MonitoringTransport::new(config);
    let error = transport
        .execute_buffered(
            request(
                "/v1/responses",
                None,
                Instant::now() + Duration::from_secs(2),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("body cap");
    assert_eq!(
        error.kind,
        MonitoringTransportFailureKind::BodyLimitExceeded
    );
    assert_eq!(error.failure_kind, FailureKind::ProtocolMismatch);

    let hanging = spawn_server(|_| Box::pin(async { ServerAction::Hang })).await;
    let transport =
        MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&hanging.url));
    let token = CancellationToken::new();
    token.cancel();
    let error = transport
        .execute_buffered(
            request(
                "/v1/responses",
                None,
                Instant::now() + Duration::from_secs(2),
            ),
            token,
        )
        .await
        .expect_err("cancelled");
    assert_eq!(error.kind, MonitoringTransportFailureKind::Cancelled);
    assert_eq!(error.failure_kind, FailureKind::Cancelled);
}

#[tokio::test]
async fn monitoring_transport_streams_chunks_without_retaining_success_body() {
    let sse_body = "data: first\n\ndata: second\n\n";
    let server = spawn_server(move |_| {
        let response = response("200 OK", &[("content-type", "text/event-stream")], sse_body);
        Box::pin(async move { ServerAction::Respond(response) })
    })
    .await;
    let transport = MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url));
    let mut received = Vec::new();
    let result = transport
        .execute_streaming(
            request(
                "/v1/responses",
                None,
                Instant::now() + Duration::from_secs(2),
            ),
            CancellationToken::new(),
            |chunk| received.extend_from_slice(chunk),
        )
        .await
        .expect("stream response");

    assert_eq!(received, sse_body.as_bytes());
    assert!(
        result.body.is_empty(),
        "streamed body must not be reconstructed"
    );
    assert_eq!(result.http_status, 200);
}

#[tokio::test]
async fn monitoring_transport_records_first_content_before_stream_completion() {
    let first = "data: first\n\n";
    let second = "data: second\n\n";
    let body_len = first.len() + second.len();
    let head = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nContent-Length: {body_len}\r\nConnection: close\r\n\r\n"
    );
    let server = spawn_server(move |_| {
        let head = head.clone();
        Box::pin(async move {
            ServerAction::RespondChunks(vec![
                (Duration::from_millis(10), format!("{head}{first}")),
                (Duration::from_millis(180), second.to_string()),
            ])
        })
    })
    .await;
    let transport = MonitoringTransport::new(MonitoringTransportConfig::loopback_test(&server.url));

    let result = transport
        .execute_streaming(
            request(
                "/v1/responses",
                None,
                Instant::now() + Duration::from_secs(2),
            ),
            CancellationToken::new(),
            |_| {},
        )
        .await
        .expect("stream response");

    let first_content_ms = result
        .first_content_latency_ms
        .expect("first content timestamp");
    assert!(
        result.first_headers_latency_ms <= first_content_ms,
        "headers must arrive no later than content: headers={} content={}",
        result.first_headers_latency_ms,
        first_content_ms
    );
    assert!(
        first_content_ms < result.total_latency_ms,
        "first content must be measured before the stream completes: first_content={} total={}",
        first_content_ms,
        result.total_latency_ms
    );
    assert!(result.total_latency_ms >= 150);
}

fn request(
    path: &str,
    auth_header: Option<MonitoringAuthHeader>,
    request_deadline: Instant,
) -> MonitoringTransportRequest {
    MonitoringTransportRequest {
        descriptor: RequestDescriptor {
            method: "POST".to_string(),
            path: path.to_string(),
            body: b"{}".to_vec(),
            stream: false,
        },
        public_headers: vec![
            ("accept".to_string(), "application/json".to_string()),
            ("content-type".to_string(), "application/json".to_string()),
        ],
        auth_header,
        request_deadline,
    }
}
