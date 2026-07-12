use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Instant,
};

use serde_json::{json, Value};

use crate::{
    models::{
        pricing::{BalanceSnapshot, RequestCostEstimate, RequestUsage},
        proxy::{CreateRequestLogInput, ProxyStatus},
        routing::{
            RouteCandidateExplanation, RouteEndpointKind, RoutingGroupFilter, RoutingPolicy,
        },
    },
    services::{
        database::{now_millis_for_services, AppDatabase},
        outbound::{agent_builder_for_proxy, ProxyConfig},
        proxy::{
            adapters::responses::{
                extract_responses_metadata, normalize_responses_request, render_responses_response,
                should_try_chat_fallback, upstream_responses_path,
            },
            build_upstream_url, enabled_candidates, extract_chat_request_metadata,
            observability::{ObservedUsage, RequestObservation, SseUsageObserver},
            openai_error, redact_error_message,
            router::{select_route_candidates, RouteRequest},
            routing_affinity::RouteAffinityStore,
            routing_failure::{
                classify_route_failure, RouteFailureAction, RouteFailureInput, RouteFailureScope,
            },
            routing_health::{apply_health_transition, error_summary_for_failure},
            routing_probe::{ProbeCacheKey, ProbeConfirmationCache},
            routing_types::RouteHealthState,
            scheduler::{
                eligibility::evaluate_candidate,
                types::{CandidateRejectionCode, ScheduleRequest},
            },
            should_fallback, RouteCandidate,
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
    data_key: [u8; 32],
    active_requests: Arc<AtomicU32>,
    request_count: Arc<AtomicU64>,
    route_affinity: Arc<Mutex<RouteAffinityStore>>,
    probe_cache: Arc<Mutex<ProbeConfirmationCache>>,
}

#[derive(Clone)]
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
    retry_after_ms: Option<i64>,
    status_label: String,
    error_message: Option<String>,
    route_policy: Option<String>,
    route_reason: Option<String>,
    rejected_candidates_json: Option<String>,
    group_binding_id: Option<String>,
    normalization_status: Option<String>,
    balance_scope: Option<String>,
    economic_context_json: Option<String>,
    request_cost: RequestCostEstimate,
    pending_successes: Vec<PendingCandidateSuccess>,
}

#[derive(Debug, Clone)]
struct PendingCandidateSuccess {
    station_key_id: String,
    status_label: String,
    checked_at: String,
    duration_ms: i64,
    endpoint: RouteEndpointKind,
    model: Option<String>,
}

enum ProxyResponseBody {
    Buffered(Vec<u8>),
    Streamed(Box<dyn Read + Send>),
}

#[derive(Debug, Default)]
struct ResponseWriteOutcome {
    first_token_ms: Option<i64>,
    usage: Option<ObservedUsage>,
    error_message: Option<String>,
}

#[derive(Debug, Clone)]
struct RouteLogContext {
    policy: RoutingPolicy,
    explanations: Vec<RouteCandidateExplanation>,
}

struct RouteLogMetadata {
    policy: String,
    reason: String,
    rejected_candidates_json: String,
    group_binding_id: Option<String>,
    normalization_status: Option<String>,
    balance_scope: Option<String>,
    economic_context_json: String,
}

impl ProxyRuntimeState {
    pub fn status(&self, default_port: u16) -> ProxyStatus {
        let inner = self.inner.lock().unwrap_or_else(|error| error.into_inner());
        ProxyStatus {
            running: inner.running,
            bind_addr: "127.0.0.1".to_string(),
            port: if inner.port == 0 {
                default_port
            } else {
                inner.port
            },
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

    pub fn start(
        &self,
        database: AppDatabase,
        data_key: [u8; 32],
        port: u16,
    ) -> Result<ProxyStatus, String> {
        if port == 0 {
            return Err("本地代理端口必须大于 0".to_string());
        }

        let mut inner = self
            .inner
            .lock()
            .map_err(|_| "代理状态锁已损坏".to_string())?;
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
            data_key,
            active_requests: Arc::clone(&active_requests),
            request_count: Arc::clone(&request_count),
            route_affinity: Arc::new(Mutex::new(RouteAffinityStore::default())),
            probe_cache: Arc::new(Mutex::new(ProbeConfirmationCache::default())),
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
            let wake_port = if inner.port == 0 {
                default_port
            } else {
                inner.port
            };
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

    pub fn cleanup_before_update(&self, default_port: u16) -> Result<ProxyStatus, String> {
        self.stop(default_port)
    }

    pub fn restart(
        &self,
        database: AppDatabase,
        data_key: [u8; 32],
        port: u16,
    ) -> Result<ProxyStatus, String> {
        let _ = self.stop(port)?;
        self.start(database, data_key, port)
    }

    fn status_from_inner(&self, inner: &ProxyRuntimeInner, default_port: u16) -> ProxyStatus {
        ProxyStatus {
            running: inner.running,
            bind_addr: "127.0.0.1".to_string(),
            port: if inner.port == 0 {
                default_port
            } else {
                inner.port
            },
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

fn run_server(
    listener: TcpListener,
    stop_signal: Arc<AtomicBool>,
    context: Arc<ProxyServerContext>,
) {
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
    let (method, path, request_observation, response) = match read_http_request(&mut stream) {
        Ok(request) => {
            let method = request.method.clone();
            let path = request.path.clone();
            let request_observation = serde_json::from_slice::<Value>(&request.body)
                .ok()
                .map(|body| RequestObservation::from_json(&body))
                .unwrap_or_default();
            (
                method,
                path,
                request_observation,
                handle_proxy_request(context, &request),
            )
        }
        Err(error) => (
            "HTTP".to_string(),
            "/".to_string(),
            RequestObservation::default(),
            ProxyResponse::json_error(400, "bad_request", &error),
        ),
    };
    let mut log_snapshot = RequestLogSnapshot::from_response(&response, request_observation);
    let pending_successes = response.pending_successes.clone();
    let write_result = write_http_response(&mut stream, response, started);
    if let Some(usage) = write_result.usage.as_ref() {
        log_snapshot.request_cost = request_cost_for_observed_usage(
            context,
            log_snapshot.station_key_id.as_deref(),
            log_snapshot.station_id.as_deref(),
            log_snapshot.model.as_deref(),
            usage,
        );
    }
    if write_result.error_message.is_none() {
        let completed_at = now_string();
        commit_pending_successes(
            context,
            &pending_successes,
            Some(&completed_at),
            Some(started.elapsed().as_millis() as i64),
        );
    }
    let _ = context.database.insert_request_log(build_request_log_input(
        method,
        path,
        log_snapshot,
        started_at,
        started,
        &write_result,
    ));
    let _ = stream.shutdown(Shutdown::Both);
}

fn commit_pending_successes(
    context: &ProxyServerContext,
    pending_successes: &[PendingCandidateSuccess],
    completed_at: Option<&str>,
    completed_duration_ms: Option<i64>,
) {
    for success in pending_successes {
        let checked_at = completed_at.unwrap_or(success.checked_at.as_str());
        let duration_ms = completed_duration_ms.unwrap_or(success.duration_ms);
        let _ = context.database.touch_station_key_usage(
            &success.station_key_id,
            &success.status_label,
            Some(checked_at),
            Some(checked_at),
        );
        let _ = context.database.record_station_key_success(
            &success.station_key_id,
            duration_ms,
            checked_at,
        );
        if let Ok(mut affinity) = context.route_affinity.lock() {
            affinity.record_success(
                success.endpoint.clone(),
                success.model.as_deref(),
                &success.station_key_id,
                checked_at
                    .parse::<i64>()
                    .unwrap_or_else(|_| now_millis_for_services() as i64),
            );
        }
    }
}

#[derive(Debug, Clone)]
struct RequestLogSnapshot {
    model: Option<String>,
    stream: bool,
    status_label: String,
    station_key_id: Option<String>,
    station_id: Option<String>,
    upstream_base_url: Option<String>,
    fallback_count: i64,
    error_message: Option<String>,
    route_policy: Option<String>,
    route_reason: Option<String>,
    rejected_candidates_json: Option<String>,
    request_cost: RequestCostEstimate,
    group_binding_id: Option<String>,
    normalization_status: Option<String>,
    balance_scope: Option<String>,
    economic_context_json: Option<String>,
    reasoning_effort: Option<String>,
}

impl RequestLogSnapshot {
    fn from_response(response: &ProxyResponse, observation: RequestObservation) -> Self {
        Self {
            model: response.model.clone(),
            stream: response.stream,
            status_label: response.status_label.clone(),
            station_key_id: response.station_key_id.clone(),
            station_id: response.station_id.clone(),
            upstream_base_url: response.upstream_base_url.clone(),
            fallback_count: response.fallback_count,
            error_message: response.error_message.clone(),
            route_policy: response.route_policy.clone(),
            route_reason: response.route_reason.clone(),
            rejected_candidates_json: response.rejected_candidates_json.clone(),
            request_cost: response.request_cost.clone(),
            group_binding_id: response.group_binding_id.clone(),
            normalization_status: response.normalization_status.clone(),
            balance_scope: response.balance_scope.clone(),
            economic_context_json: response.economic_context_json.clone(),
            reasoning_effort: observation.reasoning_effort,
        }
    }
}

fn build_request_log_input(
    method: String,
    path: String,
    snapshot: RequestLogSnapshot,
    started_at: String,
    started: Instant,
    write_result: &ResponseWriteOutcome,
) -> CreateRequestLogInput {
    let interrupted = snapshot.stream && write_result.error_message.is_some();
    let error_message = if interrupted {
        write_result.error_message.clone()
    } else {
        snapshot.error_message.clone()
    };

    CreateRequestLogInput {
        method,
        path,
        model: snapshot.model,
        stream: snapshot.stream,
        status: if interrupted {
            "interrupted".to_string()
        } else {
            snapshot.status_label
        },
        lifecycle_status: Some(if interrupted {
            "interrupted".to_string()
        } else {
            "completed".to_string()
        }),
        station_key_id: snapshot.station_key_id,
        station_id: snapshot.station_id,
        upstream_base_url: snapshot.upstream_base_url,
        fallback_count: snapshot.fallback_count,
        error_message,
        route_policy: snapshot.route_policy,
        route_reason: snapshot.route_reason,
        rejected_candidates_json: snapshot.rejected_candidates_json,
        prompt_tokens: write_result
            .usage
            .as_ref()
            .and_then(|usage| usage.input_tokens)
            .or(snapshot.request_cost.prompt_tokens),
        completion_tokens: write_result
            .usage
            .as_ref()
            .and_then(|usage| usage.output_tokens)
            .or(snapshot.request_cost.completion_tokens),
        total_tokens: write_result
            .usage
            .as_ref()
            .and_then(|usage| usage.total_tokens)
            .or(snapshot.request_cost.total_tokens),
        cache_creation_tokens: write_result
            .usage
            .as_ref()
            .and_then(|usage| usage.cache_creation_tokens)
            .or(snapshot.request_cost.cache_creation_tokens),
        cache_read_tokens: write_result
            .usage
            .as_ref()
            .and_then(|usage| usage.cache_read_tokens)
            .or(snapshot.request_cost.cache_read_tokens),
        reasoning_effort: snapshot.reasoning_effort,
        first_token_ms: write_result.first_token_ms,
        billing_mode: snapshot.request_cost.billing_mode,
        estimated_input_cost: snapshot.request_cost.estimated_input_cost,
        estimated_output_cost: snapshot.request_cost.estimated_output_cost,
        estimated_total_cost: snapshot.request_cost.estimated_total_cost,
        base_input_cost: snapshot.request_cost.base_input_cost,
        base_output_cost: snapshot.request_cost.base_output_cost,
        base_fixed_cost: snapshot.request_cost.base_fixed_cost,
        base_total_cost: snapshot.request_cost.base_total_cost,
        cost_currency: snapshot.request_cost.cost_currency,
        pricing_rule_id: snapshot.request_cost.pricing_rule_id,
        pricing_source: snapshot.request_cost.pricing_source,
        cost_status: Some(snapshot.request_cost.cost_status),
        group_binding_id: snapshot.group_binding_id,
        normalization_status: snapshot.normalization_status,
        balance_scope: snapshot.balance_scope,
        economic_context_json: snapshot.economic_context_json,
        started_at,
        finished_at: Some(now_string()),
        duration_ms: Some(started.elapsed().as_millis() as i64),
    }
}

fn handle_proxy_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    if request.method == "OPTIONS" {
        return cors_preflight_response();
    }

    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/usage") | ("GET", "/v1/usage") => local_usage_response(context),
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

fn local_usage_response(context: &ProxyServerContext) -> ProxyResponse {
    let snapshots = match context.database.list_balance_snapshots() {
        Ok(snapshots) => snapshots,
        Err(error) => return ProxyResponse::json_error(500, "database_error", &error),
    };
    let mut latest_by_station = HashMap::new();
    for snapshot in snapshots {
        let should_replace = latest_by_station
            .get(&snapshot.station_id)
            .map(|current| balance_snapshot_rank(&snapshot) > balance_snapshot_rank(current))
            .unwrap_or(true);
        if snapshot.scope == "station" && should_replace {
            latest_by_station.insert(snapshot.station_id.clone(), snapshot);
        }
    }

    let latest_station_balances = latest_by_station.values().collect::<Vec<_>>();
    let total_balance = latest_station_balances
        .iter()
        .filter_map(|snapshot| snapshot.value)
        .sum::<f64>();
    let currency = latest_station_balances
        .iter()
        .find_map(|snapshot| {
            let currency = snapshot.currency.trim();
            (!currency.is_empty()).then(|| currency.to_string())
        })
        .unwrap_or_else(|| "CNY".to_string());
    let low_balance_stations = latest_station_balances
        .iter()
        .filter(|snapshot| snapshot.status == "low" || snapshot.status == "depleted")
        .count();
    let updated_at = latest_station_balances
        .iter()
        .map(|snapshot| snapshot.updated_at.as_str())
        .max()
        .map(str::to_string);
    let body = serde_json::to_vec(&json!({
        "is_active": true,
        "remaining": total_balance,
        "balance": total_balance,
        "unit": currency,
        "quota": {
            "remaining": total_balance,
            "unit": currency,
        },
        "source": "relay_pool_desktop_balance_snapshots",
        "stations": latest_station_balances.len(),
        "low_balance_stations": low_balance_stations,
        "updated_at": updated_at,
    }))
    .unwrap_or_else(|_| b"{}".to_vec());

    ProxyResponse {
        status_code: 200,
        content_type: "application/json".to_string(),
        body: ProxyResponseBody::Buffered(body),
        model: None,
        stream: false,
        station_key_id: None,
        station_id: None,
        upstream_base_url: None,
        fallback_count: 0,
        retry_after_ms: None,
        status_label: "success".to_string(),
        error_message: None,
        route_policy: None,
        route_reason: None,
        rejected_candidates_json: None,
        group_binding_id: None,
        normalization_status: None,
        balance_scope: Some("station".to_string()),
        economic_context_json: None,
        request_cost: crate::services::pricing::request_cost_unknown(),
        pending_successes: Vec::new(),
    }
}

fn balance_snapshot_rank(snapshot: &BalanceSnapshot) -> (i128, i128, i128) {
    (
        parse_balance_time(&snapshot.updated_at),
        parse_balance_time(&snapshot.created_at),
        snapshot
            .collected_at
            .as_deref()
            .map(parse_balance_time)
            .unwrap_or(0),
    )
}

fn parse_balance_time(value: &str) -> i128 {
    value.trim().parse::<i128>().unwrap_or(0)
}

fn forward_models_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    let candidates = match context
        .database
        .proxy_route_candidates_with_data_key(&context.data_key)
    {
        Ok(candidates) => enabled_candidates(candidates),
        Err(error) => return ProxyResponse::json_error(500, "database_error", &error),
    };
    if candidates.is_empty() {
        return ProxyResponse::json_error(
            503,
            "no_enabled_keys",
            "Key 池中没有可用的 enabled Station Key。",
        );
    }
    aggregate_models_request(context, request, &candidates)
}

fn aggregate_models_request(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    candidates: &[RouteCandidate],
) -> ProxyResponse {
    let mut seen_ids = HashSet::new();
    let mut models = Vec::new();
    let mut success_count = 0_i64;
    let mut failed_count = 0_i64;
    let mut last_error = None;

    for candidate in candidates {
        let checked_at = now_string();
        let attempt_started = Instant::now();
        match forward_to_candidate(request, candidate, false) {
            Ok(response) if response.status_code < 400 => {
                match extract_models_from_response(&response) {
                    Ok(items) => {
                        success_count += 1;
                        for item in items {
                            let Some(id) = item.get("id").and_then(Value::as_str) else {
                                continue;
                            };
                            if seen_ids.insert(id.to_string()) {
                                models.push(item);
                            }
                        }
                        let used_at = checked_at.clone();
                        record_candidate_success(
                            context,
                            candidate,
                            "success",
                            &used_at,
                            &checked_at,
                            attempt_started.elapsed().as_millis() as i64,
                        );
                    }
                    Err(error) => {
                        failed_count += 1;
                        last_error = Some(error.clone());
                        record_candidate_failure(
                            context,
                            candidate,
                            "warning",
                            &checked_at,
                            &error,
                            RouteFailureInput::transport_error(false, &error),
                        );
                    }
                }
            }
            Ok(response) => {
                failed_count += 1;
                let error = format!("上游返回 HTTP {}", response.status_code);
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    candidate,
                    "warning",
                    &checked_at,
                    &error,
                    RouteFailureInput::http_status_with_retry_after(
                        response.status_code,
                        false,
                        response.retry_after_ms,
                    ),
                );
            }
            Err(error) => {
                failed_count += 1;
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    candidate,
                    "error",
                    &checked_at,
                    &error,
                    RouteFailureInput::transport_error(false, &error),
                );
            }
        }
    }

    if success_count == 0 {
        return ProxyResponse::json_error(
            502,
            "all_upstreams_failed",
            &format!(
                "所有 enabled Station Key 都无法获取模型列表: {}",
                last_error.unwrap_or_else(|| "未知错误".to_string())
            ),
        )
        .with_fallback_count(candidates.len().saturating_sub(1) as i64);
    }

    ProxyResponse {
        status_code: 200,
        content_type: "application/json".to_string(),
        body: ProxyResponseBody::Buffered(
            serde_json::to_vec(&serde_json::json!({
                "object": "list",
                "data": models
            }))
            .unwrap_or_else(|_| b"{\"object\":\"list\",\"data\":[]}".to_vec()),
        ),
        model: None,
        stream: false,
        station_key_id: None,
        station_id: None,
        upstream_base_url: None,
        fallback_count: failed_count,
        retry_after_ms: None,
        status_label: "success".to_string(),
        error_message: None,
        route_policy: None,
        route_reason: None,
        rejected_candidates_json: None,
        group_binding_id: None,
        normalization_status: None,
        balance_scope: None,
        economic_context_json: None,
        request_cost: crate::services::pricing::request_cost_unknown(),
        pending_successes: Vec::new(),
    }
}

fn extract_models_from_response(response: &ProxyResponse) -> Result<Vec<Value>, String> {
    let body = response
        .body_bytes()
        .ok_or_else(|| "模型列表响应不是可读取的 JSON body".to_string())?;
    let value: Value =
        serde_json::from_slice(body).map_err(|error| format!("模型列表 JSON 无法解析: {error}"))?;
    if let Some(data) = value.get("data").and_then(Value::as_array) {
        return Ok(data.clone());
    }
    if let Some(data) = value.as_array() {
        return Ok(data.clone());
    }
    Err("模型列表响应缺少 data 数组".to_string())
}

fn select_proxy_route(
    context: &ProxyServerContext,
    route_request: &RouteRequest,
) -> Result<crate::services::proxy::router::RouteSelection, ProxyResponse> {
    let rich_candidates = context
        .database
        .proxy_rich_route_candidates_with_data_key(&context.data_key)
        .map_err(|error| ProxyResponse::json_error(500, "database_error", &error))?;
    if rich_candidates.is_empty() {
        return Err(ProxyResponse::json_error(
            503,
            "no_enabled_keys",
            "Key 池中没有可用的 enabled Station Key。",
        ));
    }
    let aliases = context
        .database
        .enabled_model_alias_pairs()
        .map_err(|error| ProxyResponse::json_error(500, "database_error", &error))?;
    let route = select_route_candidates(route_request, rich_candidates, &aliases)
        .map_err(|error| ProxyResponse::json_error(500, "route_selector_error", &error))?;
    if route.accepted.is_empty() {
        let log_context = route_log_context(&route_request.policy, &route.explanations);
        let error_code = route
            .scheduler_error_code
            .as_deref()
            .unwrap_or("no_route_candidates");
        return Err(ProxyResponse::json_error(
            503,
            error_code,
            &format!(
                "没有可用 Station Key 支持该请求：model={} endpoint={:?} stream={}",
                route_request.model.as_deref().unwrap_or("<none>"),
                route_request.endpoint,
                route_request.stream
            ),
        )
        .with_route_metadata(route_log_metadata(&log_context, None)));
    }
    Ok(route)
}

fn revalidate_automatic_candidate_before_forward(
    context: &ProxyServerContext,
    route_request: &RouteRequest,
    station_key_id: &str,
) -> Result<(), ProxyResponse> {
    if !matches!(route_request.policy, RoutingPolicy::AutomaticBalanced) {
        return Ok(());
    }
    let max_rate_multiplier = route_request.max_rate_multiplier.ok_or_else(|| {
        ProxyResponse::json_error(
            503,
            "routing_multiplier_limit_not_configured",
            "自动路由需要先设置倍率上限",
        )
    })?;
    if !max_rate_multiplier.is_finite() || max_rate_multiplier < 0.0 {
        return Err(ProxyResponse::json_error(
            503,
            "routing_multiplier_limit_not_configured",
            "自动路由倍率上限无效",
        ));
    }

    let now_ms = now_millis_for_services() as i64;
    let candidates = context
        .database
        .load_scheduler_candidates(&route_request.routing_group_filter, now_ms)
        .map_err(|error| ProxyResponse::json_error(500, "database_error", &error))?;
    let Some(candidate) = candidates
        .into_iter()
        .find(|candidate| candidate.station_key_id == station_key_id)
    else {
        return Err(ProxyResponse::json_error(
            503,
            missing_revalidation_candidate_error_code(&route_request.routing_group_filter),
            "自动路由转发前预检失败：选中的 Key 已不在当前可调度候选中",
        ));
    };

    let schedule_request = ScheduleRequest {
        endpoint: route_request.endpoint.clone(),
        requested_model: route_request.model.clone(),
        mapped_model: route_request.model.clone(),
        routing_group_filter: route_request.routing_group_filter.clone(),
        stream: route_request.stream,
        uses_tools: route_request.uses_tools,
        uses_vision: route_request.uses_vision,
        uses_reasoning: route_request.uses_reasoning,
        max_rate_multiplier,
        session_hash: route_request.session_hash.clone(),
        previous_response_id: route_request.previous_response_id.clone(),
        excluded_key_ids: route_request.excluded_key_ids.clone(),
        now_ms,
    };
    evaluate_candidate(&schedule_request, &candidate).map_err(|rejection| {
        ProxyResponse::json_error(
            503,
            candidate_revalidation_error_code(&rejection.reasons),
            "自动路由转发前预检失败：选中的 Key 不再满足当前请求的硬约束",
        )
    })
}

fn missing_revalidation_candidate_error_code(filter: &RoutingGroupFilter) -> &'static str {
    if matches!(filter, RoutingGroupFilter::AllGroups) {
        "routing_no_eligible_candidate"
    } else {
        "routing_no_candidate_in_group_scope"
    }
}

fn candidate_revalidation_error_code(reasons: &[CandidateRejectionCode]) -> &'static str {
    if reasons.contains(&CandidateRejectionCode::MultiplierOverCeiling) {
        return "routing_no_candidate_within_multiplier_limit";
    }
    if reasons.contains(&CandidateRejectionCode::MultiplierEvidenceExpired) {
        return "routing_multiplier_evidence_expired";
    }
    if reasons.iter().any(|reason| {
        matches!(
            reason,
            CandidateRejectionCode::NoMultiplierEvidence
                | CandidateRejectionCode::MultiplierEvidenceInvalid
                | CandidateRejectionCode::MultiplierEvidenceNegative
                | CandidateRejectionCode::MultiplierEvidenceUnboundGroup
                | CandidateRejectionCode::MultiplierEvidenceLowConfidence
        )
    }) {
        return "routing_no_multiplier_evidence";
    }
    if reasons.contains(&CandidateRejectionCode::RoutingGroupMismatch) {
        return "routing_no_candidate_in_group_scope";
    }
    "routing_no_eligible_candidate"
}

fn route_request_for_chat(
    request: &ParsedRequest,
    model: Option<String>,
    stream: bool,
    body: &Value,
    policy: RoutingPolicy,
    max_rate_multiplier: Option<f64>,
    routing_group_filter: RoutingGroupFilter,
    allow_depleted_fallback: bool,
    current_station_key_id: Option<String>,
) -> RouteRequest {
    RouteRequest {
        endpoint: RouteEndpointKind::ChatCompletions,
        model: model.clone(),
        stream,
        uses_tools: uses_tools(body),
        uses_vision: uses_vision(body),
        uses_reasoning: uses_reasoning(body, model.as_deref()),
        policy,
        max_rate_multiplier,
        routing_group_filter,
        session_hash: request_session_hash(request, body),
        previous_response_id: previous_response_id(body),
        excluded_key_ids: Vec::new(),
        current_station_key_id,
        allow_depleted_fallback,
        now_ms: now_millis_for_services() as i64,
    }
}

fn route_request_for_responses(
    request: &ParsedRequest,
    model: Option<String>,
    stream: bool,
    body: &Value,
    policy: RoutingPolicy,
    max_rate_multiplier: Option<f64>,
    routing_group_filter: RoutingGroupFilter,
    allow_depleted_fallback: bool,
    current_station_key_id: Option<String>,
) -> RouteRequest {
    RouteRequest {
        endpoint: RouteEndpointKind::Responses,
        model: model.clone(),
        stream,
        uses_tools: uses_tools(body),
        uses_vision: uses_vision(body),
        uses_reasoning: uses_reasoning(body, model.as_deref()),
        policy,
        max_rate_multiplier,
        routing_group_filter,
        session_hash: request_session_hash(request, body),
        previous_response_id: previous_response_id(body),
        excluded_key_ids: Vec::new(),
        current_station_key_id,
        allow_depleted_fallback,
        now_ms: now_millis_for_services() as i64,
    }
}

fn lookup_route_affinity(
    context: &ProxyServerContext,
    endpoint: RouteEndpointKind,
    model: Option<&str>,
) -> Option<String> {
    context
        .route_affinity
        .lock()
        .ok()
        .and_then(|mut affinity| affinity.lookup(endpoint, model, now_millis_for_services() as i64))
}

fn parse_routing_policy(value: &str) -> RoutingPolicy {
    match value {
        "automatic_balanced" | "automatic" => RoutingPolicy::AutomaticBalanced,
        "stable_first" | "stable" => RoutingPolicy::StableFirst,
        "backup_only" => RoutingPolicy::BackupOnly,
        "cheap_first" => RoutingPolicy::CheapFirst,
        "cost_stable_first" => RoutingPolicy::CostStableFirst,
        _ => RoutingPolicy::PriorityFallback,
    }
}

fn route_log_context(
    policy: &RoutingPolicy,
    explanations: &[RouteCandidateExplanation],
) -> RouteLogContext {
    RouteLogContext {
        policy: policy.clone(),
        explanations: explanations.to_vec(),
    }
}

fn route_log_metadata(
    context: &RouteLogContext,
    selected_station_key_id: Option<&str>,
) -> RouteLogMetadata {
    let selected = selected_station_key_id.and_then(|id| {
        context
            .explanations
            .iter()
            .find(|candidate| candidate.station_key_id == id)
    });
    let reason = selected_station_key_id
        .and_then(|_| selected)
        .map(|candidate| {
            let reasons = if candidate.reasons.is_empty() {
                "matched route selector".to_string()
            } else {
                candidate.reasons.join("; ")
            };
            format!(
                "selected {} on {}: {}",
                candidate.key_name, candidate.station_name, reasons
            )
        })
        .unwrap_or_else(|| "no accepted route candidate".to_string());
    let rejected = context
        .explanations
        .iter()
        .filter(|candidate| !candidate.accepted)
        .map(|candidate| {
            json!({
                "stationKeyId": candidate.station_key_id,
                "stationId": candidate.station_id,
                "stationName": candidate.station_name,
                "keyName": candidate.key_name,
                "rejectionReasons": candidate.rejection_reasons,
            })
        })
        .collect::<Vec<_>>();
    let economic_context = json!({
        "selected": selected.map(|candidate| json!({
            "stationKeyId": candidate.station_key_id,
            "stationId": candidate.station_id,
            "stationName": candidate.station_name,
            "keyName": candidate.key_name,
            "pricingRuleId": candidate.pricing_rule_id,
            "groupBindingId": candidate.group_binding_id,
            "rateMultiplier": candidate.rate_multiplier,
            "normalizationStatus": candidate.normalization_status,
            "priceConfidence": candidate.price_confidence,
            "estimatedInputPrice": candidate.estimated_input_price,
            "estimatedOutputPrice": candidate.estimated_output_price,
            "priceCurrency": candidate.price_currency,
            "balanceStatus": candidate.balance_status,
            "balanceValue": candidate.balance_value,
            "balanceScope": candidate.balance_scope,
            "balanceCollectedAt": candidate.balance_collected_at,
            "economicFreshness": candidate.economic_freshness,
            "economicReasons": candidate.economic_reasons,
        })),
        "rejected": rejected,
    });

    RouteLogMetadata {
        policy: routing_policy_label(&context.policy).to_string(),
        reason,
        rejected_candidates_json: serde_json::to_string(&rejected)
            .unwrap_or_else(|_| "[]".to_string()),
        group_binding_id: selected.and_then(|candidate| candidate.group_binding_id.clone()),
        normalization_status: selected.and_then(|candidate| candidate.normalization_status.clone()),
        balance_scope: selected.and_then(|candidate| candidate.balance_scope.clone()),
        economic_context_json: serde_json::to_string(&economic_context)
            .unwrap_or_else(|_| "{}".to_string()),
    }
}

fn routing_policy_label(policy: &RoutingPolicy) -> &'static str {
    match policy {
        RoutingPolicy::AutomaticBalanced => "automatic_balanced",
        RoutingPolicy::PriorityFallback => "priority_fallback",
        RoutingPolicy::StableFirst => "stable_first",
        RoutingPolicy::BackupOnly => "backup_only",
        RoutingPolicy::CheapFirst => "cheap_first",
        RoutingPolicy::CostStableFirst => "cost_stable_first",
    }
}

fn rewrite_request_model(
    request: &ParsedRequest,
    body: &Value,
    client_model: Option<&str>,
    mapped_model: Option<&str>,
) -> ParsedRequest {
    let Some(mapped_model) = mapped_model else {
        return request.clone();
    };
    if client_model == Some(mapped_model) {
        return request.clone();
    }
    let mut body = body.clone();
    if let Some(object) = body.as_object_mut() {
        object.insert("model".to_string(), Value::String(mapped_model.to_string()));
    }
    ParsedRequest {
        method: request.method.clone(),
        path: request.path.clone(),
        headers: request.headers.clone(),
        body: serde_json::to_vec(&body).unwrap_or_else(|_| request.body.clone()),
    }
}

fn uses_tools(body: &Value) -> bool {
    body.get("tool_choice").is_some()
        || body
            .get("tools")
            .and_then(Value::as_array)
            .map(|tools| !tools.is_empty())
            .unwrap_or(false)
}

fn uses_vision(body: &Value) -> bool {
    match body {
        Value::Object(object) => {
            object.contains_key("image_url")
                || object.contains_key("input_image")
                || object
                    .get("type")
                    .and_then(Value::as_str)
                    .map(|value| value == "image" || value == "input_image")
                    .unwrap_or(false)
                || object.values().any(uses_vision)
        }
        Value::Array(items) => items.iter().any(uses_vision),
        _ => false,
    }
}

fn uses_reasoning(body: &Value, model: Option<&str>) -> bool {
    RequestObservation::from_json(body).uses_reasoning
        || model.map(|model| model.starts_with('o')).unwrap_or(false)
}

fn request_session_hash(request: &ParsedRequest, body: &Value) -> Option<String> {
    request
        .headers
        .get("x-relay-session-hash")
        .or_else(|| request.headers.get("x-session-id"))
        .and_then(|value| non_empty(value))
        .map(stable_session_hash)
        .or_else(|| {
            body.get("metadata")
                .and_then(|metadata| metadata.get("session_hash"))
                .and_then(Value::as_str)
                .and_then(non_empty)
                .map(stable_session_hash)
        })
        .or_else(|| {
            body.get("user")
                .and_then(Value::as_str)
                .and_then(non_empty)
                .map(stable_session_hash)
        })
}

fn previous_response_id(body: &Value) -> Option<String> {
    body.get("previous_response_id")
        .and_then(Value::as_str)
        .and_then(non_empty)
        .map(ToString::to_string)
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    if value.is_empty() {
        None
    } else {
        Some(value)
    }
}

fn stable_session_hash(value: &str) -> String {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn record_candidate_success(
    context: &ProxyServerContext,
    candidate: &RouteCandidate,
    status_label: &str,
    used_at: &str,
    checked_at: &str,
    duration_ms: i64,
) {
    let _ = context.database.touch_station_key_usage(
        &candidate.station_key_id,
        status_label,
        Some(used_at),
        Some(checked_at),
    );
    let _ = context.database.record_station_key_success(
        &candidate.station_key_id,
        duration_ms,
        checked_at,
    );
}

fn record_candidate_failure(
    context: &ProxyServerContext,
    candidate: &RouteCandidate,
    status_label: &str,
    checked_at: &str,
    error_summary: &str,
    failure_input: RouteFailureInput,
) {
    let classified = classify_route_failure(failure_input);
    let _ = context.database.touch_station_key_usage(
        &candidate.station_key_id,
        status_label,
        None,
        Some(checked_at),
    );
    if classified.scope == RouteFailureScope::RequestOnly
        || classified.action == RouteFailureAction::IgnoreForKeyHealth
    {
        return;
    }

    let current_health = context
        .database
        .get_station_key_health(candidate.station_key_id.clone())
        .ok();
    let consecutive_failures = current_health
        .as_ref()
        .map(|health| health.consecutive_failures + 1)
        .unwrap_or(1);
    let current_state = current_health
        .as_ref()
        .map(|health| route_health_state_from_record(health, checked_at))
        .unwrap_or(RouteHealthState::Unknown);
    let now_ms = checked_at
        .parse::<i64>()
        .unwrap_or_else(|_| now_millis_for_services() as i64);
    let transition =
        apply_health_transition(current_state, &classified, consecutive_failures, now_ms);
    let cooldown_until = transition
        .cooldown_until_ms
        .as_ref()
        .map(|value| value.to_string());
    let classified_error_summary = error_summary_for_failure(&classified, error_summary);

    let _ = context.database.record_station_key_failure_with_cooldown(
        &candidate.station_key_id,
        &classified_error_summary,
        checked_at,
        cooldown_until.as_deref(),
    );
}

fn route_health_state_from_record(
    health: &crate::models::routing::StationKeyHealth,
    now: &str,
) -> RouteHealthState {
    let now_ms = now.parse::<i64>().unwrap_or_default();
    if health
        .cooldown_until
        .as_deref()
        .and_then(|value| value.parse::<i64>().ok())
        .map(|cooldown_until| cooldown_until > now_ms)
        .unwrap_or(false)
    {
        return RouteHealthState::Cooldown;
    }
    if health.consecutive_failures > 0 {
        return RouteHealthState::Degraded;
    }
    if health.success_count > 0 || health.last_success_at.is_some() {
        return RouteHealthState::Ready;
    }
    RouteHealthState::Unknown
}

fn confirm_switch_candidate(
    context: &ProxyServerContext,
    candidate: &RouteCandidate,
    endpoint: RouteEndpointKind,
    mapped_model: Option<&str>,
    route_context: &RouteLogContext,
) -> Result<(), String> {
    if route_context
        .explanations
        .iter()
        .find(|explanation| explanation.station_key_id == candidate.station_key_id)
        .and_then(|explanation| explanation.balance_status.as_deref())
        == Some("depleted")
    {
        return Err("switch probe skipped because candidate balance is depleted".to_string());
    }

    let cache_key = ProbeCacheKey::new(
        candidate.station_key_id.clone(),
        endpoint.clone(),
        mapped_model,
    );
    let now_ms = now_millis_for_services() as i64;
    if context
        .probe_cache
        .lock()
        .map(|mut cache| cache.should_skip_probe(&cache_key, now_ms))
        .unwrap_or(false)
    {
        return Ok(());
    }

    let checked_at = now_string();
    match send_switch_probe(candidate, endpoint, mapped_model) {
        Ok(()) => {
            if let Ok(mut cache) = context.probe_cache.lock() {
                cache.record_pass(cache_key, now_ms);
            }
            Ok(())
        }
        Err(SwitchProbeError::Http {
            status,
            retry_after_ms,
        }) => {
            let error = format!("switch probe returned HTTP {status}");
            record_candidate_failure(
                context,
                candidate,
                "warning",
                &checked_at,
                &error,
                RouteFailureInput::http_status_with_retry_after(status, false, retry_after_ms),
            );
            Err(error)
        }
        Err(SwitchProbeError::Transport(error)) => {
            record_candidate_failure(
                context,
                candidate,
                "error",
                &checked_at,
                &error,
                RouteFailureInput::transport_error(false, &error),
            );
            Err(error)
        }
    }
}

enum SwitchProbeError {
    Http {
        status: u16,
        retry_after_ms: Option<i64>,
    },
    Transport(String),
}

fn send_switch_probe(
    candidate: &RouteCandidate,
    endpoint: RouteEndpointKind,
    mapped_model: Option<&str>,
) -> Result<(), SwitchProbeError> {
    let model = mapped_model.unwrap_or("gpt-4o-mini");
    match endpoint {
        RouteEndpointKind::Responses => {
            let body = responses_probe_body(model);
            let path = upstream_responses_path(&candidate.upstream_api_format);
            match send_probe_request(candidate, path, &body) {
                Ok(()) => Ok(()),
                Err(SwitchProbeError::Http { status, .. })
                    if matches!(status, 404 | 405 | 501)
                        && should_try_chat_fallback(&candidate.upstream_api_format) =>
                {
                    let body = chat_probe_body(model);
                    send_probe_request(candidate, "/v1/chat/completions", &body)
                }
                Err(error) => Err(error),
            }
        }
        RouteEndpointKind::ChatCompletions => {
            let body = chat_probe_body(model);
            send_probe_request(candidate, "/v1/chat/completions", &body)
        }
        RouteEndpointKind::Models | RouteEndpointKind::Embeddings => Ok(()),
    }
}

fn send_probe_request(
    candidate: &RouteCandidate,
    upstream_path: &str,
    body: &[u8],
) -> Result<(), SwitchProbeError> {
    let url = build_upstream_url(&candidate.upstream_base_url, upstream_path);
    let proxy = ProxyConfig {
        mode: candidate.collector_proxy_mode.clone(),
        url: candidate.collector_proxy_url.clone(),
    };
    let agent = agent_builder_for_proxy(&proxy)
        .map_err(SwitchProbeError::Transport)?
        .timeout(std::time::Duration::from_secs(10))
        .build();
    let result = agent
        .post(&url)
        .set("authorization", &format!("Bearer {}", candidate.api_key))
        .set("content-type", "application/json")
        .set("accept", "application/json")
        .send_bytes(body);

    match result {
        Ok(response) if response.status() < 400 => Ok(()),
        Ok(response) => Err(SwitchProbeError::Http {
            status: response.status(),
            retry_after_ms: parse_retry_after_ms(response.header("retry-after")),
        }),
        Err(ureq::Error::Status(status, response)) => Err(SwitchProbeError::Http {
            status,
            retry_after_ms: parse_retry_after_ms(response.header("retry-after")),
        }),
        Err(error) => Err(SwitchProbeError::Transport(redact_error_message(&format!(
            "{error}"
        )))),
    }
}

fn chat_probe_body(model: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": model,
        "messages": [{"role": "user", "content": "ping"}],
        "stream": false,
        "max_tokens": 1
    }))
    .unwrap_or_default()
}

