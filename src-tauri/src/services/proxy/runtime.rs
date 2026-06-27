use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use serde_json::Value;

use crate::{
    models::proxy::{CreateRequestLogInput, ProxyStatus},
    services::{
        database::{now_millis_for_services, AppDatabase},
        proxy::{
            adapters::responses::{
                extract_responses_metadata, normalize_responses_request, render_responses_response,
                should_try_chat_fallback, upstream_responses_path,
            },
            build_upstream_url, enabled_candidates, extract_chat_request_metadata, extract_request_kind,
            preferred_candidates,
            openai_error, redact_error_message, should_fallback, RouteCandidate,
        },
    },
};

#[derive(Debug, Default)]
pub struct ProxyRuntimeState {
    inner: Mutex<ProxyRuntimeInner>,
}

#[derive(Debug, Default)]
struct ProxyRuntimeInner {
    running: bool,
    port: u16,
    started_at: Option<String>,
    last_error: Option<String>,
    request_count: Option<Arc<AtomicU64>>,
    stop_signal: Option<Arc<AtomicBool>>,
    active_requests: Option<Arc<AtomicU32>>,
    handle: Option<JoinHandle<()>>,
}

struct ProxyServerContext {
    database: AppDatabase,
    active_requests: Arc<AtomicU32>,
    request_count: Arc<AtomicU64>,
}

struct ParsedRequest {
    method: String,
    path: String,
    headers: HashMap<String, String>,
    body: Vec<u8>,
}

struct ProxyResponse {
    status_code: u16,
    content_type: String,
    body: ProxyResponseBody,
    model: Option<String>,
    stream: bool,
    station_key_id: Option<String>,
    station_id: Option<String>,
    upstream_base_url: Option<String>,
    fallback_count: i64,
    status_label: String,
    error_message: Option<String>,
}

enum ProxyResponseBody {
    Buffered(Vec<u8>),
}

impl ProxyRuntimeState {
    pub fn status(&self, default_port: u16) -> ProxyStatus {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        ProxyStatus {
            running: inner.running,
            bind_addr: "127.0.0.1".to_string(),
            port: if inner.port == 0 { default_port } else { inner.port },
            started_at: inner.started_at.clone(),
            last_error: inner.last_error.clone(),
            active_requests: inner
                .active_requests
                .as_ref()
                .map(|counter| counter.load(Ordering::Relaxed))
                .unwrap_or(0),
            request_count: inner
                .request_count
                .as_ref()
                .map(|counter| counter.load(Ordering::Relaxed))
                .unwrap_or(0),
        }
    }

