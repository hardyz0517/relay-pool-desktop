use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use http::{header, HeaderName, HeaderValue, Method, StatusCode};
use relay_pool_desktop_lib::outbound::{
    AsyncOutboundClient, AsyncOutboundClientConfig, ManualProxy, OutboundFailureKind,
    OutboundHeaderPolicy, OutboundHeaders, OutboundRequest, ProxyPolicy, RequestBudget,
    SecretHeaderValue, TimeoutPolicy, TransportPoolKey,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
};
use tokio_util::sync::CancellationToken;

type BoxServerFuture = Pin<Box<dyn Future<Output = ServerAction> + Send>>;

#[derive(Clone)]
struct TestServer {
    url: String,
    received: Arc<Mutex<Vec<String>>>,
}

enum ServerAction {
    Respond(String),
    DelayThenRespond(Duration, String),
    Hang,
    HeadersThenHang(String),
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
                    ServerAction::DelayThenRespond(delay, response) => {
                        tokio::time::sleep(delay).await;
                        let _ = stream.write_all(response.as_bytes()).await;
                    }
                    ServerAction::Hang => {
                        std::future::pending::<()>().await;
                    }
                    ServerAction::HeadersThenHang(headers) => {
                        let _ = stream.write_all(headers.as_bytes()).await;
                        std::future::pending::<()>().await;
                    }
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

fn test_client(
    first_byte_timeout: Duration,
    body_read_timeout: Duration,
    total_timeout: Duration,
    success_body_max_bytes: usize,
    error_body_max_bytes: usize,
) -> AsyncOutboundClient {
    AsyncOutboundClient::new(AsyncOutboundClientConfig {
        timeouts: TimeoutPolicy {
            connect_timeout: Duration::from_millis(50),
            first_byte_timeout,
            body_read_timeout,
            total_timeout,
        },
        header_policy: OutboundHeaderPolicy::provider_default(),
        success_body_max_bytes,
        error_body_max_bytes,
        max_attempts: 2,
        redirect_max_hops: 2,
        https_downgrade_allowed: false,
    })
}

#[tokio::test]
async fn direct_requests_reuse_one_client_and_redact_evidence() {
    let server = spawn_server(|_| {
        Box::pin(async { ServerAction::Respond(response("200 OK", &[], "provider-body")) })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );

    for _ in 0..3 {
        let result = client
            .execute(
                OutboundRequest::get(
                    format!("{}/models?api_key=secret#frag", server.url),
                    RequestBudget::from_now(Duration::from_secs(2)),
                ),
                CancellationToken::new(),
            )
            .await
            .expect("direct request succeeds");
        assert_eq!(result.status, StatusCode::OK);
        assert_eq!(&result.body[..], b"provider-body");
        assert_eq!(result.evidence.start_url, format!("{}/models", server.url));
        assert!(!result.evidence.body_redaction.contains("provider-body"));
    }

    let metrics = client.metrics();
    assert_eq!(metrics.pool_size, 1);
    assert_eq!(metrics.client_instances_created, 1);
}

#[test]
fn proxy_policy_keys_cover_direct_system_manual_http_and_socks_without_secrets() {
    let direct = ProxyPolicy::Direct.pool_key();
    let system = ProxyPolicy::System.pool_key();
    let http_proxy = ManualProxy::parse_with_credentials(
        "http://127.0.0.1:8080",
        Some("proxy-user".to_string()),
        Some(SecretHeaderValue::new("proxy-password-canary")),
    )
    .expect("manual HTTP proxy");
    let socks_proxy = ManualProxy::parse("socks5h://127.0.0.1:1080").expect("manual SOCKS proxy");

    assert_ne!(direct, system);
    assert_eq!(
        ProxyPolicy::Manual(http_proxy.clone()).pool_key(),
        TransportPoolKey::Manual {
            scheme: relay_pool_desktop_lib::outbound::ProxyScheme::Http,
            endpoint: "http://127.0.0.1:8080".to_string()
        }
    );
    assert!(matches!(
        ProxyPolicy::Manual(socks_proxy).pool_key(),
        TransportPoolKey::Manual { .. }
    ));
    let debug = format!("{http_proxy:?}");
    assert!(!debug.contains("proxy-password-canary"));
    assert!(!format!("{:?}", ProxyPolicy::Manual(http_proxy).pool_key())
        .contains("proxy-password-canary"));
    assert!(ManualProxy::parse("http://user:secret@127.0.0.1:8080").is_err());
}

#[tokio::test]
async fn rejects_userinfo_control_urls_and_headers_outside_allowlist() {
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                "http://user:secret@127.0.0.1/blocked",
                RequestBudget::from_now(Duration::from_secs(1)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("URL userinfo must be rejected");
    assert_eq!(error.kind, OutboundFailureKind::InvalidUrl);

    let error = client
        .execute(
            OutboundRequest::get(
                "http://127.0.0.1/\nblocked",
                RequestBudget::from_now(Duration::from_secs(1)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("control characters must be rejected");
    assert_eq!(error.kind, OutboundFailureKind::InvalidUrl);

    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    let error = headers
        .insert_public(
            HeaderName::from_static("x-provider-secret"),
            HeaderValue::from_static("nope"),
            &policy,
        )
        .expect_err("unlisted header must be rejected");
    assert_eq!(
        error.kind,
        OutboundFailureKind::HeaderNotAllowed("x-provider-secret".to_string())
    );
}

#[tokio::test]
async fn cross_origin_redirect_strips_sensitive_headers_and_redacts_history() {
    let target = spawn_server(|_| {
        Box::pin(async { ServerAction::Respond(response("200 OK", &[], "target-ok")) })
    })
    .await;
    let target_url = target.url.clone();
    let source = spawn_server(move |_| {
        let target_url = target_url.clone();
        Box::pin(async move {
            ServerAction::Respond(response(
                "302 Found",
                &[("Location", &format!("{target_url}/final?token=secret"))],
                "",
            ))
        })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let policy = OutboundHeaderPolicy::provider_default();
    let mut headers = OutboundHeaders::new();
    headers
        .insert_public(
            HeaderName::from_static("x-request-id"),
            HeaderValue::from_static("req-1"),
            &policy,
        )
        .unwrap();
    headers
        .insert_sensitive(
            header::AUTHORIZATION,
            SecretHeaderValue::new("Bearer provider-secret-canary"),
            &policy,
        )
        .unwrap();
    let request = OutboundRequest {
        method: Method::GET,
        url: format!("{}/start", source.url),
        headers,
        body: Vec::new(),
        proxy: ProxyPolicy::Direct,
        budget: RequestBudget::from_now(Duration::from_secs(2)),
    };

    let result = client
        .execute(request, CancellationToken::new())
        .await
        .expect("redirect succeeds");

    assert_eq!(result.status, StatusCode::OK);
    assert_eq!(&result.body[..], b"target-ok");
    assert_eq!(result.evidence.redirect_chain.len(), 1);
    assert!(!format!("{:?}", result.evidence).contains("provider-secret-canary"));
    assert!(!result.evidence.redirect_chain[0].contains("token=secret"));
    let target_request = target.received.lock().expect("target received").join("\n");
    assert!(target_request.contains("x-request-id: req-1"));
    assert!(!target_request.contains("Authorization: Bearer provider-secret-canary"));
}

#[tokio::test]
async fn redirect_loop_and_limit_are_typed_failures() {
    let loop_server = spawn_server(|_| {
        Box::pin(async {
            ServerAction::Respond(response("302 Found", &[("Location", "/loop")], ""))
        })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/loop", loop_server.url),
                RequestBudget::from_now(Duration::from_secs(2)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("redirect loop is typed");
    assert_eq!(error.kind, OutboundFailureKind::RedirectLoop);

    let chain_server = spawn_server(|request| {
        Box::pin(async move {
            let path = request
                .lines()
                .next()
                .and_then(|line| line.split_whitespace().nth(1))
                .unwrap_or("/0")
                .trim_start_matches('/')
                .parse::<u32>()
                .unwrap_or(0);
            ServerAction::Respond(response(
                "302 Found",
                &[("Location", &format!("/{}", path + 1))],
                "",
            ))
        })
    })
    .await;
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/0", chain_server.url),
                RequestBudget::from_now(Duration::from_secs(2)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("redirect limit is typed");
    assert_eq!(error.kind, OutboundFailureKind::RedirectLimitExceeded);
}

#[tokio::test]
async fn body_limit_retry_after_and_remaining_budget_are_typed() {
    let large_body = spawn_server(|_| {
        Box::pin(async { ServerAction::Respond(response("200 OK", &[], "abcdef")) })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        3,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/large", large_body.url),
                RequestBudget::from_now(Duration::from_secs(2)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("body exceeds configured limit");
    assert_eq!(
        error.kind,
        OutboundFailureKind::BodyLimitExceeded { limit_bytes: 3 }
    );

    let retry = spawn_server(|_| {
        Box::pin(async {
            ServerAction::Respond(response(
                "429 Too Many Requests",
                &[("Retry-After", "5")],
                "slow down",
            ))
        })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/retry", retry.url),
                RequestBudget::from_now(Duration::from_millis(100)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("retry-after beyond remaining budget");
    assert_eq!(error.kind, OutboundFailureKind::RetryAfterExceedsBudget);
    assert_eq!(error.retry_after_ms, Some(5_000));
}

#[tokio::test]
async fn first_byte_body_total_timeout_and_cancel_are_distinct() {
    let first_byte_server = spawn_server(|_| {
        Box::pin(async {
            ServerAction::DelayThenRespond(
                Duration::from_millis(100),
                response("200 OK", &[], "late"),
            )
        })
    })
    .await;
    let client = test_client(
        Duration::from_millis(5),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/slow-first-byte", first_byte_server.url),
                RequestBudget::from_now(Duration::from_secs(2)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("first byte timeout");
    assert_eq!(error.kind, OutboundFailureKind::FirstByteTimeout);

    let body_server = spawn_server(|_| {
        Box::pin(async {
            ServerAction::HeadersThenHang(
                "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: keep-alive\r\n\r\n"
                    .to_string(),
            )
        })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_millis(5),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/slow-body", body_server.url),
                RequestBudget::from_now(Duration::from_secs(2)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("body timeout");
    assert_eq!(error.kind, OutboundFailureKind::BodyTimeout);

    let total_server = spawn_server(|_| {
        Box::pin(async {
            ServerAction::HeadersThenHang(
                "HTTP/1.1 200 OK\r\nContent-Length: 4\r\nConnection: keep-alive\r\n\r\n"
                    .to_string(),
            )
        })
    })
    .await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_millis(5),
        1024,
        1024,
    );
    let error = client
        .execute(
            OutboundRequest::get(
                format!("{}/total-timeout", total_server.url),
                RequestBudget::from_now(Duration::from_secs(2)),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("total timeout");
    assert_eq!(error.kind, OutboundFailureKind::TotalTimeout);

    let cancel_server = spawn_server(|_| Box::pin(async { ServerAction::Hang })).await;
    let client = test_client(
        Duration::from_secs(1),
        Duration::from_secs(1),
        Duration::from_secs(2),
        1024,
        1024,
    );
    let token = CancellationToken::new();
    let request = OutboundRequest::get(
        format!("{}/cancel", cancel_server.url),
        RequestBudget::from_now(Duration::from_secs(2)),
    );
    let executing = tokio::spawn({
        let client = client.clone();
        let token = token.clone();
        async move { client.execute(request, token).await }
    });
    tokio::time::sleep(Duration::from_millis(10)).await;
    token.cancel();
    let error = executing
        .await
        .expect("join")
        .expect_err("cancellation is typed");
    assert_eq!(error.kind, OutboundFailureKind::Cancelled);

    let expired = client
        .execute(
            OutboundRequest::get(
                format!("{}/expired", cancel_server.url),
                RequestBudget::from_deadline(Instant::now()),
            ),
            CancellationToken::new(),
        )
        .await
        .expect_err("expired parent budget");
    assert_eq!(expired.kind, OutboundFailureKind::BudgetExhausted);
}