fn responses_probe_body(model: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({
        "model": model,
        "input": "ping",
        "store": false,
        "max_output_tokens": 1
    }))
    .unwrap_or_default()
}

fn forward_chat_request(context: &ProxyServerContext, request: &ParsedRequest) -> ProxyResponse {
    let body_value: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return ProxyResponse::json_error(
                400,
                "bad_json",
                &format!("请求 JSON 无法解析: {error}"),
            );
        }
    };
    let (model, stream) = extract_chat_request_metadata(&body_value);
    let current_station_key_id = lookup_route_affinity(
        context,
        RouteEndpointKind::ChatCompletions,
        model.as_deref(),
    );
    let settings = context.database.get_settings().ok();
    let policy = settings
        .as_ref()
        .map(|settings| parse_routing_policy(&settings.default_routing_strategy))
        .unwrap_or(RoutingPolicy::PriorityFallback);
    let route_request = route_request_for_chat(
        request,
        model.clone(),
        stream,
        &body_value,
        policy,
        settings
            .as_ref()
            .and_then(|settings| settings.max_rate_multiplier),
        settings
            .as_ref()
            .map(|settings| settings.default_routing_group_filter.clone())
            .unwrap_or_default(),
        settings
            .as_ref()
            .map(|settings| settings.allow_depleted_fallback)
            .unwrap_or(false),
        current_station_key_id,
    );
    if matches!(route_request.policy, RoutingPolicy::AutomaticBalanced) {
        return forward_automatic_chat_request(
            context,
            request,
            &body_value,
            route_request,
            model,
            stream,
        );
    }
    let route = match select_proxy_route(context, &route_request) {
        Ok(route) => route,
        Err(response) => return response.with_request_meta(model, stream),
    };
    let route_context = route_log_context(&route_request.policy, &route.explanations);
    let request = rewrite_request_model(
        request,
        &body_value,
        model.as_deref(),
        route.mapped_model.as_deref(),
    );
    let probe_model = route.mapped_model.clone().or_else(|| model.clone());
    let candidates = route
        .accepted
        .into_iter()
        .map(|item| item.candidate)
        .collect::<Vec<_>>();
    forward_with_fallback(
        context,
        &request,
        &candidates,
        model,
        probe_model,
        stream,
        &route_context,
    )
}