    pub fn start(&self, database: AppDatabase, port: u16) -> Result<ProxyStatus, String> {
        if port == 0 {
            return Err("本地代理端口必须大于 0".to_string());
        }

        let mut inner = self.inner.lock().map_err(|_| "代理状态锁已损坏".to_string())?;
        if inner.running {
            return Ok(self.status_from_inner(&inner, port));
        }

        let listener = TcpListener::bind(("127.0.0.1", port))
            .map_err(|error| format!("启动本地代理失败，端口 {port} 不可用: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("配置本地代理监听失败: {error}"))?;

        let stop_signal = Arc::new(AtomicBool::new(false));
        let active_requests = Arc::new(AtomicU32::new(0));
        let request_count = Arc::new(AtomicU64::new(0));
        let thread_stop = Arc::clone(&stop_signal);
        let context = Arc::new(ProxyServerContext {
            database,
            active_requests: Arc::clone(&active_requests),
            request_count: Arc::clone(&request_count),
        });
        let handle = thread::spawn(move || run_server(listener, thread_stop, context));

        inner.running = true;
        inner.port = port;
        inner.started_at = Some(now_string());
        inner.last_error = None;
        inner.request_count = Some(Arc::clone(&request_count));
        inner.stop_signal = Some(stop_signal);
        inner.active_requests = Some(active_requests);
        inner.handle = Some(handle);
        Ok(self.status_from_inner(&inner, port))
    }

    pub fn stop(&self, default_port: u16) -> Result<ProxyStatus, String> {
        let (handle, wake_port) = {
            let mut inner = self
                .inner
                .lock()
                .map_err(|_| "代理状态锁已损坏".to_string())?;
            if let Some(stop_signal) = &inner.stop_signal {
                stop_signal.store(true, Ordering::Relaxed);
            }
            inner.running = false;
            let wake_port = if inner.port == 0 { default_port } else { inner.port };
            inner.stop_signal = None;
            inner.active_requests = None;
            inner.request_count = None;
            (inner.handle.take(), wake_port)
        };

        if let Some(handle) = handle {
            let _ = TcpStream::connect(("127.0.0.1", wake_port));
            let _ = handle.join();
        }

        Ok(self.status(default_port))
    }

    pub fn restart(&self, database: AppDatabase, port: u16) -> Result<ProxyStatus, String> {
        let _ = self.stop(port)?;
        self.start(database, port)
    }

    fn status_from_inner(&self, inner: &ProxyRuntimeInner, default_port: u16) -> ProxyStatus {
        ProxyStatus {
            running: inner.running,
            bind_addr: "127.0.0.1".to_string(),
            port: if inner.port == 0 { default_port } else { inner.port },
            started_at: inner.started_at.clone(),
            last_error: inner.last_error.clone(),
            active_requests: inner
                .active_requests
                .as_ref()
                .map(|counter| counter.load(Ordering::Relaxed))
                .unwrap_or(0),
            request_count: inner
                .request_count
                .as_ref()
                .map(|counter| counter.load(Ordering::Relaxed))
                .unwrap_or(0),
        }
    }
}

fn run_server(listener: TcpListener, stop_signal: Arc<AtomicBool>, context: Arc<ProxyServerContext>) {
    while !stop_signal.load(Ordering::Relaxed) {
        match listener.accept() {
            Ok((stream, _)) => {
                if stop_signal.load(Ordering::Relaxed) {
                    let _ = stream.shutdown(Shutdown::Both);
                    break;
                }
                let context = Arc::clone(&context);
                thread::spawn(move || {
                    let _guard = ActiveRequestGuard::new(Arc::clone(&context.active_requests));
                    handle_connection(stream, &context);
                });
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                thread::sleep(std::time::Duration::from_millis(30));
            }
            Err(_) => break,
        }
    }
}

struct ActiveRequestGuard {
    counter: Arc<AtomicU32>,
}

impl ActiveRequestGuard {
    fn new(counter: Arc<AtomicU32>) -> Self {
        counter.fetch_add(1, Ordering::Relaxed);
        Self { counter }
    }
}

impl Drop for ActiveRequestGuard {
    fn drop(&mut self) {
        self.counter.fetch_sub(1, Ordering::Relaxed);
    }
}

fn handle_connection(mut stream: TcpStream, context: &ProxyServerContext) {
    context.request_count.fetch_add(1, Ordering::Relaxed);
    let started_at = now_string();
    let started = Instant::now();
    let (method, path, response) = match read_http_request(&mut stream) {
        Ok(request) => {
            let method = request.method.clone();
            let path = request.path.clone();
            (method, path, handle_proxy_request(context, &request))
        }
        Err(error) => (
            "HTTP".to_string(),
            "/".to_string(),
            ProxyResponse::json_error(400, "bad_request", &error),
        ),
    };
    let finished_at = now_string();
    let _ = context.database.insert_request_log(CreateRequestLogInput {
        method,
        path,
        model: response.model.clone(),
        stream: response.stream,
        status: response.status_label.clone(),
        station_key_id: response.station_key_id.clone(),
        station_id: response.station_id.clone(),
        upstream_base_url: response.upstream_base_url.clone(),
        fallback_count: response.fallback_count,
        error_message: response.error_message.clone(),
        started_at,
        finished_at: Some(finished_at),
        duration_ms: Some(started.elapsed().as_millis() as i64),
    });
    let _ = write_http_response(&mut stream, response);
    let _ = stream.shutdown(Shutdown::Both);
}

fn handle_proxy_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/v1/models") => forward_models_request(context, request),
        ("POST", "/v1/chat/completions") => forward_chat_request(context, request),
        ("POST", "/v1/responses") => forward_responses_request(context, request),
        _ => ProxyResponse::json_error(
            404,
            "not_found",
            "Relay Pool Desktop P5 只支持 /v1/models、/v1/chat/completions 和 /v1/responses。",
        ),
    }
}

fn forward_models_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    let candidates = match context.database.proxy_route_candidates() {
        Ok(candidates) => enabled_candidates(candidates),
        Err(error) => return ProxyResponse::json_error(500, "database_error", &error),
    };
    if candidates.is_empty() {
        return ProxyResponse::json_error(503, "no_enabled_keys", "Key 池中没有可用的 enabled Station Key。");
    }
    forward_with_fallback(context, request, &candidates, None)
}

