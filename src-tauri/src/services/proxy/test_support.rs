use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use crate::{
    models::{
        routing::{UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
        station_keys::StationKey,
        stations::CreateStationInput,
    },
    services::{
        database::AppDatabase,
        proxy::runtime::{read_http_request_for_tests, ProxyRuntimeState},
        secrets::crypto::generate_data_key,
    },
};

#[derive(Debug, Clone, Copy)]
pub(crate) enum EndpointProbe {
    Models,
    Chat,
    Responses,
    Embeddings,
}

#[derive(Debug, Clone)]
pub(crate) struct CapturedRequest {
    pub method: String,
    pub path: String,
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
    Status { status: u16, reason: &'static str },
    ChunkedSse(Vec<u8>),
    DisconnectAfterChunk(Vec<u8>),
    PausedSse {
        first_chunk: Vec<u8>,
        release: Arc<AtomicBool>,
    },
    DelayedHeaders { delay: Duration, body: Vec<u8> },
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
            assert!(Instant::now() < deadline, "timed out waiting for upstream requests");
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

pub(crate) struct LegacyGatewayCase {
    database: AppDatabase,
    proxy: ProxyRuntimeState,
    local_key: String,
    port: u16,
    upstream: LoopbackUpstream,
    station_keys: Vec<StationKey>,
    upstream_key_labels: HashMap<String, String>,
    paused_release: Option<Arc<AtomicBool>>,
}

impl LegacyGatewayCase {
    pub(crate) fn new() -> Self {
        Self::with_responses(vec![ok_models(), ok_chat(), ok_responses(), ok_embeddings()])
    }

    pub(crate) fn with_statuses(statuses: [u16; 2]) -> Self {
        Self::with_station_count(
            vec![
                ScriptedResponse::Status {
                    status: statuses[0],
                    reason: status_reason(statuses[0]),
                },
                ok_chat(),
                ScriptedResponse::Status {
                    status: statuses[1],
                    reason: status_reason(statuses[1]),
                },
            ],
            2,
        )
    }

    pub(crate) fn stream_then_disconnect(chunk: &[u8]) -> Self {
        Self::with_station_count(
            vec![ScriptedResponse::DisconnectAfterChunk(chunk.to_vec())],
            2,
        )
    }

    pub(crate) fn paused_stream() -> Self {
        let release = Arc::new(AtomicBool::new(false));
        let mut case = Self::with_station_count(
            vec![ScriptedResponse::PausedSse {
                first_chunk: b"data: {\"choices\":[{\"delta\":{\"content\":\"hold\"}}]}\n\n".to_vec(),
                release: Arc::clone(&release),
            }],
            1,
        );
        case.paused_release = Some(release);
        case
    }

    pub(crate) fn with_responses(responses: Vec<ScriptedResponse>) -> Self {
        Self::with_station_count(responses, 1)
    }

    fn with_station_count(responses: Vec<ScriptedResponse>, station_count: usize) -> Self {
        let upstream = LoopbackUpstream::script(responses);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = generate_data_key();
        let local_key = database
            .ensure_secure_local_access_key()
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
        let station_keys =
            create_station_keys(&database, &data_key, upstream.base_url.as_str(), station_count);
        let upstream_key_labels = station_keys
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let label = if station_count == 1 {
                    "key-a".to_string()
                } else {
                    format!("key-{}", (b'a' + index as u8) as char)
                };
                let credential = if station_count == 1 {
                    "upstream-key".to_string()
                } else {
                    format!("upstream-key-{}", (b'a' + index as u8) as char)
                };
                (format!("Bearer {credential}"), label)
            })
            .collect();
        let proxy = ProxyRuntimeState::default();
        let status = proxy
            .start_ephemeral_for_tests(database.clone(), data_key, )
            .expect("start proxy");

        Self {
            database,
            proxy,
            local_key,
            port: status.port,
            upstream,
            station_keys,
            upstream_key_labels,
            paused_release: None,
        }
    }

    pub(crate) fn local_key(&self) -> &str {
        &self.local_key
    }

    pub(crate) fn request(&self, endpoint: EndpointProbe, token: Option<&str>) -> HttpResponse {
        self.send_raw(endpoint.method(), endpoint.target(), token, endpoint.body(), &[])
    }

    pub(crate) fn upstream_requests(&self) -> usize {
        self.upstream.captured_count()
    }

    pub(crate) fn post_chat(
        &self,
        query: &str,
        headers: &[(&str, &str)],
        body: &[u8],
    ) -> ObservedChatRequest {
        let before = self.upstream.captured_count();
        let target = format!("/v1/chat/completions{query}");
        let downstream = self.send_raw(
            "POST",
            &target,
            Some(&self.local_key),
            body,
            headers,
        );
        self.upstream.wait_for_requests(before + 1);
        let upstream = self
            .upstream
            .captured_requests()
            .into_iter()
            .nth(before)
            .expect("captured upstream request");
        ObservedChatRequest { downstream, upstream }
    }

    pub(crate) fn post_buffered_chat(&self) -> ObservedBufferedChat {
        let response = self.send_raw(
            "POST",
            "/v1/chat/completions",
            Some(&self.local_key),
            br#"{"model":"alias-model","messages":[{"role":"user","content":"ping"}],"stream":false}"#,
            &[],
        );
        self.upstream.wait_for_requests(3);
        let captured = self.upstream.captured_requests();
        let attempted_key_ids = captured
            .iter()
            .filter_map(|request| request.header("authorization"))
            .filter_map(|authorization| self.upstream_key_labels.get(authorization))
            .fold(Vec::<String>::new(), |mut keys, key| {
                if keys.last() != Some(key) {
                    keys.push(key.clone());
                }
                keys
            });
        let selected_key_id = attempted_key_ids
            .last()
            .cloned()
            .unwrap_or_else(|| "<none>".to_string());
        let logs = self.database.list_local_proxy_request_logs().expect("request logs");
        let health = self.database.list_station_key_health().expect("health");
        let health_updates = self
            .station_keys
            .iter()
            .enumerate()
            .filter_map(|(index, key)| {
                let label = format!("key-{}", (b'a' + index as u8) as char);
                health
                    .iter()
                    .find(|item| item.station_key_id == key.id)
                    .map(|item| (label, item.success_count > 0 && item.failure_count == 0))
                    .or_else(|| {
                        health
                            .iter()
                            .find(|item| item.station_key_id == key.id)
                            .map(|item| (format!("key-{}", (b'a' + index as u8) as char), item.success_count > 0))
                    })
            })
            .collect::<Vec<_>>();

        ObservedBufferedChat {
            downstream_status: response.status,
            attempted_key_ids,
            selected_key_id,
            fallback_count: logs.first().map(|log| log.fallback_count).unwrap_or(0),
            health_updates,
            request_logs_len: logs.len(),
        }
    }

    pub(crate) fn post_streaming_chat(&self) -> ObservedStreamChat {
        let response = self.send_raw(
            "POST",
            "/v1/chat/completions",
            Some(&self.local_key),
            br#"{"model":"alias-model","messages":[{"role":"user","content":"ping"}],"stream":true}"#,
            &[("accept", "text/event-stream")],
        );
        self.upstream.wait_for_requests(1);
        let captured = self.upstream.captured_requests();
        let attempted_key_ids = captured
            .iter()
            .filter_map(|request| request.header("authorization"))
            .filter_map(|authorization| self.upstream_key_labels.get(authorization))
            .fold(Vec::<String>::new(), |mut keys, key| {
                if keys.last() != Some(key) {
                    keys.push(key.clone());
                }
                keys
            });
        let logs = self.database.list_local_proxy_request_logs().expect("request logs");

        ObservedStreamChat {
            downstream_body: response.body,
            attempted_key_ids,
            second_upstream_requests: captured
                .iter()
                .filter_map(|request| request.header("authorization"))
                .filter(|authorization| self.upstream_key_labels.get(*authorization).is_some_and(|label| label == "key-b"))
                .count(),
            request_logs_len: logs.len(),
        }
    }

    pub(crate) fn start_streaming_chat(&self) -> ActiveStream {
        let release = self
            .paused_release
            .as_ref()
            .cloned()
            .expect("paused stream release");
        let port = self.port;
        let local_key = self.local_key.clone();
        let handle = thread::spawn(move || {
            let mut stream = TcpStream::connect(("127.0.0.1", port)).expect("connect proxy");
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .expect("read timeout");
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .expect("write timeout");
            let body = br#"{"model":"alias-model","messages":[{"role":"user","content":"ping"}],"stream":true}"#;
            let request = format!(
                "POST /v1/chat/completions HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nAuthorization: Bearer {local_key}\r\nAccept: text/event-stream\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            stream.write_all(request.as_bytes()).expect("write headers");
            stream.write_all(body).expect("write body");
            let mut response = Vec::new();
            let _ = stream.read_to_end(&mut response);
            response
        });
        ActiveStream {
            release,
            handle: Some(handle),
        }
    }

    pub(crate) fn wait_active_requests(&self, expected: u32) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.proxy.status(self.port).active_requests != expected {
            assert!(
                Instant::now() < deadline,
                "active request count did not become {expected}"
            );
            thread::sleep(Duration::from_millis(10));
        }
    }

    pub(crate) fn prepare_for_update(
        &self,
        timeout: Duration,
    ) -> Result<crate::models::proxy::ProxyStatus, String> {
        self.proxy.prepare_for_update(self.port, timeout)
    }

    pub(crate) fn request_logs(&self) -> Vec<crate::models::proxy::RequestLog> {
        self.database
            .list_local_proxy_request_logs()
            .expect("request logs")
    }

    fn send_raw(
        &self,
        method: &str,
        target: &str,
        token: Option<&str>,
        body: &[u8],
        headers: &[(&str, &str)],
    ) -> HttpResponse {
        let mut stream = TcpStream::connect(("127.0.0.1", self.port)).expect("connect proxy");
        stream
            .set_read_timeout(Some(Duration::from_secs(3)))
            .expect("read timeout");
        stream
            .set_write_timeout(Some(Duration::from_secs(3)))
            .expect("write timeout");
        let mut request = format!(
            "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n",
            self.port
        );
        if let Some(token) = token {
            request.push_str(&format!("Authorization: Bearer {token}\r\n"));
        }
        for (name, value) in headers {
            request.push_str(&format!("{name}: {value}\r\n"));
        }
        if !body.is_empty() {
            request.push_str("Content-Type: application/json\r\n");
            request.push_str(&format!("Content-Length: {}\r\n", body.len()));
        }
        request.push_str("\r\n");
        stream.write_all(request.as_bytes()).expect("write headers");
        if !body.is_empty() {
            stream.write_all(body).expect("write body");
        }
        let mut response = Vec::new();
        stream.read_to_end(&mut response).expect("read proxy response");
        HttpResponse::parse(&response)
    }
}

#[derive(Debug)]
pub(crate) struct ObservedChatRequest {
    pub(crate) downstream: HttpResponse,
    pub(crate) upstream: CapturedRequest,
}

#[derive(Debug)]
pub(crate) struct ObservedBufferedChat {
    pub(crate) downstream_status: u16,
    pub(crate) attempted_key_ids: Vec<String>,
    pub(crate) selected_key_id: String,
    pub(crate) fallback_count: i64,
    pub(crate) health_updates: Vec<(String, bool)>,
    pub(crate) request_logs_len: usize,
}

#[derive(Debug)]
pub(crate) struct ObservedStreamChat {
    pub(crate) downstream_body: Vec<u8>,
    pub(crate) attempted_key_ids: Vec<String>,
    pub(crate) second_upstream_requests: usize,
    pub(crate) request_logs_len: usize,
}

pub(crate) struct ActiveStream {
    release: Arc<AtomicBool>,
    handle: Option<JoinHandle<Vec<u8>>>,
}

impl ActiveStream {
    pub(crate) fn release_eof(mut self) -> Vec<u8> {
        self.release.store(true, Ordering::Relaxed);
        self.handle
            .take()
            .expect("stream handle")
            .join()
            .expect("stream client joins")
    }
}

impl Drop for LegacyGatewayCase {
    fn drop(&mut self) {
        let _ = self.proxy.stop(self.port);
    }
}

#[derive(Debug)]
pub(crate) struct HttpResponse {
    pub(crate) status: u16,
    pub(crate) headers: HashMap<String, String>,
    pub(crate) body: Vec<u8>,
}

impl HttpResponse {
    fn parse(bytes: &[u8]) -> Self {
        let split = bytes
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .expect("response header terminator");
        let head = String::from_utf8_lossy(&bytes[..split]);
        let mut lines = head.lines();
        let status = lines
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse::<u16>().ok())
            .expect("response status");
        let headers = lines
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
            .collect();
        Self {
            status,
            headers,
            body: bytes[split + 4..].to_vec(),
        }
    }
}