fn forward_responses_request(
    context: &ProxyServerContext,
    request: &ParsedRequest,
) -> ProxyResponse {
    let body_value: Value = match serde_json::from_slice(&request.body) {
        Ok(value) => value,
        Err(error) => {
            return ProxyResponse::json_error(
                400,
                "bad_json",
                &format!("请求 JSON 无法解析: {error}"),
            );
        }
    };
    let (model, stream) = extract_responses_metadata(&body_value);
    let current_station_key_id =
        lookup_route_affinity(context, RouteEndpointKind::Responses, model.as_deref());
    let settings = context.database.get_settings().ok();
    let policy = settings
        .as_ref()
        .map(|settings| parse_routing_policy(&settings.default_routing_strategy))
        .unwrap_or(RoutingPolicy::PriorityFallback);
    let route_request = route_request_for_responses(
        request,
        model.clone(),
        stream,
        &body_value,
        policy,
        settings
            .as_ref()
            .and_then(|settings| settings.max_rate_multiplier),
        settings
            .as_ref()
            .map(|settings| settings.default_routing_group_filter.clone())
            .unwrap_or_default(),
        settings
            .as_ref()
            .map(|settings| settings.allow_depleted_fallback)
            .unwrap_or(false),
        current_station_key_id,
    );
    if matches!(route_request.policy, RoutingPolicy::AutomaticBalanced) {
        return forward_automatic_responses_request(
            context,
            request,
            &body_value,
            route_request,
            model,
            stream,
        );
    }
    let route = match select_proxy_route(context, &route_request) {
        Ok(route) => route,
        Err(response) => return response.with_request_meta(model, stream),
    };
    let route_context = route_log_context(&route_request.policy, &route.explanations);
    let request = rewrite_request_model(
        request,
        &body_value,
        model.as_deref(),
        route.mapped_model.as_deref(),
    );
    let probe_model = route.mapped_model.clone().or_else(|| model.clone());
    let body_value: Value = serde_json::from_slice(&request.body).unwrap_or(body_value);
    let candidates = route
        .accepted
        .into_iter()
        .map(|item| item.candidate)
        .collect::<Vec<_>>();
    forward_responses_with_fallback(
        context,
        &request,
        &candidates,
        &body_value,
        model,
        probe_model,
        stream,
        &route_context,
    )
}