fn forward_chat_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    let body_value: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return ProxyResponse::json_error(400, "bad_json", &format!("请求 JSON 无法解析: {error}"));
        }
    };
    let request_kind = extract_request_kind(&body_value);
    let (model, stream) = extract_chat_request_metadata(&body_value);
    if stream {
        return ProxyResponse::json_error(
            400,
            "stream_not_supported",
            "P5 MVP 暂不支持 stream=true；请关闭流式或等待 P5.5 SSE 支持。",
        )
        .with_request_meta(model, true);
    }
    let candidates = match context.database.proxy_route_candidates() {
        Ok(candidates) => enabled_candidates(candidates),
        Err(error) => return ProxyResponse::json_error(500, "database_error", &error),
    };
    if candidates.is_empty() {
        return ProxyResponse::json_error(503, "no_enabled_keys", "Key 池中没有可用的 enabled Station Key。")
            .with_request_meta(model, stream);
    }
    let candidates = preferred_candidates(candidates, request_kind);
    forward_with_fallback(context, request, &candidates, model)
}

fn forward_responses_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    let body_value: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return ProxyResponse::json_error(400, "bad_json", &format!("请求 JSON 无法解析: {error}"));
        }
    };
    let (model, stream) = extract_responses_metadata(&body_value);
    if stream {
        return ProxyResponse::json_error(
            400,
            "stream_not_supported",
            "P5 MVP 暂不支持 stream=true；请关闭流式或等待 P5.2 SSE 支持。",
        )
        .with_request_meta(model, true);
    }
    let candidates = match context.database.proxy_route_candidates() {
        Ok(candidates) => enabled_candidates(candidates),
        Err(error) => return ProxyResponse::json_error(500, "database_error", &error),
    };
    if candidates.is_empty() {
        return ProxyResponse::json_error(503, "no_enabled_keys", "Key 池中没有可用的 enabled Station Key。")
            .with_request_meta(model, stream);
    }
    let request_kind = extract_request_kind(&body_value);
    let candidates = preferred_candidates(candidates, request_kind);
    forward_responses_with_fallback(context, request, &candidates, &body_value, model, stream)
}

fn forward_responses_with_fallback(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    candidates: &[RouteCandidate],
    body_value: &Value,
    model: Option<String>,
    stream: bool,
) -> ProxyResponse {
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        let checked_at = now_string();
        match forward_responses_to_candidate(request, candidate, body_value, model.as_deref()) {
            Ok(response) if response.status_code < 400 || !should_fallback(response.status_code) => {
                let response = response
                    .with_candidate(candidate)
                    .with_fallback_count(index as i64)
                    .with_request_meta(model.clone(), false);
                let status_label = response.status_label.clone();
                let used_at = checked_at.clone();
                let _ = context.database.touch_station_key_usage(
                    &candidate.station_key_id,
                    &status_label,
                    Some(&used_at),
                    Some(&checked_at),
                );
                return response;
            }
            Ok(response) => {
                last_error = Some(format!("上游返回 HTTP {}", response.status_code));
                let _ = context.database.touch_station_key_usage(
                    &candidate.station_key_id,
                    "warning",
                    None,
                    Some(&checked_at),
                );
            }
            Err(error) => {
                last_error = Some(error);
                let _ = context.database.touch_station_key_usage(
                    &candidate.station_key_id,
                    "error",
                    None,
                    Some(&checked_at),
                );
            }
        }
    }
    ProxyResponse::json_error(
        502,
        "all_upstreams_failed",
        &format!(
            "所有 enabled Station Key 都转发失败: {}",
            last_error.unwrap_or_else(|| "未知错误".to_string())
        ),
    )
    .with_fallback_count(candidates.len().saturating_sub(1) as i64)
    .with_request_meta(model, stream)
}

fn forward_responses_to_candidate(
    request: &ParsedRequest,
    candidate: &RouteCandidate,
    body_value: &Value,
    fallback_model: Option<&str>,
) -> Result<ProxyResponse, String> {
    let direct_response = forward_to_candidate_with_body(
        request,
        candidate,
        upstream_responses_path(&candidate.upstream_api_format),
        request.body.as_slice(),
    )?;

    if direct_response.status_code < 400 {
        return Ok(render_responses_proxy_response(direct_response, fallback_model));
    }

    if matches!(direct_response.status_code, 404 | 405 | 501)
        && should_try_chat_fallback(&candidate.upstream_api_format)
    {
        let normalized = normalize_responses_request(body_value);
        let chat_request = ParsedRequest {
            method: request.method.clone(),
            path: "/v1/chat/completions".to_string(),
            headers: request.headers.clone(),
            body: serde_json::to_vec(&normalized).unwrap_or_default(),
        };
        let chat_response = forward_to_candidate(&chat_request, candidate)?;
        if chat_response.status_code < 400 {
            return Ok(render_responses_proxy_response(chat_response, fallback_model));
        }
        return Ok(chat_response);
    }

    Ok(direct_response)
}