impl EndpointProbe {
    fn method(self) -> &'static str {
        match self {
            Self::Models => "GET",
            Self::Chat | Self::Responses | Self::Embeddings => "POST",
        }
    }

    fn target(self) -> &'static str {
        match self {
            Self::Models => "/v1/models",
            Self::Chat => "/v1/chat/completions",
            Self::Responses => "/v1/responses",
            Self::Embeddings => "/v1/embeddings",
        }
    }

    fn body(self) -> &'static [u8] {
        match self {
            Self::Models => b"",
            Self::Chat => br#"{"model":"alias-model","messages":[{"role":"user","content":"ping"}],"stream":false}"#,
            Self::Responses => br#"{"model":"alias-model","input":"ping","stream":false}"#,
            Self::Embeddings => br#"{"model":"alias-model","input":"ping"}"#,
        }
    }
}

fn create_station_keys(
    database: &AppDatabase,
    data_key: &[u8; 32],
    upstream_base_url: &str,
    count: usize,
) -> Vec<StationKey> {
    let mut keys = Vec::new();
    for index in 0..count {
        let suffix = (b'a' + index as u8) as char;
        let credential = if count == 1 {
            "upstream-key".to_string()
        } else {
            format!("upstream-key-{suffix}")
        };
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: format!("Contract Station {suffix}"),
                    station_type: "openai-compatible".to_string(),
                    website_url: upstream_base_url.to_string(),
                    api_base_url: format!("{}/v1", upstream_base_url.trim_end_matches('/')),
                    api_key: credential,
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
        keys.extend(database.list_station_keys(station.id).expect("station keys"));
        thread::sleep(Duration::from_millis(2));
    }
    for key in &keys {
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
    }
    keys
}