fn forward_automatic_chat_request(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    body_value: &Value,
    mut route_request: RouteRequest,
    model: Option<String>,
    stream: bool,
) -> ProxyResponse {
    let mut excluded_key_ids = HashSet::new();
    let mut fallback_count = 0_i64;
    let mut last_error: Option<String>;
    let mut last_preflight_error: Option<ProxyResponse> = None;

    loop {
        route_request.excluded_key_ids = excluded_key_ids.iter().cloned().collect();
        route_request.now_ms = now_millis_for_services() as i64;
        let route = match select_proxy_route(context, &route_request) {
            Ok(route) => route,
            Err(response) => {
                let response = last_preflight_error.take().unwrap_or(response);
                return response
                    .with_fallback_count(fallback_count)
                    .with_request_meta(model, stream);
            }
        };
        let route_context = route_log_context(&route_request.policy, &route.explanations);
        let Some(selected) = route.accepted.first() else {
            return ProxyResponse::json_error(
                503,
                "routing_no_eligible_candidate",
                &format!(
                    "没有可用 Station Key 支持该请求：model={} endpoint={:?} stream={}",
                    route_request.model.as_deref().unwrap_or("<none>"),
                    route_request.endpoint,
                    route_request.stream
                ),
            )
            .with_fallback_count(fallback_count)
            .with_request_meta(model, stream)
            .with_route_metadata(route_log_metadata(&route_context, None));
        };
        let candidate = selected.candidate.clone();
        if let Err(response) = revalidate_automatic_candidate_before_forward(
            context,
            &route_request,
            &candidate.station_key_id,
        ) {
            last_preflight_error = Some(response);
            excluded_key_ids.insert(candidate.station_key_id);
            fallback_count += 1;
            if fallback_count > 32 {
                return ProxyResponse::json_error(
                    502,
                    "routing_all_upstreams_failed",
                    "自动路由转发前预检连续失败，已达到重试上限",
                )
                .with_fallback_count(fallback_count)
                .with_request_meta(model, stream)
                .with_route_metadata(route_log_metadata(&route_context, None));
            }
            continue;
        }
        let rewritten_request = rewrite_request_model(
            request,
            body_value,
            model.as_deref(),
            route.mapped_model.as_deref(),
        );
        let checked_at = now_string();
        let attempt_started = Instant::now();

        match forward_to_candidate(&rewritten_request, &candidate, stream) {
            Ok(response)
                if response.status_code < 400 || !should_fallback(response.status_code) =>
            {
                let routed_response = response
                    .with_candidate(&candidate)
                    .with_fallback_count(fallback_count)
                    .with_request_meta(model.clone(), stream)
                    .with_route_metadata(route_log_metadata(
                        &route_context,
                        Some(candidate.station_key_id.as_str()),
                    ));
                let status_label = routed_response.status_label.clone();
                if routed_response.status_code < 400 {
                    let request_cost =
                        extract_request_cost(context, &candidate, &routed_response, stream);
                    return routed_response
                        .with_request_cost(request_cost)
                        .with_pending_success(
                            &candidate,
                            RouteEndpointKind::ChatCompletions,
                            model.clone(),
                            attempt_started.elapsed().as_millis() as i64,
                            checked_at,
                        );
                }

                record_candidate_failure(
                    context,
                    &candidate,
                    &status_label,
                    &checked_at,
                    &format!("上游返回 HTTP {}", routed_response.status_code),
                    RouteFailureInput::http_status_with_retry_after(
                        routed_response.status_code,
                        false,
                        routed_response.retry_after_ms,
                    ),
                );
                let request_cost =
                    extract_request_cost(context, &candidate, &routed_response, stream);
                return routed_response.with_request_cost(request_cost);
            }
            Ok(response) => {
                let error = format!("上游返回 HTTP {}", response.status_code);
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    &candidate,
                    "warning",
                    &checked_at,
                    &error,
                    RouteFailureInput::http_status_with_retry_after(
                        response.status_code,
                        false,
                        response.retry_after_ms,
                    ),
                );
            }
            Err(error) => {
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    &candidate,
                    "error",
                    &checked_at,
                    &error,
                    RouteFailureInput::transport_error(false, &error),
                );
            }
        }

        excluded_key_ids.insert(candidate.station_key_id);
        fallback_count += 1;
        if fallback_count > 32 {
            return ProxyResponse::json_error(
                502,
                "routing_all_upstreams_failed",
                &format!(
                    "自动路由重试次数已达上限：{}",
                    last_error.unwrap_or_else(|| "未知错误".to_string())
                ),
            )
            .with_fallback_count(fallback_count)
            .with_request_meta(model, stream)
            .with_route_metadata(route_log_metadata(&route_context, None));
        }
    }
}

fn forward_automatic_responses_request(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    body_value: &Value,
    mut route_request: RouteRequest,
    model: Option<String>,
    stream: bool,
) -> ProxyResponse {
    let mut excluded_key_ids = HashSet::new();
    let mut fallback_count = 0_i64;
    let mut last_error: Option<String>;
    let mut last_preflight_error: Option<ProxyResponse> = None;

    loop {
        route_request.excluded_key_ids = excluded_key_ids.iter().cloned().collect();
        route_request.now_ms = now_millis_for_services() as i64;
        let route = match select_proxy_route(context, &route_request) {
            Ok(route) => route,
            Err(response) => {
                let response = last_preflight_error.take().unwrap_or(response);
                return response
                    .with_fallback_count(fallback_count)
                    .with_request_meta(model, stream);
            }
        };
        let route_context = route_log_context(&route_request.policy, &route.explanations);
        let Some(selected) = route.accepted.first() else {
            return ProxyResponse::json_error(
                503,
                "routing_no_eligible_candidate",
                &format!(
                    "没有可用 Station Key 支持该请求：model={} endpoint={:?} stream={}",
                    route_request.model.as_deref().unwrap_or("<none>"),
                    route_request.endpoint,
                    route_request.stream
                ),
            )
            .with_fallback_count(fallback_count)
            .with_request_meta(model, stream)
            .with_route_metadata(route_log_metadata(&route_context, None));
        };
        let candidate = selected.candidate.clone();
        if let Err(response) = revalidate_automatic_candidate_before_forward(
            context,
            &route_request,
            &candidate.station_key_id,
        ) {
            last_preflight_error = Some(response);
            excluded_key_ids.insert(candidate.station_key_id);
            fallback_count += 1;
            if fallback_count > 32 {
                return ProxyResponse::json_error(
                    502,
                    "routing_all_upstreams_failed",
                    "自动路由转发前预检连续失败，已达到重试上限",
                )
                .with_fallback_count(fallback_count)
                .with_request_meta(model, stream)
                .with_route_metadata(route_log_metadata(&route_context, None));
            }
            continue;
        }
        let rewritten_request = rewrite_request_model(
            request,
            body_value,
            model.as_deref(),
            route.mapped_model.as_deref(),
        );
        let rewritten_body: Value =
            serde_json::from_slice(&rewritten_request.body).unwrap_or_else(|_| body_value.clone());
        let checked_at = now_string();
        let attempt_started = Instant::now();

        match forward_responses_to_candidate(
            &rewritten_request,
            &candidate,
            &rewritten_body,
            route.mapped_model.as_deref().or(model.as_deref()),
            stream,
        ) {
            Ok(response)
                if response.status_code < 400 || !should_fallback(response.status_code) =>
            {
                let routed_response = response
                    .with_candidate(&candidate)
                    .with_fallback_count(fallback_count)
                    .with_request_meta(model.clone(), stream)
                    .with_route_metadata(route_log_metadata(
                        &route_context,
                        Some(candidate.station_key_id.as_str()),
                    ));
                let status_label = routed_response.status_label.clone();
                if routed_response.status_code < 400 {
                    let request_cost =
                        extract_request_cost(context, &candidate, &routed_response, false);
                    return routed_response
                        .with_request_cost(request_cost)
                        .with_pending_success(
                            &candidate,
                            RouteEndpointKind::Responses,
                            model.clone(),
                            attempt_started.elapsed().as_millis() as i64,
                            checked_at,
                        );
                }

                record_candidate_failure(
                    context,
                    &candidate,
                    &status_label,
                    &checked_at,
                    &format!("上游返回 HTTP {}", routed_response.status_code),
                    RouteFailureInput::http_status_with_retry_after(
                        routed_response.status_code,
                        false,
                        routed_response.retry_after_ms,
                    ),
                );
                let request_cost =
                    extract_request_cost(context, &candidate, &routed_response, false);
                return routed_response.with_request_cost(request_cost);
            }
            Ok(response) => {
                let error = format!("上游返回 HTTP {}", response.status_code);
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    &candidate,
                    "warning",
                    &checked_at,
                    &error,
                    RouteFailureInput::http_status_with_retry_after(
                        response.status_code,
                        false,
                        response.retry_after_ms,
                    ),
                );
            }
            Err(error) => {
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    &candidate,
                    "error",
                    &checked_at,
                    &error,
                    RouteFailureInput::transport_error(false, &error),
                );
            }
        }

        excluded_key_ids.insert(candidate.station_key_id);
        fallback_count += 1;
        if fallback_count > 32 {
            return ProxyResponse::json_error(
                502,
                "routing_all_upstreams_failed",
                &format!(
                    "自动路由重试次数已达上限：{}",
                    last_error.unwrap_or_else(|| "未知错误".to_string())
                ),
            )
            .with_fallback_count(fallback_count)
            .with_request_meta(model, stream)
            .with_route_metadata(route_log_metadata(&route_context, None));
        }
    }
}

fn forward_responses_with_fallback(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    candidates: &[RouteCandidate],
    body_value: &Value,
    model: Option<String>,
    probe_model: Option<String>,
    stream: bool,
    route_context: &RouteLogContext,
) -> ProxyResponse {
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if index > 0 {
            if let Err(error) = confirm_switch_candidate(
                context,
                candidate,
                RouteEndpointKind::Responses,
                probe_model.as_deref(),
                route_context,
            ) {
                last_error = Some(error);
                continue;
            }
        }
        let checked_at = now_string();
        let attempt_started = Instant::now();
        match forward_responses_to_candidate(
            request,
            candidate,
            body_value,
            model.as_deref(),
            stream,
        ) {
            Ok(response)
                if response.status_code < 400 || !should_fallback(response.status_code) =>
            {
                let routed_response = response
                    .with_candidate(candidate)
                    .with_fallback_count(index as i64)
                    .with_request_meta(model.clone(), stream)
                    .with_route_metadata(route_log_metadata(
                        route_context,
                        Some(candidate.station_key_id.as_str()),
                    ));
                let status_label = routed_response.status_label.clone();
                if routed_response.status_code < 400 {
                    let request_cost =
                        extract_request_cost(context, candidate, &routed_response, false);
                    return routed_response
                        .with_request_cost(request_cost)
                        .with_pending_success(
                            candidate,
                            RouteEndpointKind::Responses,
                            model.clone(),
                            attempt_started.elapsed().as_millis() as i64,
                            checked_at,
                        );
                } else {
                    record_candidate_failure(
                        context,
                        candidate,
                        &status_label,
                        &checked_at,
                        &format!("上游返回 HTTP {}", routed_response.status_code),
                        RouteFailureInput::http_status_with_retry_after(
                            routed_response.status_code,
                            false,
                            routed_response.retry_after_ms,
                        ),
                    );
                }
                let request_cost =
                    extract_request_cost(context, candidate, &routed_response, false);
                return routed_response.with_request_cost(request_cost);
            }
            Ok(response) => {
                let error = format!("上游返回 HTTP {}", response.status_code);
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    candidate,
                    "warning",
                    &checked_at,
                    &error,
                    RouteFailureInput::http_status_with_retry_after(
                        response.status_code,
                        false,
                        response.retry_after_ms,
                    ),
                );
            }
            Err(error) => {
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    candidate,
                    "error",
                    &checked_at,
                    &error,
                    RouteFailureInput::transport_error(false, &error),
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
    .with_route_metadata(route_log_metadata(route_context, None))
}

fn forward_responses_to_candidate(
    request: &ParsedRequest,
    candidate: &RouteCandidate,
    body_value: &Value,
    fallback_model: Option<&str>,
    stream: bool,
) -> Result<ProxyResponse, String> {
    let direct_response = forward_to_candidate_with_body(
        request,
        candidate,
        upstream_responses_path(&candidate.upstream_api_format),
        request.body.as_slice(),
        stream,
    )?;

    if direct_response.status_code < 400 {
        if stream {
            return Ok(direct_response);
        }
        return Ok(render_responses_proxy_response(
            direct_response,
            fallback_model,
        ));
    }

    if !stream
        && matches!(direct_response.status_code, 404 | 405 | 501)
        && should_try_chat_fallback(&candidate.upstream_api_format)
    {
        let normalized = normalize_responses_request(body_value);
        let chat_request = ParsedRequest {
            method: request.method.clone(),
            path: "/v1/chat/completions".to_string(),
            headers: request.headers.clone(),
            body: serde_json::to_vec(&normalized).unwrap_or_default(),
        };
        let chat_response = forward_to_candidate(&chat_request, candidate, false)?;
        if chat_response.status_code < 400 {
            return Ok(render_responses_proxy_response(
                chat_response,
                fallback_model,
            ));
        }
        return Ok(chat_response);
    }

    Ok(direct_response)
}

fn render_responses_proxy_response(
    response: ProxyResponse,
    fallback_model: Option<&str>,
) -> ProxyResponse {
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
        retry_after_ms: response.retry_after_ms,
        status_label: response.status_label,
        error_message: response.error_message,
        route_policy: response.route_policy,
        route_reason: response.route_reason,
        rejected_candidates_json: response.rejected_candidates_json,
        group_binding_id: response.group_binding_id,
        normalization_status: response.normalization_status,
        balance_scope: response.balance_scope,
        economic_context_json: response.economic_context_json,
        request_cost: response.request_cost,
        pending_successes: response.pending_successes,
    }
}