fn render_responses_proxy_response(response: ProxyResponse, fallback_model: Option<&str>) -> ProxyResponse {
    let body_value = response
        .body_bytes()
        .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
        .unwrap_or_else(|| Value::Null);
    let rendered = render_responses_response(body_value, fallback_model);
    ProxyResponse {
        status_code: response.status_code,
        content_type: "application/json".to_string(),
        body: ProxyResponseBody::Buffered(serde_json::to_vec(&rendered).unwrap_or_default()),
        model: response.model,
        stream: false,
        station_key_id: response.station_key_id,
        station_id: response.station_id,
        upstream_base_url: response.upstream_base_url,
        fallback_count: response.fallback_count,
        status_label: response.status_label,
        error_message: response.error_message,
    }
}

fn forward_with_fallback(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    candidates: &[RouteCandidate],
    model: Option<String>,
) -> ProxyResponse {
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        let checked_at = now_string();
        match forward_to_candidate(request, candidate) {
            Ok(response) if response.status_code < 400 || !should_fallback(response.status_code) => {
                let response = response
                    .with_candidate(candidate)
                    .with_fallback_count(index as i64)
                    .with_request_meta(model.clone(), false);
                let status_label = response.status_label.clone();
                let used_at = checked_at.clone();
                let _ = context.database.touch_station_key_usage(
                    &candidate.station_key_id,
                    &status_label,
                    Some(&used_at),
                    Some(&checked_at),
                );
                return response.with_request_meta(model.clone(), false);
            }
            Ok(response) => {
                last_error = Some(format!("上游返回 HTTP {}", response.status_code));
                let _ = context.database.touch_station_key_usage(
                    &candidate.station_key_id,
                    "warning",
                    None,
                    Some(&checked_at),
                );
            }
            Err(error) => {
                last_error = Some(error);
                let _ = context.database.touch_station_key_usage(
                    &candidate.station_key_id,
                    "error",
                    None,
                    Some(&checked_at),
                );
            }
        }
    }
    ProxyResponse::json_error(
        502,
        "all_upstreams_failed",
        &format!(
            "所有 enabled Station Key 都转发失败: {}",
            last_error.unwrap_or_else(|| "未知错误".to_string())
        ),
    )
    .with_fallback_count(candidates.len().saturating_sub(1) as i64)
    .with_request_meta(model, false)
}

fn forward_to_candidate(
    request: &ParsedRequest,
    candidate: &RouteCandidate,
) -> Result<ProxyResponse, String> {
    forward_to_candidate_with_body(request, candidate, &request.path, request.body.as_slice())
}

fn forward_to_candidate_with_body(
    request: &ParsedRequest,
    candidate: &RouteCandidate,
    upstream_path: &str,
    body: &[u8],
) -> Result<ProxyResponse, String> {
    let url = build_upstream_url(&candidate.upstream_base_url, upstream_path);
    let agent = ureq::AgentBuilder::new()
        .timeout(std::time::Duration::from_secs(45))
        .build();
    let mut upstream = agent
        .request(&request.method, &url)
        .set("authorization", &format!("Bearer {}", candidate.api_key))
        .set("content-type", content_type(request));
    if request.path == "/v1/responses" {
        upstream = upstream.set("accept", "application/json");
    }
    if let Some(accept) = request.headers.get("accept") {
        upstream = upstream.set("accept", accept);
    }

    let result = if body.is_empty() {
        upstream.call()
    } else {
        upstream.send_bytes(body)
    };

    match result {
        Ok(response) => Ok(response_from_upstream(response)),
        Err(ureq::Error::Status(_, response)) => Ok(response_from_upstream(response)),
        Err(error) => Err(redact_error_message(&format!("{error}"))),
    }
}

fn response_from_upstream(response: ureq::Response) -> ProxyResponse {
    let status_code = response.status();
    let content_type = response
        .header("content-type")
        .unwrap_or("application/json")
        .to_string();
    let body = response
        .into_reader()
        .take(2 * 1024 * 1024)
        .bytes()
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    ProxyResponse {
        status_code,
        content_type,
        body: ProxyResponseBody::Buffered(body),
        model: None,
        stream: false,
        station_key_id: None,
        station_id: None,
        upstream_base_url: None,
        fallback_count: 0,
        status_label: if status_code < 400 {
            "success".to_string()
        } else if should_fallback(status_code) {
            "fallback".to_string()
        } else {
            "failed".to_string()
        },
        error_message: if status_code >= 400 {
            Some(format!("上游返回 HTTP {status_code}"))
        } else {
            None
        },
    }
}