fn status_reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        404 => "Not Found",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        _ => "Relay Pool Response",
    }
}

fn read_captured_request(stream: &mut TcpStream) -> Result<CapturedRequest, String> {
    let (method, path, path_and_query, headers, body) = read_http_request_for_tests(stream)?;
    Ok(CapturedRequest {
        method,
        path,
        path_and_query,
        headers,
        body,
    })
}

fn write_scripted_response(stream: &mut TcpStream, response: ScriptedResponse) {
    match response {
        ScriptedResponse::Json(body) => write_response(stream, 200, "OK", "application/json", &body),
        ScriptedResponse::Status { status, reason } => {
            write_response(stream, status, reason, "application/json", b"{}")
        }
        ScriptedResponse::ChunkedSse(body) => write_response(stream, 200, "OK", "text/event-stream", &body),
        ScriptedResponse::DisconnectAfterChunk(body) => {
            let header = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
            let _ = stream.write_all(header.as_bytes());
            let _ = stream.write_all(&body);
        }
        ScriptedResponse::PausedSse { first_chunk, release } => {
            let header = "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\nconnection: close\r\n\r\n";
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

fn write_response(stream: &mut TcpStream, status: u16, reason: &str, content_type: &str, body: &[u8]) {
    let header = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: {content_type}\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
        body.len()
    );
    let _ = stream.write_all(header.as_bytes());
    let _ = stream.write_all(body);
}

fn ok_models() -> ScriptedResponse {
    ScriptedResponse::Json(br#"{"object":"list","data":[{"id":"mapped-model","object":"model"}]}"#.to_vec())
}

fn ok_chat() -> ScriptedResponse {
    ScriptedResponse::Json(br#"{"id":"chatcmpl-contract","choices":[{"message":{"role":"assistant","content":"ok"},"finish_reason":"stop","index":0}],"usage":{"prompt_tokens":1,"completion_tokens":1,"total_tokens":2}}"#.to_vec())
}

fn ok_responses() -> ScriptedResponse {
    ScriptedResponse::Json(br#"{"id":"resp_contract","output_text":"ok","usage":{"input_tokens":1,"output_tokens":1,"total_tokens":2}}"#.to_vec())
}

fn ok_embeddings() -> ScriptedResponse {
    ScriptedResponse::Json(br#"{"object":"list","data":[{"embedding":[0.1],"index":0}],"usage":{"prompt_tokens":1,"total_tokens":1}}"#.to_vec())
}