fn forward_with_fallback(
    context: &ProxyServerContext,
    request: &ParsedRequest,
    candidates: &[RouteCandidate],
    model: Option<String>,
    probe_model: Option<String>,
    stream: bool,
    route_context: &RouteLogContext,
) -> ProxyResponse {
    let mut last_error = None;
    for (index, candidate) in candidates.iter().enumerate() {
        if index > 0 {
            if let Err(error) = confirm_switch_candidate(
                context,
                candidate,
                RouteEndpointKind::ChatCompletions,
                probe_model.as_deref(),
                route_context,
            ) {
                last_error = Some(error);
                continue;
            }
        }
        let checked_at = now_string();
        let attempt_started = Instant::now();
        match forward_to_candidate(request, candidate, stream) {
            Ok(response)
                if response.status_code < 400 || !should_fallback(response.status_code) =>
            {
                let routed_response = response
                    .with_candidate(candidate)
                    .with_fallback_count(index as i64)
                    .with_request_meta(model.clone(), stream)
                    .with_route_metadata(route_log_metadata(
                        route_context,
                        Some(candidate.station_key_id.as_str()),
                    ));
                let status_label = routed_response.status_label.clone();
                if routed_response.status_code < 400 {
                    let request_cost =
                        extract_request_cost(context, candidate, &routed_response, stream);
                    return routed_response
                        .with_request_cost(request_cost)
                        .with_pending_success(
                            candidate,
                            RouteEndpointKind::ChatCompletions,
                            model.clone(),
                            attempt_started.elapsed().as_millis() as i64,
                            checked_at,
                        );
                } else {
                    record_candidate_failure(
                        context,
                        candidate,
                        &status_label,
                        &checked_at,
                        &format!("上游返回 HTTP {}", routed_response.status_code),
                        RouteFailureInput::http_status_with_retry_after(
                            routed_response.status_code,
                            false,
                            routed_response.retry_after_ms,
                        ),
                    );
                }
                let request_cost =
                    extract_request_cost(context, candidate, &routed_response, stream);
                return routed_response.with_request_cost(request_cost);
            }
            Ok(response) => {
                let error = format!("上游返回 HTTP {}", response.status_code);
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    candidate,
                    "warning",
                    &checked_at,
                    &error,
                    RouteFailureInput::http_status_with_retry_after(
                        response.status_code,
                        false,
                        response.retry_after_ms,
                    ),
                );
            }
            Err(error) => {
                last_error = Some(error.clone());
                record_candidate_failure(
                    context,
                    candidate,
                    "error",
                    &checked_at,
                    &error,
                    RouteFailureInput::transport_error(false, &error),
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
    .with_route_metadata(route_log_metadata(route_context, None))
}

fn forward_to_candidate(
    request: &ParsedRequest,
    candidate: &RouteCandidate,
    stream: bool,
) -> Result<ProxyResponse, String> {
    forward_to_candidate_with_body(
        request,
        candidate,
        &request.path,
        request.body.as_slice(),
        stream,
    )
}

fn forward_to_candidate_with_body(
    request: &ParsedRequest,
    candidate: &RouteCandidate,
    upstream_path: &str,
    body: &[u8],
    stream: bool,
) -> Result<ProxyResponse, String> {
    let url = build_upstream_url(&candidate.upstream_base_url, upstream_path);
    let proxy = ProxyConfig {
        mode: candidate.collector_proxy_mode.clone(),
        url: candidate.collector_proxy_url.clone(),
    };
    let agent = agent_builder_for_proxy(&proxy)?
        .timeout(std::time::Duration::from_secs(45))
        .build();
    let mut upstream = agent
        .request(&request.method, &url)
        .set("authorization", &format!("Bearer {}", candidate.api_key))
        .set("content-type", content_type(request));
    if let Some(accept) = request.headers.get("accept") {
        upstream = upstream.set("accept", accept);
    } else if stream {
        upstream = upstream.set("accept", "text/event-stream");
    } else if request.path == "/v1/responses" {
        upstream = upstream.set("accept", "application/json");
    }

    let result = if body.is_empty() {
        upstream.call()
    } else {
        upstream.send_bytes(body)
    };

    match result {
        Ok(response) => Ok(response_from_upstream(response, stream)),
        Err(ureq::Error::Status(_, response)) => Ok(response_from_upstream(response, false)),
        Err(error) => Err(redact_error_message(&format!("{error}"))),
    }
}

fn cors_preflight_response() -> ProxyResponse {
    ProxyResponse {
        status_code: 204,
        content_type: "text/plain".to_string(),
        body: ProxyResponseBody::Buffered(Vec::new()),
        model: None,
        stream: false,
        station_key_id: None,
        station_id: None,
        upstream_base_url: None,
        fallback_count: 0,
        retry_after_ms: None,
        status_label: "success".to_string(),
        error_message: None,
        route_policy: None,
        route_reason: None,
        rejected_candidates_json: None,
        group_binding_id: None,
        normalization_status: None,
        balance_scope: None,
        economic_context_json: None,
        request_cost: crate::services::pricing::request_cost_unknown(),
        pending_successes: Vec::new(),
    }
}

fn response_from_upstream(response: ureq::Response, stream: bool) -> ProxyResponse {
    let status_code = response.status();
    let content_type = response
        .header("content-type")
        .unwrap_or("application/json")
        .to_string();
    let retry_after_ms = parse_retry_after_ms(response.header("retry-after"));
    if stream && status_code < 400 {
        return ProxyResponse {
            status_code,
            content_type,
            body: ProxyResponseBody::Streamed(Box::new(response.into_reader())),
            model: None,
            stream: true,
            station_key_id: None,
            station_id: None,
            upstream_base_url: None,
            fallback_count: 0,
            retry_after_ms,
            status_label: "success".to_string(),
            error_message: None,
            route_policy: None,
            route_reason: None,
            rejected_candidates_json: None,
            group_binding_id: None,
            normalization_status: None,
            balance_scope: None,
            economic_context_json: None,
            request_cost: crate::services::pricing::request_cost_unknown(),
            pending_successes: Vec::new(),
        };
    }
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
        retry_after_ms,
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
        route_policy: None,
        route_reason: None,
        rejected_candidates_json: None,
        group_binding_id: None,
        normalization_status: None,
        balance_scope: None,
        economic_context_json: None,
        request_cost: crate::services::pricing::request_cost_unknown(),
        pending_successes: Vec::new(),
    }
}

fn extract_request_cost(
    context: &ProxyServerContext,
    candidate: &RouteCandidate,
    response: &ProxyResponse,
    stream: bool,
) -> RequestCostEstimate {
    if stream {
        return crate::services::pricing::request_cost_unknown();
    }
    let Some(body) = response.body_bytes() else {
        return crate::services::pricing::request_cost_unknown();
    };
    let Ok(value) = serde_json::from_slice::<Value>(body) else {
        return crate::services::pricing::request_cost_unknown();
    };
    let Some(usage) = ObservedUsage::from_json(&value) else {
        return crate::services::pricing::request_cost_unknown();
    };

    request_cost_for_observed_usage(
        context,
        Some(&candidate.station_key_id),
        Some(&candidate.station_id),
        response.model.as_deref(),
        &usage,
    )
}

fn request_cost_for_observed_usage(
    context: &ProxyServerContext,
    station_key_id: Option<&str>,
    station_id: Option<&str>,
    model: Option<&str>,
    usage: &ObservedUsage,
) -> RequestCostEstimate {
    let economics = station_key_id.and_then(|station_key_id| {
        context
            .database
            .route_candidate_economics_for_model(
                station_key_id.to_string(),
                model.map(ToString::to_string),
            )
            .ok()
            .flatten()
    });
    let request_usage = RequestUsage {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        total_tokens: usage.total_tokens,
        request_count: Some(1),
        cache_creation_tokens: usage.cache_creation_tokens,
        cache_read_tokens: usage.cache_read_tokens,
        media_count: None,
        duration_seconds: None,
        size_tier: None,
    };
    crate::services::pricing::request_cost_from_pricing_parts_and_usage(
        economics
            .as_ref()
            .and_then(|economics| station_key_id.map(|station_key_id| (economics, station_key_id)))
            .map(
                |(economics, station_key_id)| crate::services::pricing::RequestPricingParts {
                    station_key_id,
                    station_id,
                    model,
                    pricing_rule_id: economics.pricing_rule_id.as_deref(),
                    pricing_model: economics.pricing_model.as_deref(),
                    group_binding_id: economics.group_binding_id.as_deref(),
                    rate_multiplier: economics.rate_multiplier,
                    normalization_status: economics.normalization_status.as_deref(),
                    price_confidence: economics.price_confidence,
                    base_input_price: economics.base_input_price,
                    base_output_price: economics.base_output_price,
                    base_fixed_price: economics.base_fixed_price,
                    estimated_input_price: economics.estimated_input_price,
                    estimated_output_price: economics.estimated_output_price,
                    fixed_price: economics.fixed_price,
                    price_currency: economics.price_currency.as_deref(),
                    pricing_source: economics.pricing_source.as_deref(),
                    collected_at: economics.balance_collected_at.as_deref(),
                },
            ),
        &request_usage,
    )
}

impl ProxyResponse {
    fn body_bytes(&self) -> Option<&[u8]> {
        match &self.body {
            ProxyResponseBody::Buffered(bytes) => Some(bytes.as_slice()),
            ProxyResponseBody::Streamed(_) => None,
        }
    }

    fn json_error(status_code: u16, code: &str, message: &str) -> Self {
        let body =
            serde_json::to_vec(&openai_error(message, code)).unwrap_or_else(|_| b"{}".to_vec());
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
            retry_after_ms: None,
            status_label: "failed".to_string(),
            error_message: Some(redact_error_message(message)),
            route_policy: None,
            route_reason: None,
            rejected_candidates_json: None,
            group_binding_id: None,
            normalization_status: None,
            balance_scope: None,
            economic_context_json: None,
            request_cost: crate::services::pricing::request_cost_unknown(),
            pending_successes: Vec::new(),
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

    fn with_route_metadata(mut self, metadata: RouteLogMetadata) -> Self {
        self.route_policy = Some(metadata.policy);
        self.route_reason = Some(metadata.reason);
        self.rejected_candidates_json = Some(metadata.rejected_candidates_json);
        self.group_binding_id = metadata.group_binding_id;
        self.normalization_status = metadata.normalization_status;
        self.balance_scope = metadata.balance_scope;
        self.economic_context_json = Some(metadata.economic_context_json);
        self
    }

    fn with_request_cost(mut self, request_cost: RequestCostEstimate) -> Self {
        self.request_cost = request_cost;
        self
    }

    fn with_pending_success(
        mut self,
        candidate: &RouteCandidate,
        endpoint: RouteEndpointKind,
        model: Option<String>,
        duration_ms: i64,
        checked_at: String,
    ) -> Self {
        self.pending_successes.push(PendingCandidateSuccess {
            station_key_id: candidate.station_key_id.clone(),
            status_label: self.status_label.clone(),
            checked_at,
            duration_ms,
            endpoint,
            model,
        });
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

fn write_http_response(
    stream: &mut TcpStream,
    response: ProxyResponse,
    started: Instant,
) -> ResponseWriteOutcome {
    let reason = reason_phrase(response.status_code);
    match response.body {
        ProxyResponseBody::Buffered(body) => {
            let usage = serde_json::from_slice::<Value>(&body)
                .ok()
                .and_then(|value| ObservedUsage::from_json(&value));
            let header = format!(
                "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncontent-length: {}\r\naccess-control-allow-origin: *\r\naccess-control-allow-methods: GET, POST, OPTIONS\r\naccess-control-allow-headers: authorization, content-type, accept\r\nconnection: close\r\n\r\n",
                response.status_code,
                reason,
                response.content_type,
                body.len()
            );
            let error_message = stream
                .write_all(header.as_bytes())
                .and_then(|_| stream.write_all(&body))
                .err()
                .map(|error| format!("写入响应失败: {error}"));
            ResponseWriteOutcome {
                first_token_ms: None,
                usage,
                error_message,
            }
        }
        ProxyResponseBody::Streamed(mut body) => {
            let header = format!(
                "HTTP/1.1 {} {}\r\ncontent-type: {}\r\ncache-control: no-cache\r\naccess-control-allow-origin: *\r\naccess-control-allow-methods: GET, POST, OPTIONS\r\naccess-control-allow-headers: authorization, content-type, accept\r\nconnection: close\r\n\r\n",
                response.status_code,
                reason,
                response.content_type,
            );
            let mut observer = SseUsageObserver::default();
            let mut first_token_ms = None;
            let mut error_message = stream
                .write_all(header.as_bytes())
                .err()
                .map(|error| format!("写入流式响应失败: {error}"));
            let mut buffer = [0_u8; 8192];
            while error_message.is_none() {
                let count = match body.read(&mut buffer) {
                    Ok(count) => count,
                    Err(error) => {
                        error_message = Some(format!("读取流式响应失败: {error}"));
                        break;
                    }
                };
                if count == 0 {
                    break;
                }
                observer.push(&buffer[..count]);
                if let Err(error) = stream.write_all(&buffer[..count]) {
                    error_message = Some(format!("写入流式响应失败: {error}"));
                    break;
                }
                if first_token_ms.is_none() {
                    first_token_ms = Some(started.elapsed().as_millis() as i64);
                }
            }
            if error_message.is_none() {
                error_message = stream
                    .flush()
                    .err()
                    .map(|error| format!("写入流式响应失败: {error}"));
            }
            ResponseWriteOutcome {
                first_token_ms,
                usage: observer.usage().cloned(),
                error_message,
            }
        }
    }
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

fn parse_retry_after_ms(value: Option<&str>) -> Option<i64> {
    let trimmed = value?.trim();
    let seconds = trimmed.parse::<i64>().ok()?;
    Some(seconds.max(1) * 1000)
}

fn reason_phrase(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{
        pricing::{UpsertBalanceSnapshotInput, UpsertPricingRuleInput},
        routing::{UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput},
        settings::UpdateSettingsInput,
        station_keys::{StationKey, UpdateStationKeyInput},
        stations::CreateStationInput,
    };
    use std::{
        io::Read,
        net::TcpListener,
        sync::atomic::{AtomicU32, AtomicU64},
        thread,
        time::Duration,
    };

    #[test]
    fn proxy_status_reports_localhost_bind_only() {
        let proxy = ProxyRuntimeState::default();
        let status = proxy.status(8787);

        assert_eq!(status.bind_addr, "127.0.0.1");
        assert_ne!(status.bind_addr, "0.0.0.0");
    }

    #[test]
    fn cleanup_before_update_stops_the_running_proxy() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let probe = TcpListener::bind(("127.0.0.1", 0)).expect("bind probe");
        let port = probe.local_addr().expect("probe addr").port();
        drop(probe);
        let proxy = ProxyRuntimeState::default();

        let started = proxy
            .start(
                database,
                crate::services::secrets::crypto::generate_data_key(),
                port,
            )
            .expect("start proxy");
        assert!(started.running);

        let stopped = proxy
            .cleanup_before_update(port)
            .expect("cleanup before update");

        assert!(!stopped.running);
        assert!(TcpStream::connect(("127.0.0.1", port)).is_err());
    }

    #[test]
    fn write_http_response_supports_streamed_bodies_without_content_length() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("local addr").port();

        let handle = thread::spawn(move || {
            let (mut server_stream, _) = listener.accept().expect("accept");
            let response = ProxyResponse {
                status_code: 200,
                content_type: "text/event-stream".to_string(),
                body: ProxyResponseBody::Streamed(Box::new(std::io::Cursor::new(
                    b"data: {\"choices\":[],\"usage\":{\"prompt_tokens\":9,\"completion_tokens\":4,\"prompt_tokens_details\":{\"cached_tokens\":3}}}\n\ndata: [DONE]\n\n".to_vec(),
                ))),
                model: Some("gpt-5.4".to_string()),
                stream: true,
                station_key_id: Some("key-1".to_string()),
                station_id: Some("station-1".to_string()),
                upstream_base_url: Some("https://example.test/v1".to_string()),
                fallback_count: 0,
                retry_after_ms: None,
                status_label: "success".to_string(),
                error_message: None,
                route_policy: None,
                route_reason: None,
                rejected_candidates_json: None,
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: None,
                request_cost: crate::services::pricing::request_cost_unknown(),
                pending_successes: Vec::new(),
            };
            let outcome = write_http_response(&mut server_stream, response, Instant::now());
            assert!(outcome.error_message.is_none());
            assert!(outcome.first_token_ms.is_some());
            let usage = outcome.usage.expect("stream usage");
            assert_eq!(usage.input_tokens, Some(9));
            assert_eq!(usage.output_tokens, Some(4));
            assert_eq!(usage.cache_read_tokens, Some(3));
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("timeout");

        let mut buf = Vec::new();
        client.read_to_end(&mut buf).expect("read all");
        handle.join().expect("join");

        let text = String::from_utf8(buf).expect("utf8");
        assert!(text.contains("content-type: text/event-stream"));
        assert!(!text.contains("content-length:"));
        assert!(text.contains("\"prompt_tokens\":9"));
    }

    #[test]
    fn request_log_finalizes_after_buffered_response_write() {
        let response = ProxyResponse::json_error(400, "bad_request", "bad input");
        let started = Instant::now() - Duration::from_millis(25);

        let input = build_request_log_input(
            "POST".to_string(),
            "/v1/chat/completions".to_string(),
            RequestLogSnapshot::from_response(
                &response,
                crate::services::proxy::observability::RequestObservation {
                    reasoning_effort: Some("high".to_string()),
                    uses_reasoning: true,
                },
            ),
            "1000".to_string(),
            started,
            &ResponseWriteOutcome {
                first_token_ms: None,
                usage: Some(ObservedUsage {
                    input_tokens: Some(9),
                    output_tokens: Some(4),
                    total_tokens: Some(13),
                    cache_creation_tokens: None,
                    cache_read_tokens: Some(3),
                }),
                error_message: None,
            },
        );

        assert_eq!(input.status, "failed");
        assert_eq!(input.reasoning_effort.as_deref(), Some("high"));
        assert_eq!(input.prompt_tokens, Some(9));
        assert_eq!(input.completion_tokens, Some(4));
        assert_eq!(input.total_tokens, Some(13));
        assert_eq!(input.cache_read_tokens, Some(3));
        assert!(input.finished_at.is_some());
        assert!(input.duration_ms.is_some_and(|duration| duration >= 25));
    }

    #[test]
    fn stream_write_failure_marks_request_interrupted() {
        let response = ProxyResponse {
            status_code: 200,
            content_type: "text/event-stream".to_string(),
            body: ProxyResponseBody::Streamed(Box::new(std::io::Cursor::new(
                b"data: {\"id\":\"evt-1\"}\n\n".to_vec(),
            ))),
            model: Some("gpt-5.4".to_string()),
            stream: true,
            station_key_id: Some("key-1".to_string()),
            station_id: Some("station-1".to_string()),
            upstream_base_url: Some("https://example.test/v1".to_string()),
            fallback_count: 0,
            retry_after_ms: None,
            status_label: "success".to_string(),
            error_message: None,
            route_policy: Some("cost_stable_first".to_string()),
            route_reason: Some("selected key-1".to_string()),
            rejected_candidates_json: Some("[]".to_string()),
            group_binding_id: None,
            normalization_status: None,
            balance_scope: None,
            economic_context_json: None,
            request_cost: crate::services::pricing::request_cost_unknown(),
            pending_successes: Vec::new(),
        };
        let write_result = ResponseWriteOutcome {
            first_token_ms: Some(123),
            usage: Some(ObservedUsage {
                input_tokens: Some(5),
                output_tokens: Some(2),
                total_tokens: Some(7),
                cache_creation_tokens: None,
                cache_read_tokens: Some(3),
            }),
            error_message: Some("写入流式响应失败: connection reset".to_string()),
        };

        let input = build_request_log_input(
            "POST".to_string(),
            "/v1/chat/completions".to_string(),
            RequestLogSnapshot::from_response(&response, RequestObservation::default()),
            "1000".to_string(),
            Instant::now(),
            &write_result,
        );

        assert_eq!(input.status, "interrupted");
        assert_eq!(input.first_token_ms, Some(123));
        assert_eq!(input.cache_read_tokens, Some(3));
        assert_eq!(input.fallback_count, 0);
        assert!(input
            .error_message
            .as_deref()
            .is_some_and(|message| message.contains("connection reset")));
    }

    #[test]
    fn response_from_upstream_marks_success_as_streamed_when_requested() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("local addr").port();

        let handle = thread::spawn(move || {
            let (mut server_stream, _) = listener.accept().expect("accept");
            let mut buf = [0_u8; 1024];
            let _ = server_stream.read(&mut buf);
            let body = b"data: hello\n\n";
            let header = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            server_stream.write_all(header.as_bytes()).expect("header");
            server_stream.write_all(body).expect("body");
        });

        let response = ureq::get(&format!("http://127.0.0.1:{port}/v1/chat/completions"))
            .call()
            .expect("request");

        let proxy_response = response_from_upstream(response, true);
        match proxy_response.body {
            ProxyResponseBody::Streamed(_) => {}
            ProxyResponseBody::Buffered(_) => panic!("expected streamed body"),
        }

        handle.join().expect("join");
    }

    #[test]
    fn forward_chat_request_preserves_stream_metadata_for_sse_success() {
        let upstream = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let upstream_port = upstream.local_addr().expect("upstream addr").port();

        let handle = thread::spawn(move || {
            let (mut server_stream, _) = upstream.accept().expect("accept upstream");
            let _ = server_stream.set_read_timeout(Some(Duration::from_millis(200)));
            let mut buf = [0_u8; 2048];
            let mut read = 0;
            let deadline = std::time::Instant::now() + Duration::from_millis(200);
            loop {
                match server_stream.read(&mut buf[read..]) {
                    Ok(0) => break,
                    Ok(count) => {
                        read += count;
                        if read >= buf.len() {
                            break;
                        }
                    }
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            || error.kind() == std::io::ErrorKind::TimedOut =>
                    {
                        if read > 0 || std::time::Instant::now() >= deadline {
                            break;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("read upstream request: {error}"),
                }
            }
            let body = b"data: {\"choices\":[]}\n\ndata: [DONE]\n\n";
            let header = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                body.len()
            );
            server_stream.write_all(header.as_bytes()).expect("header");
            server_stream.write_all(body).expect("body");
        });

        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        database
            .create_station(CreateStationInput {
                name: "Streaming station".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: format!("http://127.0.0.1:{upstream_port}"),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-test-streaming".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let context = ProxyServerContext {
            database,
            data_key: crate::services::secrets::crypto::generate_data_key(),
            active_requests: Arc::new(AtomicU32::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            route_affinity: Arc::new(Mutex::new(RouteAffinityStore::default())),
            probe_cache: Arc::new(Mutex::new(ProbeConfirmationCache::default())),
        };
        let request = ParsedRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: serde_json::to_vec(&serde_json::json!({
                "model": "gpt-5.4",
                "messages": [{"role": "user", "content": "ping"}],
                "stream": true
            }))
            .expect("body"),
        };

        let response = forward_chat_request(&context, &request);

        assert_eq!(response.status_code, 200);
        assert!(
            response.stream,
            "request log metadata should record stream=true"
        );
        assert_eq!(response.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(response.content_type, "text/event-stream");
        assert!(matches!(response.body, ProxyResponseBody::Streamed(_)));

        context.active_requests.store(0, Ordering::Relaxed);
        context.request_count.store(0, Ordering::Relaxed);
        handle.join().expect("join upstream");
    }

    #[test]
    fn forward_responses_request_streams_with_sse_accept_header() {
        let upstream = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let upstream_port = upstream.local_addr().expect("upstream addr").port();

        let handle = thread::spawn(move || {
            for _ in 0..2 {
                let (mut server_stream, _) = upstream.accept().expect("accept upstream");
                let _ = server_stream.set_read_timeout(Some(Duration::from_millis(1000)));
                let mut buf = [0_u8; 4096];
                let mut read = 0;
                let deadline = std::time::Instant::now() + Duration::from_millis(1000);
                loop {
                    match server_stream.read(&mut buf[read..]) {
                        Ok(0) => break,
                        Ok(count) => {
                            read += count;
                            if read >= buf.len() {
                                break;
                            }
                            if String::from_utf8_lossy(&buf[..read])
                                .to_lowercase()
                                .contains("\r\n\r\n")
                            {
                                break;
                            }
                        }
                        Err(error)
                            if error.kind() == std::io::ErrorKind::WouldBlock
                                || error.kind() == std::io::ErrorKind::TimedOut =>
                        {
                            if read > 0 || std::time::Instant::now() >= deadline {
                                break;
                            }
                            thread::sleep(Duration::from_millis(5));
                        }
                        Err(error) => panic!("read upstream request: {error}"),
                    }
                }
                let request_text = String::from_utf8_lossy(&buf[..read]).to_lowercase();
                if !request_text.contains("accept: text/event-stream") {
                    let body = b"{\"id\":\"probe-ok\",\"output_text\":\"pong\"}";
                    let header = format!(
                        "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        body.len()
                    );
                    server_stream.write_all(header.as_bytes()).expect("header");
                    server_stream.write_all(body).expect("body");
                    continue;
                }

                let body = b"event: response.output_text.delta\ndata: {\"delta\":\"pong\"}\n\ndata: [DONE]\n\n";
                let header = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: text/event-stream\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                server_stream.write_all(header.as_bytes()).expect("header");
                server_stream.write_all(body).expect("body");
                return;
            }
        });

        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        database
            .create_station(CreateStationInput {
                name: "Responses streaming station".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: format!("http://127.0.0.1:{upstream_port}"),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-test-responses-streaming".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let context = ProxyServerContext {
            database,
            data_key: crate::services::secrets::crypto::generate_data_key(),
            active_requests: Arc::new(AtomicU32::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            route_affinity: Arc::new(Mutex::new(RouteAffinityStore::default())),
            probe_cache: Arc::new(Mutex::new(ProbeConfirmationCache::default())),
        };
        let request = ParsedRequest {
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: serde_json::to_vec(&serde_json::json!({
                "model": "gpt-5.4",
                "input": "ping",
                "stream": true
            }))
            .expect("body"),
        };

        let response = forward_responses_request(&context, &request);

        assert_eq!(response.status_code, 200);
        assert!(response.stream);
        assert_eq!(response.model.as_deref(), Some("gpt-5.4"));
        assert_eq!(response.content_type, "text/event-stream");
        assert!(matches!(response.body, ProxyResponseBody::Streamed(_)));

        handle.join().expect("join upstream");
    }

    #[test]
    fn chat_request_skips_key_that_does_not_allow_model() {
        let skipped = test_upstream_json_success("skipped", false);
        let allowed = test_upstream_json_success("allowed", false);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key_a = create_test_station_key(&database, "blocked-model", &skipped.base_url);
        let key_b = create_test_station_key(&database, "allowed-model", &allowed.base_url);
        database
            .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
            .expect("reorder");

        database
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: key_a.id.clone(),
                model_allowlist: vec!["other-model".to_string()],
                ..default_capabilities_input(key_a.id.clone())
            })
            .expect("blocked capabilities");
        database
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: key_b.id.clone(),
                model_allowlist: vec!["gpt-5.4".to_string()],
                ..default_capabilities_input(key_b.id.clone())
            })
            .expect("allowed capabilities");

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));

        assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
        assert_eq!(response.status_code, 200);
        assert!(
            !skipped.was_called(),
            "blocked key should be skipped before network"
        );
        allowed.join();
        skipped.join();
    }

    #[test]
    fn responses_request_skips_chat_only_key() {
        let skipped = test_upstream_json_success("chat-only", false);
        let allowed = test_upstream_json_success("responses", false);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key_a = create_test_station_key(&database, "chat-only", &skipped.base_url);
        let key_b = create_test_station_key(&database, "responses", &allowed.base_url);
        database
            .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
            .expect("reorder");

        database
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: key_a.id.clone(),
                supports_responses: false,
                supports_chat_completions: true,
                ..default_capabilities_input(key_a.id.clone())
            })
            .expect("chat-only capabilities");
        database
            .update_station_key_capabilities(UpdateStationKeyCapabilitiesInput {
                station_key_id: key_b.id.clone(),
                supports_responses: true,
                ..default_capabilities_input(key_b.id.clone())
            })
            .expect("responses capabilities");

        let context = proxy_context(database);
        let response = forward_responses_request(&context, &responses_request("gpt-5.4", false));

        assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
        assert_eq!(response.status_code, 200);
        assert!(
            !skipped.was_called(),
            "chat-only key should be skipped before network"
        );
        allowed.join();
        skipped.join();
    }

    #[test]
    fn alias_rewrites_upstream_model_but_logs_client_model() {
        let upstream = test_upstream_json_success("alias", true);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key = create_test_station_key(&database, "alias", &upstream.base_url);
        database
            .upsert_model_alias(UpsertModelAliasInput {
                id: None,
                client_model: "gpt-4o-mini".to_string(),
                upstream_model: "openai/gpt-4o-mini".to_string(),
                enabled: true,
                note: None,
            })
            .expect("alias");

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-4o-mini", false));

        assert_eq!(response.station_key_id.as_deref(), Some(key.id.as_str()));
        assert_eq!(response.status_code, 200);
        assert_eq!(response.model.as_deref(), Some("gpt-4o-mini"));
        upstream.join();
    }

    #[test]
    fn request_cost_uses_pricing_rule_for_requested_model() {
        let upstream = test_upstream_json_success_with_usage(
            "priced-model",
            false,
            Some(serde_json::json!({
                "prompt_tokens": 12,
                "completion_tokens": 20,
                "total_tokens": 32
            })),
        );
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key = create_test_station_key(&database, "priced-model", &upstream.base_url);

        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: key.station_id.clone(),
                station_key_id: Some(key.id.clone()),
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "default-group".to_string(),
                input_price: None,
                output_price: None,
                fixed_price: None,
                rate_multiplier: Some(1.0),
                currency: "CNY".to_string(),
                unit: "multiplier".to_string(),
                price_type: "multiplier".to_string(),
                base_price_source: None,
                normalization_status: Some("group_rate_only".to_string()),
                source: "collector".to_string(),
                confidence: 0.8,
                enabled: true,
                note: None,
                collected_at: Some("1000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("group rate rule");
        database
            .upsert_pricing_rule(UpsertPricingRuleInput {
                id: None,
                station_id: key.station_id.clone(),
                station_key_id: Some(key.id.clone()),
                group_binding_id: None,
                group_name: Some("default".to_string()),
                tier_label: None,
                model: "gpt-5.4".to_string(),
                input_price: Some(2.0),
                output_price: Some(10.0),
                fixed_price: None,
                rate_multiplier: None,
                currency: "CNY".to_string(),
                unit: "per_1m_tokens".to_string(),
                price_type: "token".to_string(),
                base_price_source: Some("manual".to_string()),
                normalization_status: Some("complete".to_string()),
                source: "manual".to_string(),
                confidence: 1.0,
                enabled: true,
                note: None,
                collected_at: Some("2000".to_string()),
                valid_from: None,
                valid_until: None,
            })
            .expect("model price rule");

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));

        assert_eq!(response.status_code, 200);
        let response_body: Value =
            serde_json::from_slice(response.body_bytes().expect("body")).expect("response json");
        assert_eq!(response_body["usage"]["total_tokens"], 32);
        assert_eq!(response.request_cost.total_tokens, Some(32));
        assert_eq!(response.request_cost.estimated_input_cost, Some(0.000024));
        assert_eq!(response.request_cost.estimated_output_cost, Some(0.0002));
        assert_f64_close(response.request_cost.estimated_total_cost, 0.000224);
        assert_eq!(response.request_cost.cost_status, "priced");

        let streamed_cost = request_cost_for_observed_usage(
            &context,
            Some(&key.id),
            Some(&key.station_id),
            Some("gpt-5.4"),
            &ObservedUsage {
                input_tokens: Some(12),
                output_tokens: Some(20),
                total_tokens: Some(32),
                cache_creation_tokens: Some(2),
                cache_read_tokens: Some(8),
            },
        );
        assert_eq!(streamed_cost.cache_read_tokens, Some(8));
        assert_eq!(streamed_cost.billing_mode.as_deref(), Some("token"));
        assert_f64_close(streamed_cost.estimated_total_cost, 0.000224);
        upstream.join();
    }

    #[test]
    fn successful_proxy_request_updates_key_health() {
        let upstream = test_upstream_json_success("health-success", false);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key = create_test_station_key(&database, "health-success", &upstream.base_url);

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));
        commit_pending_successes(&context, &response.pending_successes, None, None);
        let health = context
            .database
            .get_station_key_health(key.id.clone())
            .expect("health");

        assert_eq!(response.status_code, 200);
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.consecutive_failures, 0);
        assert!(health.last_success_at.is_some());
        let remembered_key = context.route_affinity.lock().expect("affinity").lookup(
            RouteEndpointKind::ChatCompletions,
            Some("gpt-5.4"),
            now_millis_for_services() as i64,
        );
        assert_eq!(remembered_key.as_deref(), Some(key.id.as_str()));
        upstream.join();
    }

    #[test]
    fn runtime_skips_key_in_cooldown_and_uses_next_candidate() {
        let skipped = test_upstream_json_success("cooldown", false);
        let ready = test_upstream_json_success("ready", false);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key_a = create_test_station_key(&database, "cooldown", &skipped.base_url);
        let key_b = create_test_station_key(&database, "ready", &ready.base_url);
        database
            .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
            .expect("reorder");
        let base_now = now_millis_for_services() as i64;
        database
            .record_station_key_failure(&key_a.id, "timeout", &base_now.to_string())
            .expect("failure 1");
        database
            .record_station_key_failure(&key_a.id, "timeout", &(base_now + 1).to_string())
            .expect("failure 2");
        database
            .record_station_key_failure(&key_a.id, "timeout", &(base_now + 2).to_string())
            .expect("failure 3");

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));

        assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
        assert_eq!(response.status_code, 200);
        assert!(
            !skipped.was_called(),
            "cooldown key should be skipped before network"
        );
        ready.join();
        skipped.join();
    }

    #[test]
    fn runtime_uses_retry_after_header_for_rate_limit_cooldown() {
        let upstream = test_upstream_status(429, "Too Many Requests", &[("retry-after", "90")]);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key = create_test_station_key(&database, "rate-limited", &upstream.base_url);
        let before = now_millis_for_services() as i64;

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));
        let health = context
            .database
            .get_station_key_health(key.id)
            .expect("health");
        let cooldown_until = health
            .cooldown_until
            .as_deref()
            .and_then(|value| value.parse::<i64>().ok())
            .expect("cooldown");

        assert_eq!(response.status_code, 502);
        assert_eq!(health.failure_count, 1);
        assert!(
            cooldown_until >= before + 90_000 && cooldown_until < before + 120_000,
            "cooldown should honor retry-after, got {cooldown_until}, before {before}"
        );
        upstream.join();
    }

    #[test]
    fn runtime_falls_back_after_key_scoped_hard_failure() {
        let rejected = test_upstream_status(401, "Unauthorized", &[]);
        let accepted = test_upstream_json_success_times("after-auth-failure", false, None, 2);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key_a = create_test_station_key(&database, "auth-failed", &rejected.base_url);
        let key_b = create_test_station_key(&database, "auth-fallback", &accepted.base_url);
        database
            .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
            .expect("reorder");
        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));
        let failed_health = context
            .database
            .get_station_key_health(key_a.id.clone())
            .expect("failed key health");

        assert_eq!(response.status_code, 200);
        assert_eq!(response.station_key_id.as_deref(), Some(key_b.id.as_str()));
        assert_eq!(response.fallback_count, 1);
        assert!(rejected.was_called());
        assert!(accepted.was_called());
        assert_eq!(accepted.call_count(), 2);
        assert!(failed_health
            .last_error_summary
            .as_deref()
            .is_some_and(|summary| summary.contains("auth_error")));

        rejected.join();
        accepted.join();
    }

    #[test]
    fn automatic_runtime_rechecks_multiplier_before_retrying_next_key() {
        let accepted =
            test_upstream_json_success_times("automatic-expensive-after-failure", false, None, 1);
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        enable_automatic_routing(&database, 1.0);
        let key_b =
            create_test_station_key(&database, "automatic-now-too-expensive", &accepted.base_url);
        set_manual_multiplier_with_priority(&database, &key_b, 2.0, 100);
        let key_a =
            create_test_station_key(&database, "automatic-first-fails", "http://127.0.0.1:1");
        set_manual_multiplier_with_priority(&database, &key_a, 0.5, -100);
        database
            .reorder_key_pool(vec![key_a.id.clone(), key_b.id.clone()])
            .expect("reorder");

        let context = proxy_context(database);
        let response = forward_chat_request(&context, &chat_request("gpt-5.4", false));

        assert_eq!(
            response.status_code,
            503,
            "expected automatic retry to reject over-ceiling fallback; accepted_called={} selected={:?} route_policy={:?} route_reason={:?} rejected={:?}",
            accepted.was_called(),
            response.station_key_id,
            response.route_policy,
            response.route_reason,
            response.rejected_candidates_json
        );
        let error_body: Value =
            serde_json::from_slice(response.body_bytes().expect("error body")).expect("error json");
        assert_eq!(
            error_body["error"]["code"].as_str(),
            Some("routing_no_candidate_within_multiplier_limit")
        );
        assert_eq!(response.station_key_id, None);
        assert!(
            !accepted.was_called(),
            "candidate that rises above the immutable multiplier ceiling must be rejected before probe or forward"
        );
        accepted.join();
    }

    #[test]
    fn automatic_runtime_revalidates_multiplier_immediately_before_forward() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        enable_automatic_routing(&database, 1.0);
        let key =
            create_test_station_key(&database, "automatic-forward-recheck", "http://127.0.0.1:1");
        set_manual_multiplier_with_priority(&database, &key, 0.5, 0);
        let request = chat_request("gpt-5.4", false);
        let body: Value = serde_json::from_slice(&request.body).expect("request body");
        let route_request = route_request_for_chat(
            &request,
            Some("gpt-5.4".to_string()),
            false,
            &body,
            RoutingPolicy::AutomaticBalanced,
            Some(1.0),
            RoutingGroupFilter::AllGroups,
            false,
            None,
        );
        set_manual_multiplier_with_priority(&database, &key, 2.0, 0);
        let context = proxy_context(database);

        let response =
            revalidate_automatic_candidate_before_forward(&context, &route_request, &key.id)
                .expect_err(
                    "candidate above immutable request ceiling should reject before forwarding",
                );
        let error_body: Value =
            serde_json::from_slice(response.body_bytes().expect("error body")).expect("error json");

        assert_eq!(response.status_code, 503);
        assert_eq!(
            error_body["error"]["code"].as_str(),
            Some("routing_no_candidate_within_multiplier_limit")
        );
    }

    #[test]
    fn stable_session_hash_uses_explicit_deterministic_hash() {
        assert_eq!(stable_session_hash("relay-session"), "508467564de2b2cf");
        assert_eq!(stable_session_hash(" relay-session "), "ca80f260a6db4d47");
    }

    #[test]
    fn forward_models_request_aggregates_and_deduplicates_enabled_keys_by_priority() {
        let first_upstream = TcpListener::bind(("127.0.0.1", 0)).expect("bind first upstream");
        let first_port = first_upstream.local_addr().expect("first addr").port();
        let second_upstream = TcpListener::bind(("127.0.0.1", 0)).expect("bind second upstream");
        let second_port = second_upstream.local_addr().expect("second addr").port();

        let first_handle = thread::spawn(move || {
            respond_once_with_models(first_upstream, &["gpt-5.4", "shared-model"]);
        });
        let second_handle = thread::spawn(move || {
            respond_once_with_models(second_upstream, &["shared-model", "o3"]);
        });

        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        database
            .create_station(CreateStationInput {
                name: "First models station".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: format!("http://127.0.0.1:{first_port}"),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-first-models".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("first station");
        thread::sleep(Duration::from_millis(2));
        database
            .create_station(CreateStationInput {
                name: "Second models station".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: format!("http://127.0.0.1:{second_port}"),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-second-models".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("second station");
        let context = ProxyServerContext {
            database,
            data_key: crate::services::secrets::crypto::generate_data_key(),
            active_requests: Arc::new(AtomicU32::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            route_affinity: Arc::new(Mutex::new(RouteAffinityStore::default())),
            probe_cache: Arc::new(Mutex::new(ProbeConfirmationCache::default())),
        };
        let request = ParsedRequest {
            method: "GET".to_string(),
            path: "/v1/models".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        };

        let response = forward_models_request(&context, &request);
        let body = response.body_bytes().expect("buffered body");
        let value: Value = serde_json::from_slice(body).expect("json body");
        let ids: Vec<_> = value["data"]
            .as_array()
            .expect("data array")
            .iter()
            .filter_map(|item| item["id"].as_str())
            .collect();

        assert_eq!(response.status_code, 200);
        assert_eq!(ids, vec!["gpt-5.4", "shared-model", "o3"]);

        first_handle.join().expect("first join");
        second_handle.join().expect("second join");
    }

    fn respond_once_with_models(listener: TcpListener, ids: &[&str]) {
        let (mut server_stream, _) = listener.accept().expect("accept upstream");
        let mut buf = [0_u8; 2048];
        let _ = server_stream.read(&mut buf);
        let data: Vec<_> = ids
            .iter()
            .map(|id| serde_json::json!({ "id": id, "object": "model" }))
            .collect();
        let body = serde_json::to_vec(&serde_json::json!({
            "object": "list",
            "data": data
        }))
        .expect("models body");
        let header = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        server_stream.write_all(header.as_bytes()).expect("header");
        server_stream.write_all(&body).expect("body");
    }

    struct TestUpstream {
        base_url: String,
        called: Arc<AtomicBool>,
        call_count: Arc<AtomicU32>,
        handle: JoinHandle<()>,
    }

    impl TestUpstream {
        fn was_called(&self) -> bool {
            self.called.load(Ordering::Relaxed)
        }

        fn call_count(&self) -> u32 {
            self.call_count.load(Ordering::Relaxed)
        }

        fn join(self) {
            self.handle.join().expect("join upstream");
        }
    }

    fn test_upstream_json_success(name: &str, expect_alias_model: bool) -> TestUpstream {
        test_upstream_json_success_with_usage(name, expect_alias_model, None)
    }

    fn test_upstream_json_success_with_usage(
        name: &str,
        expect_alias_model: bool,
        usage: Option<Value>,
    ) -> TestUpstream {
        test_upstream_json_success_times(name, expect_alias_model, usage, 1)
    }

    fn test_upstream_json_success_times(
        name: &str,
        expect_alias_model: bool,
        usage: Option<Value>,
        expected_requests: usize,
    ) -> TestUpstream {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        listener
            .set_nonblocking(true)
            .expect("nonblocking upstream");
        let port = listener.local_addr().expect("upstream addr").port();
        let called = Arc::new(AtomicBool::new(false));
        let call_count = Arc::new(AtomicU32::new(0));
        let called_for_thread = Arc::clone(&called);
        let call_count_for_thread = Arc::clone(&call_count);
        let name = name.to_string();
        let handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_millis(3000);
            let mut handled = 0_usize;
            loop {
                match listener.accept() {
                    Ok((mut server_stream, _)) => {
                        called_for_thread.store(true, Ordering::Relaxed);
                        call_count_for_thread.fetch_add(1, Ordering::Relaxed);
                        let _ = server_stream.set_read_timeout(Some(Duration::from_millis(200)));
                        let mut buf = [0_u8; 4096];
                        let mut read = 0;
                        let deadline = std::time::Instant::now() + Duration::from_millis(200);
                        loop {
                            match server_stream.read(&mut buf[read..]) {
                                Ok(0) => break,
                                Ok(count) => {
                                    read += count;
                                    if read >= buf.len() {
                                        break;
                                    }
                                }
                                Err(error)
                                    if error.kind() == std::io::ErrorKind::WouldBlock
                                        || error.kind() == std::io::ErrorKind::TimedOut =>
                                {
                                    if read > 0 || std::time::Instant::now() >= deadline {
                                        break;
                                    }
                                    thread::sleep(Duration::from_millis(5));
                                }
                                Err(error) => panic!("read upstream request: {error}"),
                            }
                        }
                        let request_text = String::from_utf8_lossy(&buf[..read]);
                        if expect_alias_model {
                            assert!(
                                request_text.contains(r#""model":"openai/gpt-4o-mini""#),
                                "upstream should receive mapped model, got {request_text}"
                            );
                        }
                        let mut body_value = serde_json::json!({
                            "id": format!("chatcmpl-{name}"),
                            "object": "chat.completion",
                            "choices": [{"message": {"role": "assistant", "content": "pong"}}]
                        });
                        if let Some(usage) = usage.clone() {
                            body_value["usage"] = usage;
                        }
                        let body = serde_json::to_vec(&body_value).expect("body");
                        let header = format!(
                            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        server_stream.write_all(header.as_bytes()).expect("header");
                        server_stream.write_all(&body).expect("body");
                        handled += 1;
                        if handled >= expected_requests {
                            return;
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept upstream: {error}"),
                }
            }
        });

        TestUpstream {
            base_url: format!("http://127.0.0.1:{port}"),
            called,
            call_count,
            handle,
        }
    }

    fn test_upstream_status(status: u16, reason: &str, headers: &[(&str, &str)]) -> TestUpstream {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        listener
            .set_nonblocking(true)
            .expect("nonblocking upstream");
        let port = listener.local_addr().expect("upstream addr").port();
        let called = Arc::new(AtomicBool::new(false));
        let call_count = Arc::new(AtomicU32::new(0));
        let called_for_thread = Arc::clone(&called);
        let call_count_for_thread = Arc::clone(&call_count);
        let reason = reason.to_string();
        let headers = headers
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect::<Vec<_>>();
        let handle = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_millis(3000);
            loop {
                match listener.accept() {
                    Ok((mut server_stream, _)) => {
                        called_for_thread.store(true, Ordering::Relaxed);
                        call_count_for_thread.fetch_add(1, Ordering::Relaxed);
                        let _ = server_stream.set_read_timeout(Some(Duration::from_millis(200)));
                        let mut buf = [0_u8; 4096];
                        let _ = server_stream.read(&mut buf);
                        let body = br#"{"error":{"message":"rate limited"}}"#;
                        let extra_headers = headers
                            .iter()
                            .map(|(key, value)| format!("{key}: {value}\r\n"))
                            .collect::<String>();
                        let header = format!(
                            "HTTP/1.1 {status} {reason}\r\ncontent-type: application/json\r\n{extra_headers}content-length: {}\r\nconnection: close\r\n\r\n",
                            body.len()
                        );
                        server_stream.write_all(header.as_bytes()).expect("header");
                        server_stream.write_all(body).expect("body");
                        return;
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        if std::time::Instant::now() >= deadline {
                            return;
                        }
                        thread::sleep(Duration::from_millis(5));
                    }
                    Err(error) => panic!("accept upstream: {error}"),
                }
            }
        });

        TestUpstream {
            base_url: format!("http://127.0.0.1:{port}"),
            called,
            call_count,
            handle,
        }
    }

    fn enable_automatic_routing(database: &AppDatabase, max_rate_multiplier: f64) {
        let settings = database.get_settings().expect("settings");
        database
            .update_settings(UpdateSettingsInput {
                local_proxy_port: settings.local_proxy_port,
                default_routing_strategy: "automatic_balanced".to_string(),
                collector_proxy_mode: settings.collector_proxy_mode,
                collector_proxy_url: settings.collector_proxy_url,
                max_rate_multiplier: Some(Some(max_rate_multiplier)),
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
                tray_behavior: settings.tray_behavior,
                developer_mode_enabled: settings.developer_mode_enabled,
            })
            .expect("enable automatic routing");
    }

    fn set_manual_multiplier_with_priority(
        database: &AppDatabase,
        key: &StationKey,
        multiplier: f64,
        priority: i64,
    ) {
        database
            .update_station_key(UpdateStationKeyInput {
                id: key.id.clone(),
                station_id: key.station_id.clone(),
                name: key.name.clone(),
                api_key: None,
                enabled: key.enabled,
                priority,
                max_concurrency: key.max_concurrency,
                load_factor: key.load_factor,
                schedulable: key.schedulable,
                group_name: key.group_name.clone(),
                tier_label: key.tier_label.clone(),
                group_binding_id: key.group_binding_id.clone(),
                group_id_hash: key.group_id_hash.clone(),
                rate_multiplier: key.rate_multiplier,
                manual_rate_multiplier: Some(Some(multiplier)),
                rate_source: key.rate_source.clone(),
                balance_scope: key.balance_scope.clone(),
                status: key.status.clone(),
                note: key.note.clone(),
            })
            .expect("set manual multiplier");
    }

    fn create_test_station_key(database: &AppDatabase, name: &str, base_url: &str) -> StationKey {
        thread::sleep(Duration::from_millis(2));
        let station = database
            .create_station(CreateStationInput {
                name: name.to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: base_url.to_string(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: format!("sk-{name}"),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .list_station_keys(station.id)
            .expect("keys")
            .remove(0)
    }

    fn default_capabilities_input(station_key_id: String) -> UpdateStationKeyCapabilitiesInput {
        UpdateStationKeyCapabilitiesInput {
            station_key_id,
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: false,
            supports_stream: true,
            supports_tools: false,
            supports_vision: false,
            supports_reasoning: false,
            model_allowlist: Vec::new(),
            model_blocklist: Vec::new(),
            preferred_models: Vec::new(),
            only_use_as_backup: false,
            routing_tags: Vec::new(),
        }
    }

    fn proxy_context(database: AppDatabase) -> ProxyServerContext {
        ProxyServerContext {
            database,
            data_key: crate::services::secrets::crypto::generate_data_key(),
            active_requests: Arc::new(AtomicU32::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            route_affinity: Arc::new(Mutex::new(RouteAffinityStore::default())),
            probe_cache: Arc::new(Mutex::new(ProbeConfirmationCache::default())),
        }
    }

    fn assert_f64_close(actual: Option<f64>, expected: f64) {
        let actual = actual.expect("cost should be estimated");
        assert!(
            (actual - expected).abs() < f64::EPSILON,
            "expected {expected}, got {actual}"
        );
    }

    fn chat_request(model: &str, stream: bool) -> ParsedRequest {
        ParsedRequest {
            method: "POST".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: serde_json::to_vec(&serde_json::json!({
                "model": model,
                "messages": [{"role": "user", "content": "ping"}],
                "stream": stream
            }))
            .expect("body"),
        }
    }

    fn responses_request(model: &str, stream: bool) -> ParsedRequest {
        ParsedRequest {
            method: "POST".to_string(),
            path: "/v1/responses".to_string(),
            headers: HashMap::from([("content-type".to_string(), "application/json".to_string())]),
            body: serde_json::to_vec(&serde_json::json!({
                "model": model,
                "input": "ping",
                "stream": stream
            }))
            .expect("body"),
        }
    }

    #[test]
    fn local_usage_endpoint_returns_latest_station_balance_summary() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let key_a = create_test_station_key(&database, "usage-alpha", "https://alpha.example");
        let key_b = create_test_station_key(&database, "usage-beta", "https://beta.example");
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some("usage-alpha-old".to_string()),
                station_id: key_a.station_id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(10.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                low_balance_threshold: None,
                status: "normal".to_string(),
                source: "test".to_string(),
                confidence: 0.9,
                collected_at: Some("1000".to_string()),
            })
            .expect("old alpha balance");
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some("usage-alpha-new".to_string()),
                station_id: key_a.station_id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(12.5),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                low_balance_threshold: None,
                status: "normal".to_string(),
                source: "test".to_string(),
                confidence: 0.9,
                collected_at: Some("2000".to_string()),
            })
            .expect("new alpha balance");
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some("usage-beta".to_string()),
                station_id: key_b.station_id.clone(),
                station_key_id: None,
                scope: "station".to_string(),
                value: Some(7.5),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                low_balance_threshold: None,
                status: "low".to_string(),
                source: "test".to_string(),
                confidence: 0.9,
                collected_at: Some("1500".to_string()),
            })
            .expect("beta balance");
        database
            .upsert_balance_snapshot(UpsertBalanceSnapshotInput {
                id: Some("usage-key-scope-ignored".to_string()),
                station_id: key_b.station_id.clone(),
                station_key_id: Some(key_b.id.clone()),
                scope: "station_key".to_string(),
                value: Some(99.0),
                currency: "CNY".to_string(),
                credit_unit: None,
                used_value: None,
                total_value: None,
                low_balance_threshold: None,
                status: "normal".to_string(),
                source: "test".to_string(),
                confidence: 0.9,
                collected_at: Some("2500".to_string()),
            })
            .expect("key balance");
        let context = proxy_context(database);
        let request = ParsedRequest {
            method: "GET".to_string(),
            path: "/v1/usage".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        };

        let response = handle_proxy_request(&context, &request);

        assert_eq!(response.status_code, 200);
        assert_eq!(response.status_label, "success");
        let value: Value =
            serde_json::from_slice(response.body_bytes().expect("body")).expect("usage json");
        assert_eq!(value["is_active"], true);
        assert_eq!(value["remaining"], 20.0);
        assert_eq!(value["balance"], 20.0);
        assert_eq!(value["unit"], "CNY");
        assert_eq!(value["stations"], 2);
        assert_eq!(value["low_balance_stations"], 1);
    }

    #[test]
    fn handle_proxy_request_returns_cors_preflight_response() {
        let context = ProxyServerContext {
            database: AppDatabase::new_in_memory_for_tests().expect("database"),
            data_key: crate::services::secrets::crypto::generate_data_key(),
            active_requests: Arc::new(AtomicU32::new(0)),
            request_count: Arc::new(AtomicU64::new(0)),
            route_affinity: Arc::new(Mutex::new(RouteAffinityStore::default())),
            probe_cache: Arc::new(Mutex::new(ProbeConfirmationCache::default())),
        };
        let request = ParsedRequest {
            method: "OPTIONS".to_string(),
            path: "/v1/chat/completions".to_string(),
            headers: HashMap::new(),
            body: Vec::new(),
        };

        let response = handle_proxy_request(&context, &request);

        assert_eq!(response.status_code, 204);
        assert_eq!(response.status_label, "success");
        assert_eq!(response.body_bytes(), Some(&[][..]));
    }

    #[test]
    fn write_http_response_includes_cors_compatibility_headers() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test listener");
        let port = listener.local_addr().expect("local addr").port();

        let handle = thread::spawn(move || {
            let (mut server_stream, _) = listener.accept().expect("accept");
            let response = ProxyResponse {
                status_code: 204,
                content_type: "text/plain".to_string(),
                body: ProxyResponseBody::Buffered(Vec::new()),
                model: None,
                stream: false,
                station_key_id: None,
                station_id: None,
                upstream_base_url: None,
                fallback_count: 0,
                retry_after_ms: None,
                status_label: "success".to_string(),
                error_message: None,
                route_policy: None,
                route_reason: None,
                rejected_candidates_json: None,
                group_binding_id: None,
                normalization_status: None,
                balance_scope: None,
                economic_context_json: None,
                request_cost: crate::services::pricing::request_cost_unknown(),
                pending_successes: Vec::new(),
            };
            let outcome = write_http_response(&mut server_stream, response, Instant::now());
            assert!(outcome.error_message.is_none());
        });

        let mut client = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("timeout");
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).expect("read all");
        handle.join().expect("join");

        let text = String::from_utf8(buf).expect("utf8");
        assert!(text.contains("access-control-allow-origin: *"));
        assert!(text.contains("access-control-allow-methods: GET, POST, OPTIONS"));
        assert!(text.contains("access-control-allow-headers: authorization, content-type, accept"));
        assert!(!text.to_lowercase().contains("x-tauri"));
    }
}