impl ProxyResponse {
    fn body_bytes(&self) -> Option<&[u8]> {
        match &self.body {
            ProxyResponseBody::Buffered(bytes) => Some(bytes.as_slice()),
        }
    }

    fn json_error(status_code: u16, code: &str, message: &str) -> Self {
        let body = serde_json::to_vec(&openai_error(message, code)).unwrap_or_else(|_| b"{}".to_vec());
        Self {
            status_code,
            content_type: "application/json".to_string(),
            body: ProxyResponseBody::Buffered(body),
            model: None,
            stream: false,
            station_key_id: None,
            station_id: None,
            upstream_base_url: None,
            fallback_count: 0,
            status_label: "failed".to_string(),
            error_message: Some(redact_error_message(message)),
        }
    }

    fn with_candidate(mut self, candidate: &RouteCandidate) -> Self {
        self.station_key_id = Some(candidate.station_key_id.clone());
        self.station_id = Some(candidate.station_id.clone());
        self.upstream_base_url = Some(candidate.upstream_base_url.clone());
        self
    }

    fn with_fallback_count(mut self, count: i64) -> Self {
        self.fallback_count = count;
        self
    }

    fn with_request_meta(mut self, model: Option<String>, stream: bool) -> Self {
        self.model = model;
        self.stream = stream;
        self
    }
}

fn read_http_request(stream: &mut TcpStream) -> Result<ParsedRequest, String> {
    let mut buffer = Vec::new();
    let mut temp = [0_u8; 4096];
    let mut header_end = None;
    while header_end.is_none() && buffer.len() < 64 * 1024 {
        let read = match stream.read(&mut temp) {
            Ok(read) => read,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                continue;
            }
            Err(error) => return Err(format!("读取请求失败: {error}")),
        };
        if read == 0 {
            break;
        }
        buffer.extend_from_slice(&temp[..read]);
        header_end = find_header_end(&buffer);
    }

    let header_end = header_end.ok_or_else(|| "HTTP 请求头不完整".to_string())?;
    let header_text = String::from_utf8_lossy(&buffer[..header_end]);
    let mut lines = header_text.lines();
    let request_line = lines.next().ok_or_else(|| "HTTP 请求行为空".to_string())?;
    let mut request_parts = request_line.split_whitespace();
    let method = request_parts
        .next()
        .ok_or_else(|| "缺少 HTTP method".to_string())?
        .to_uppercase();
    let path = request_parts
        .next()
        .ok_or_else(|| "缺少 HTTP path".to_string())?
        .split('?')
        .next()
        .unwrap_or("/")
        .to_string();

    let headers = lines
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            Some((key.trim().to_lowercase(), value.trim().to_string()))
        })
        .collect::<HashMap<_, _>>();
    let content_length = headers
        .get("content-length")
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    let body_start = header_end + 4;
    let mut body = buffer.get(body_start..).unwrap_or_default().to_vec();
    if body.len() < content_length {
        let remaining = content_length - body.len();
        let mut tail = vec![0_u8; remaining];
        stream
            .read_exact(&mut tail)
            .map_err(|error| format!("读取请求 body 失败: {error}"))?;
        body.extend_from_slice(&tail);
    }
    body.truncate(content_length);

    Ok(ParsedRequest {
        method,
        path,
        headers,
        body,
    })
}

fn write_http_response(stream: &mut TcpStream, response: ProxyResponse) -> Result<(), String> {
    let reason = reason_phrase(response.status_code);
    let body = response.body_bytes().map(|bytes| bytes.to_vec()).unwrap_or_default();
    let header = format!(
        "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\naccess-control-allow-origin: *\r\nconnection: close\r\n\r\n",
        response.status_code,
        reason,
        response.content_type,
        body.len()
    );
    stream
        .write_all(header.as_bytes())
        .and_then(|_| stream.write_all(&body))
        .map_err(|error| format!("写入响应失败: {error}"))
}

fn find_header_end(buffer: &[u8]) -> Option<usize> {
    buffer.windows(4).position(|window| window == b"\r\n\r\n")
}

fn content_type(request: &ParsedRequest) -> &str {
    request
        .headers
        .get("content-type")
        .map(String::as_str)
        .unwrap_or("application/json")
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        _ => "Relay Pool Response",
    }
}

fn now_string() -> String {
    now_millis_for_services().to_string()
}
