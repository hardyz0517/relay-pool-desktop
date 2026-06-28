use std::{
    collections::VecDeque,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};
use ureq::Agent;

use crate::{
    models::collector::{CollectorEvent, CollectorRunResult},
    services::database::AppDatabase,
};

const PROBE_PATHS: [&str; 5] = [
    "/",
    "/api/pricing",
    "/api/ratio_config",
    "/api/models",
    "/v1/models",
];

const LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];

const AUTH_PROBE_PATHS: [&str; 6] = [
    "/api/v1/auth/me",
    "/api/v1/user/profile",
    "/api/v1/groups/rates",
    "/api/v1/keys",
    "/api/v1/channels/available",
    "/api/v1/usage/dashboard/stats",
];

const FIELD_HINTS: [&str; 18] = [
    "balance",
    "quota",
    "credit",
    "amount",
    "group",
    "group_id",
    "group_name",
    "rate_multiplier",
    "ratio",
    "multiplier",
    "api_key",
    "key",
    "token",
    "usage",
    "used",
    "remain",
    "remaining",
    "models",
];

const SECRET_HINTS: [&str; 12] = [
    "api_key",
    "apikey",
    "key",
    "token",
    "access_token",
    "refresh_token",
    "authorization",
    "cookie",
    "password",
    "secret",
    "session",
    "credential",
];

pub fn detect_station(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    run_probe(database, station_id, ProbeMode::Detect)
}

pub fn collect_login_state(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let credentials = database.get_station_credentials(station_id.clone())?;
    let Some(username) = credentials.login_username.clone() else {
        return Ok(login_state_manual_required(
            station_id,
            station.name,
            "缺少登录账号，无法执行登录态采集。",
        ));
    };
    if !credentials.password_present {
        return Ok(login_state_manual_required(
            station_id,
            station.name,
            "缺少登录密码，无法执行登录态采集。",
        ));
    }

    let config = ProbeMode::Collect.config();
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(config.connect_timeout)
        .timeout_read(config.read_timeout)
        .timeout_write(config.connect_timeout)
        .build();

    let login_attempt = attempt_login(&agent, &station.base_url, &username)?;
    if let Some(result) = login_attempt.manual_required {
        return Ok(login_state_manual_required(
            station_id,
            station.name,
            &result,
        ));
    }

    let Some(token) = login_attempt.token else {
        return Ok(login_state_manual_required(
            station_id,
            station.name,
            "登录接口未返回可用 token，站点可能需要验证码、2FA 或魔改字段。",
        ));
    };

    let endpoint_results = run_authenticated_probes(&agent, &station.base_url, &token, config);
    build_login_state_snapshot(
        database,
        station_id,
        station.name,
        credentials.login_status,
        Some(username),
        endpoint_results,
        Some(token),
        login_attempt.login_message,
    )
}

fn run_probe(
    database: &AppDatabase,
    station_id: String,
    mode: ProbeMode,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let credentials = database.get_station_credentials(station_id.clone())?;
    if credentials.login_username.is_some() || credentials.password_present {
        database.update_station_login_status(
            &station_id,
            "manual_required",
            Some("P3 暂不自动登录；P4 将接入 WebView 登录和 XHR 捕获。".to_string()),
        )?;
    }

    let config = mode.config();
    let probe_results = run_endpoint_probes(&station.base_url, config);
    let mut events = Vec::with_capacity(probe_results.len());
    let mut responses = Vec::with_capacity(probe_results.len());
    let mut first_error: Option<String> = None;

    for result in probe_results {
        match result {
            EndpointProbe::Response(result) => {
                events.push(CollectorEvent {
                    event_type: "probe".to_string(),
                    message: format!("GET {} -> {}", result.path, result.status),
                    status: if result.ok { "matched" } else { "checked" }.to_string(),
                });
                responses.push(json!({
                    "path": result.path,
                    "url": result.url,
                    "status": result.status,
                    "result": endpoint_result_label(result.status),
                    "detail": endpoint_detail(result.status, &result.content_type, result.json.is_some()),
                    "contentType": result.content_type,
                    "json": result.json,
                    "textPreview": result.text_preview,
                }));
            }
            EndpointProbe::Error(error) => {
                first_error.get_or_insert_with(|| error.message.clone());
                events.push(CollectorEvent {
                    event_type: "probe".to_string(),
                    message: format!("GET {} -> {}", error.path, error.message),
                    status: "error".to_string(),
                });
                responses.push(json!({
                    "path": error.path,
                    "url": error.url,
                    "result": error.label,
                    "detail": error.message,
                    "error": error.message,
                }));
            }
        }
    }

    let raw = json!({
        "mode": mode.as_str(),
        "stationId": station_id,
        "stationName": station.name,
        "baseUrl": station.base_url,
        "login": {
            "usernamePresent": credentials.login_username.is_some(),
            "passwordPresent": credentials.password_present,
            "status": if credentials.login_username.is_some() || credentials.password_present {
                "manual_required"
            } else {
                credentials.login_status.as_str()
            },
        },
        "responses": responses,
    });
    let normalized = normalize_probe(&raw);
    let endpoint_results = summarize_endpoint_results(&raw);
    let recognized = recognized_summary(&normalized);
    let matched_count = recognized
        .get("matchedFieldCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let adapter = adapter_label(&station.station_type);
    let detected_type = detected_type_label(&station.station_type, &normalized, &endpoint_results);
    let needs_login = credentials.login_username.is_some() || credentials.password_present;
    let conclusion = if matched_count > 0 {
        if mode == ProbeMode::Detect {
            "可用"
        } else {
            "已采集"
        }
    } else if needs_login {
        "需要登录"
    } else if first_error.is_some() {
        "失败"
    } else {
        "未识别"
    };
    let message = conclusion_message(conclusion, matched_count, needs_login);
    let summary = json!({
        "mode": mode.as_str(),
        "stationName": station.name,
        "adapter": adapter,
        "detectedType": detected_type,
        "conclusion": conclusion,
        "message": message,
        "probed": endpoint_results.len(),
        "endpointResults": endpoint_results,
        "recognized": recognized,
        "matchedFields": normalized.get("matchedFields").cloned().unwrap_or_else(|| json!([])),
        "balance": normalized.get("balance").cloned().unwrap_or(Value::Null),
        "groups": normalized.get("groups").cloned().unwrap_or_else(|| json!([])),
        "rateMultipliers": normalized.get("rateMultipliers").cloned().unwrap_or_else(|| json!([])),
        "keys": normalized.get("keys").cloned().unwrap_or_else(|| json!([])),
        "webviewRequired": needs_login || matched_count == 0,
        "rawPreviewAvailable": true,
        "webviewNote": "WebView 登录捕获将在 P4 接入。",
    });

    let status = if matched_count > 0 {
        "success"
    } else if needs_login {
        "manual_required"
    } else if first_error.is_some() {
        "partial"
    } else {
        "checked"
    };
    let error_message = if status == "success" {
        None
    } else {
        Some(first_error.unwrap_or_else(|| "未识别到余额、分组或倍率字段。".to_string()))
    };

    let snapshot = database.insert_collector_snapshot(
        &station_id,
        if mode == ProbeMode::Detect {
            "station-info-detect"
        } else {
            "station-info-collect"
        },
        status,
        summary,
        normalized,
        Some(redact_value(&raw)),
        error_message,
    )?;

    Ok(CollectorRunResult { snapshot, events })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeMode {
    Detect,
    Collect,
}

impl ProbeMode {
    fn as_str(self) -> &'static str {
        match self {
            ProbeMode::Detect => "detect",
            ProbeMode::Collect => "collect",
        }
    }

    fn config(self) -> ProbeConfig {
        match self {
            ProbeMode::Detect => ProbeConfig {
                global_deadline: Duration::from_secs(5),
                connect_timeout: Duration::from_millis(1000),
                read_timeout: Duration::from_millis(1800),
                max_concurrency: 3,
            },
            ProbeMode::Collect => ProbeConfig {
                global_deadline: Duration::from_secs(10),
                connect_timeout: Duration::from_millis(1200),
                read_timeout: Duration::from_millis(2200),
                max_concurrency: 3,
            },
        }
    }
}

#[derive(Clone, Copy)]
struct ProbeConfig {
    global_deadline: Duration,
    connect_timeout: Duration,
    read_timeout: Duration,
    max_concurrency: usize,
}

struct ProbeResult {
    path: &'static str,
    url: String,
    status: u16,
    content_type: String,
    json: Option<Value>,
    text_preview: Option<String>,
    ok: bool,
}

struct ProbeError {
    path: &'static str,
    url: String,
    label: String,
    message: String,
}

enum EndpointProbe {
    Response(ProbeResult),
    Error(ProbeError),
}

fn run_endpoint_probes(base_url: &str, config: ProbeConfig) -> Vec<EndpointProbe> {
    let started_at = Instant::now();
    let worker_count = config.max_concurrency.min(PROBE_PATHS.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(PROBE_PATHS.to_vec())));
    let (sender, receiver) = mpsc::channel();

    for _ in 0..worker_count {
        let worker_queue = Arc::clone(&queue);
        let worker_sender = sender.clone();
        let worker_base_url = base_url.to_string();
        thread::spawn(move || {
            let agent = ureq::AgentBuilder::new()
                .timeout_connect(config.connect_timeout)
                .timeout_read(config.read_timeout)
                .timeout_write(config.connect_timeout)
                .build();

            loop {
                let Some(path) = worker_queue
                    .lock()
                    .ok()
                    .and_then(|mut paths| paths.pop_front())
                else {
                    break;
                };
                let url = join_url(&worker_base_url, path);
                let probe = match probe_endpoint(&agent, path, &url) {
                    Ok(result) => EndpointProbe::Response(result),
                    Err(message) => EndpointProbe::Error(ProbeError {
                        path,
                        url,
                        label: error_label(&message),
                        message: shorten_error(&message),
                    }),
                };
                if worker_sender.send(probe).is_err() {
                    break;
                }
            }
        });
    }
    drop(sender);

    let mut results = Vec::with_capacity(PROBE_PATHS.len());
    while results.len() < PROBE_PATHS.len() {
        let elapsed = started_at.elapsed();
        if elapsed >= config.global_deadline {
            break;
        }
        let remaining = config.global_deadline.saturating_sub(elapsed);
        match receiver.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(result) => results.push(result),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    results.sort_by_key(|result| match result {
        EndpointProbe::Response(result) => endpoint_order(result.path),
        EndpointProbe::Error(error) => endpoint_order(error.path),
    });
    results
}

fn run_authenticated_probes(
    agent: &Agent,
    base_url: &str,
    token: &str,
    config: ProbeConfig,
) -> Vec<EndpointProbe> {
    let started_at = Instant::now();
    let worker_count = config.max_concurrency.min(AUTH_PROBE_PATHS.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(AUTH_PROBE_PATHS.to_vec())));
    let (sender, receiver) = mpsc::channel();

    for _ in 0..worker_count {
        let worker_queue = Arc::clone(&queue);
        let worker_sender = sender.clone();
        let worker_base_url = base_url.to_string();
        let worker_token = token.to_string();
        let worker_agent = agent.clone();
        thread::spawn(move || loop {
            let Some(path) = worker_queue
                .lock()
                .ok()
                .and_then(|mut paths| paths.pop_front())
            else {
                break;
            };
            let url = join_url(&worker_base_url, path);
            let probe = match probe_authenticated_endpoint(&worker_agent, path, &url, &worker_token)
            {
                Ok(result) => EndpointProbe::Response(result),
                Err(message) => EndpointProbe::Error(ProbeError {
                    path,
                    url,
                    label: error_label(&message),
                    message: shorten_error(&message),
                }),
            };
            if worker_sender.send(probe).is_err() {
                break;
            }
        });
    }
    drop(sender);

    let mut results = Vec::with_capacity(AUTH_PROBE_PATHS.len());
    while results.len() < AUTH_PROBE_PATHS.len() {
        let elapsed = started_at.elapsed();
        if elapsed >= config.global_deadline {
            break;
        }
        let remaining = config.global_deadline.saturating_sub(elapsed);
        match receiver.recv_timeout(remaining.min(Duration::from_millis(250))) {
            Ok(result) => results.push(result),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }

    results.sort_by_key(|result| match result {
        EndpointProbe::Response(result) => endpoint_order(result.path),
        EndpointProbe::Error(error) => endpoint_order(error.path),
    });
    results
}

fn probe_authenticated_endpoint(
    agent: &Agent,
    path: &'static str,
    url: &str,
    token: &str,
) -> Result<ProbeResult, String> {
    let response = match agent
        .get(url)
        .set("Authorization", &format!("Bearer {token}"))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(error.to_string()),
    };

    let status = response.status();
    let content_type = response.header("content-type").unwrap_or("").to_string();
    let text = response
        .into_string()
        .map_err(|error| format!("读取响应失败: {error}"))?;
    let json = serde_json::from_str::<Value>(&text).ok();
    let text_preview = if json.is_some() {
        None
    } else {
        Some(text.chars().take(360).collect())
    };

    Ok(ProbeResult {
        path,
        url: url.to_string(),
        status,
        content_type,
        json,
        text_preview,
        ok: (200..400).contains(&status),
    })
}

struct LoginAttempt {
    token: Option<String>,
    login_message: Option<String>,
    manual_required: Option<String>,
}

fn attempt_login(agent: &Agent, base_url: &str, username: &str) -> Result<LoginAttempt, String> {
    let password_placeholder = "[REDACTED]";
    let username_variants = [
        ("email", username),
        ("username", username),
        ("user", username),
    ];

    for path in LOGIN_PATHS {
        for (field, value) in username_variants {
            let payload = json!({
                field: value,
                "password": password_placeholder,
            });
            let url = join_url(base_url, path);
            let response = match agent.post(&url).send_json(payload) {
                Ok(response) => response,
                Err(ureq::Error::Status(_, response)) => response,
                Err(error) => {
                    return Err(format!("登录请求失败: {error}"));
                }
            };
            let status = response.status();
            let body = response
                .into_string()
                .map_err(|error| format!("读取登录响应失败: {error}"))?;
            let parsed = serde_json::from_str::<Value>(&body).ok();
            if let Some(parsed) = parsed {
                if let Some(token) = extract_token(&parsed) {
                    return Ok(LoginAttempt {
                        token: Some(token),
                        login_message: Some(format!("已从 {path} 读取登录 token。")),
                        manual_required: None,
                    });
                }
                if needs_manual_login(&parsed, status) {
                    return Ok(LoginAttempt {
                        token: None,
                        login_message: Some(shorten_error(&parsed.to_string())),
                        manual_required: Some("接口需要验证码、2FA 或额外登录步骤。".to_string()),
                    });
                }
            }
            if (200..300).contains(&status) {
                return Ok(LoginAttempt {
                    token: None,
                    login_message: Some("登录接口返回成功，但未识别到 token 字段。".to_string()),
                    manual_required: Some("登录响应未返回可用 token。".to_string()),
                });
            }
        }
    }

    Ok(LoginAttempt {
        token: None,
        login_message: Some("未能从登录接口获取 token。".to_string()),
        manual_required: Some("登录失败，账号密码可能无效或站点字段已魔改。".to_string()),
    })
}

fn extract_token(value: &Value) -> Option<String> {
    if let Some(token) = value.get("access_token").and_then(Value::as_str) {
        return Some(token.to_string());
    }
    if let Some(token) = value.get("token").and_then(Value::as_str) {
        return Some(token.to_string());
    }
    if let Some(data) = value.get("data") {
        if let Some(token) = extract_token(data) {
            return Some(token);
        }
    }
    None
}

fn needs_manual_login(value: &Value, status: u16) -> bool {
    if matches!(status, 401 | 403) {
        return true;
    }
    value
        .get("requires_2fa")
        .and_then(Value::as_bool)
        .unwrap_or(false)
        || value
            .get("captcha_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        || value
            .get("manual_required")
            .and_then(Value::as_bool)
            .unwrap_or(false)
}

fn build_login_state_snapshot(
    database: &AppDatabase,
    station_id: String,
    station_name: String,
    login_status: String,
    login_username: Option<String>,
    endpoint_results: Vec<EndpointProbe>,
    token: Option<String>,
    login_message: Option<String>,
) -> Result<CollectorRunResult, String> {
    let mut responses = Vec::with_capacity(endpoint_results.len());
    let mut first_error: Option<String> = None;

    for result in endpoint_results {
        match result {
            EndpointProbe::Response(result) => {
                responses.push(json!({
                    "path": result.path,
                    "url": result.url,
                    "status": result.status,
                    "result": endpoint_result_label(result.status),
                    "detail": endpoint_detail(result.status, &result.content_type, result.json.is_some()),
                    "contentType": result.content_type,
                    "json": result.json,
                    "textPreview": result.text_preview,
                }));
            }
            EndpointProbe::Error(error) => {
                first_error.get_or_insert_with(|| error.message.clone());
                responses.push(json!({
                    "path": error.path,
                    "url": error.url,
                    "result": error.label,
                    "detail": error.message,
                    "error": error.message,
                }));
            }
        }
    }

    if let Some(message) = login_message.clone() {
        if !message.trim().is_empty() {
            responses.insert(
                0,
                json!({
                    "path": "/api/v1/auth/login",
                    "url": "login://attempt",
                    "status": 200,
                    "result": "已检查",
                    "detail": message,
                    "contentType": "application/json",
                    "json": {
                        "message": message,
                    },
                }),
            );
        }
    }

    let raw = json!({
        "mode": "login-state",
        "stationId": station_id,
        "stationName": station_name,
        "login": {
            "usernamePresent": login_username.as_ref().map(|value| !value.trim().is_empty()).unwrap_or(false),
            "passwordPresent": token.is_some(),
            "status": login_status,
        },
        "responses": responses,
    });
    let normalized = normalize_probe(&raw);
    let endpoint_results = summarize_endpoint_results(&raw);
    let recognized = recognized_summary(&normalized);
    let matched_count = recognized
        .get("matchedFieldCount")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let conclusion = if matched_count > 0 {
        "已采集"
    } else if first_error.is_some() || token.is_none() {
        "需要登录"
    } else {
        "未识别"
    };
    let message = if matched_count > 0 {
        format!("已识别到 {matched_count} 个候选字段。")
    } else if let Some(text) = login_message.clone() {
        text
    } else {
        "未识别到可展示的业务字段。".to_string()
    };
    let summary = json!({
        "mode": "login-state",
        "stationName": station_name,
        "adapter": "Login State Adapter",
        "detectedType": "Login State",
        "conclusion": conclusion,
        "message": message,
        "loginStatus": login_status,
        "loginRequired": token.is_none(),
        "nextStep": if token.is_none() {
            "请检查账号密码或改用高级网页登录捕获。"
        } else {
            "已完成登录态采集。"
        },
        "diagnosis": if token.is_none() {
            "登录接口未返回可用 token，站点可能需要验证码、2FA 或魔改字段。"
        } else if matched_count == 0 {
            "已完成登录，但当前接口未返回可识别的余额、分组、倍率或 key 字段。"
        } else {
            "已识别到登录态接口响应。"
        },
        "probed": endpoint_results.len(),
        "endpointResults": endpoint_results,
        "recognized": recognized,
        "matchedFields": normalized.get("matchedFields").cloned().unwrap_or_else(|| json!([])),
        "balance": normalized.get("balance").cloned().unwrap_or(Value::Null),
        "groups": normalized.get("groups").cloned().unwrap_or_else(|| json!([])),
        "rateMultipliers": normalized.get("rateMultipliers").cloned().unwrap_or_else(|| json!([])),
        "keys": normalized.get("keys").cloned().unwrap_or_else(|| json!([])),
        "models": normalized.get("models").cloned().unwrap_or_else(|| json!([])),
        "webviewRequired": token.is_none() || matched_count == 0,
        "rawPreviewAvailable": true,
        "webviewNote": "网页登录捕获仍保留为高级兜底功能。",
    });
    let status = if matched_count > 0 {
        "success"
    } else if token.is_none() {
        "manual_required"
    } else if first_error.is_some() {
        "partial"
    } else {
        "checked"
    };
    let error_message = if status == "success" {
        None
    } else {
        Some(
            login_message
                .clone()
                .or(first_error)
                .unwrap_or_else(|| "未识别到余额、分组或倍率字段。".to_string()),
        )
    };

    let snapshot = database.insert_collector_snapshot(
        &station_id,
        "login-state-collect",
        status,
        summary,
        normalized,
        Some(redact_value(&raw)),
        error_message,
    )?;

    Ok(CollectorRunResult {
        snapshot,
        events: Vec::new(),
    })
}

fn login_state_manual_required(
    station_id: String,
    station_name: String,
    message: &str,
) -> CollectorRunResult {
    let snapshot = crate::models::collector::CollectorSnapshot {
        id: format!(
            "snapshot-{}",
            crate::services::database::now_millis_for_services()
        ),
        station_id: station_id.clone(),
        source: "login-state-collect".to_string(),
        status: "manual_required".to_string(),
        fetched_at: crate::services::database::now_millis_for_services().to_string(),
        summary_json: json!({
            "mode": "login-state",
            "adapter": "Login State Adapter",
            "detectedType": "Login State",
            "conclusion": "需要登录",
            "message": message,
            "loginRequired": true,
            "nextStep": "请补齐账号密码后重试，或使用高级网页登录捕获。",
            "diagnosis": message,
            "endpointResults": [],
            "recognized": {
                "balanceLabel": "未识别",
                "groupCount": 0,
                "rateCount": 0,
                "keyCount": 0,
                "matchedFieldCount": 0,
            },
            "webviewRequired": true,
            "webviewNote": "网页登录捕获仍是高级兜底功能。",
            "stationName": station_name,
        }),
        normalized_json: json!({
            "stationId": station_id,
            "adapter": "login-state",
            "status": "manual_required",
            "balance": Value::Null,
            "groups": [],
            "rateMultipliers": [],
            "keys": [],
            "models": [],
            "matchedFields": [],
            "detectedEndpoints": [],
            "pendingConfirmations": [],
            "confidenceSummary": { "recognizedFieldCount": 0 },
        }),
        raw_json_redacted: Some(json!({
            "stationName": station_name,
            "message": message,
        })),
        error_message: Some(message.to_string()),
        created_at: crate::services::database::now_millis_for_services().to_string(),
    };

    CollectorRunResult {
        snapshot,
        events: vec![crate::models::collector::CollectorEvent {
            event_type: "login-state".to_string(),
            message: message.to_string(),
            status: "manual_required".to_string(),
        }],
    }
}

fn probe_endpoint(agent: &Agent, path: &'static str, url: &str) -> Result<ProbeResult, String> {
    let response = match agent.get(url).call() {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => response,
        Err(error) => return Err(error.to_string()),
    };

    let status = response.status();
    let content_type = response.header("content-type").unwrap_or("").to_string();
    let text = response
        .into_string()
        .map_err(|error| format!("读取响应失败: {error}"))?;
    let json = serde_json::from_str::<Value>(&text).ok();
    let text_preview = if json.is_some() {
        None
    } else {
        Some(text.chars().take(360).collect())
    };

    Ok(ProbeResult {
        path,
        url: url.to_string(),
        status,
        content_type,
        json,
        text_preview,
        ok: (200..400).contains(&status),
    })
}

fn endpoint_order(path: &str) -> usize {
    PROBE_PATHS
        .iter()
        .position(|item| *item == path)
        .unwrap_or(PROBE_PATHS.len())
}

fn endpoint_result_label(status: u16) -> &'static str {
    match status {
        200..=299 => "成功",
        300..=399 => "重定向",
        401 | 403 => "需要登录",
        404 => "404",
        408 => "超时",
        429 => "限流",
        500..=599 => "站点异常",
        _ => "已检查",
    }
}

fn endpoint_detail(status: u16, content_type: &str, has_json: bool) -> &'static str {
    match status {
        200..=299 if has_json => "识别到 JSON 响应",
        200..=299 if content_type.contains("html") => "识别到页面",
        200..=299 => "接口可访问",
        401 | 403 => "接口需要登录，等待 P4 WebView 捕获",
        404 => "该站点未开放此接口",
        429 => "接口限流，可稍后重试",
        500..=599 => "站点返回服务端异常",
        _ => "已记录接口响应",
    }
}

fn error_label(message: &str) -> String {
    let lower = message.to_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") || lower.contains("deadline") {
        "超时".to_string()
    } else {
        "请求失败".to_string()
    }
}

fn shorten_error(message: &str) -> String {
    let lower = message.to_lowercase();
    let readable = if lower.contains("timeout") || lower.contains("timed out") {
        "站点请求超时"
    } else if lower.contains("dns") || lower.contains("resolve") {
        "域名解析失败"
    } else if lower.contains("connection refused") {
        "站点拒绝连接"
    } else if lower.contains("certificate") || lower.contains("tls") {
        "TLS 证书或连接失败"
    } else {
        "站点请求失败"
    };
    readable.to_string()
}

fn summarize_endpoint_results(raw: &Value) -> Vec<Value> {
    raw.get("responses")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| {
                    json!({
                        "path": item.get("path").and_then(Value::as_str).unwrap_or("/"),
                        "result": item.get("result").and_then(Value::as_str).unwrap_or("已检查"),
                        "detail": item.get("detail").and_then(Value::as_str).unwrap_or("已记录接口响应"),
                        "statusCode": item.get("status").cloned().unwrap_or(Value::Null),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

fn recognized_summary(normalized: &Value) -> Value {
    let matched_field_count = normalized
        .get("matchedFields")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let group_count = normalized
        .get("groups")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let rate_count = normalized
        .get("rateMultipliers")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let key_count = normalized
        .get("keys")
        .and_then(Value::as_array)
        .map(|items| items.len())
        .unwrap_or(0);
    let balance = normalized.get("balance").cloned().unwrap_or(Value::Null);

    json!({
        "balanceLabel": if balance.is_null() { Value::String("未识别".to_string()) } else { balance },
        "groupCount": group_count,
        "rateCount": rate_count,
        "keyCount": key_count,
        "matchedFieldCount": matched_field_count,
    })
}

fn adapter_label(station_type: &str) -> &'static str {
    match station_type {
        "sub2api" => "Sub2API Adapter",
        "newapi" => "NewAPI Adapter（待接入）",
        "openai-compatible" => "OpenAI-compatible Adapter（基础探测）",
        _ => "Auto Detect",
    }
}

fn detected_type_label(
    station_type: &str,
    normalized: &Value,
    endpoints: &[Value],
) -> &'static str {
    if station_type == "newapi" {
        return "NewAPI";
    }
    if station_type == "openai-compatible"
        || endpoints.iter().any(|endpoint| {
            endpoint
                .get("path")
                .and_then(Value::as_str)
                .map(|path| path == "/v1/models")
                .unwrap_or(false)
                && endpoint
                    .get("result")
                    .and_then(Value::as_str)
                    .map(|result| result == "成功")
                    .unwrap_or(false)
        })
    {
        return "OpenAI-compatible";
    }
    if normalized
        .get("matchedFields")
        .and_then(Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
    {
        return "Sub2API";
    }
    "Unknown"
}

fn conclusion_message(conclusion: &str, matched_count: u64, needs_login: bool) -> String {
    match conclusion {
        "可用" | "已采集" => format!("识别到 {matched_count} 个候选字段。"),
        "需要登录" if needs_login => "接口需要登录，WebView 登录捕获将在 P4 接入。".to_string(),
        "失败" => "站点请求失败或超时。".to_string(),
        _ => "未识别到余额、分组或倍率字段。".to_string(),
    }
}

fn normalize_probe(raw: &Value) -> Value {
    let mut matches = Vec::new();
    collect_matches(raw, "$", &mut matches);

    let mut groups = Vec::new();
    let mut rate_multipliers = Vec::new();
    let mut keys = Vec::new();
    let mut balance = Value::Null;

    for item in &matches {
        let field = item
            .get("field")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let value = item.get("value").cloned().unwrap_or(Value::Null);

        if matches_any(
            &field,
            &[
                "balance",
                "quota",
                "credit",
                "amount",
                "remain",
                "remaining",
            ],
        ) && balance.is_null()
        {
            balance = value.clone();
        }
        if matches_any(&field, &["group", "group_id", "group_name"]) {
            groups.push(value.clone());
        }
        if matches_any(&field, &["rate_multiplier", "ratio", "multiplier"]) {
            rate_multipliers.push(value.clone());
        }
        if matches_any(&field, &["api_key", "key", "token"]) {
            keys.push(redact_value(&value));
        }
    }

    json!({
        "matchedFields": matches,
        "balance": balance,
        "groups": groups,
        "rateMultipliers": rate_multipliers,
        "keys": keys,
    })
}

fn collect_matches(value: &Value, path: &str, matches: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let next_path = format!("{path}.{key}");
                if FIELD_HINTS
                    .iter()
                    .any(|hint| key.to_lowercase().contains(hint))
                {
                    matches.push(json!({
                        "field": key,
                        "path": next_path,
                        "value": redact_value(child),
                    }));
                }
                collect_matches(child, &next_path, matches);
            }
        }
        Value::Array(items) => {
            for (index, child) in items.iter().enumerate() {
                collect_matches(child, &format!("{path}[{index}]"), matches);
            }
        }
        _ => {}
    }
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut next = Map::new();
            for (key, child) in map {
                if is_secret_key(key) {
                    next.insert(key.clone(), Value::String("[REDACTED]".to_string()));
                } else {
                    next.insert(key.clone(), redact_value(child));
                }
            }
            Value::Object(next)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        Value::String(text) if looks_like_secret(text) => Value::String("[REDACTED]".to_string()),
        _ => value.clone(),
    }
}

fn is_secret_key(key: &str) -> bool {
    let lower = key.to_lowercase();
    SECRET_HINTS.iter().any(|hint| lower.contains(hint))
}

fn looks_like_secret(value: &str) -> bool {
    let lower = value.to_lowercase();
    value.len() > 18
        && (lower.starts_with("sk-")
            || lower.starts_with("bearer ")
            || lower.contains("authorization")
            || lower.contains("token=")
            || lower.contains("session="))
}

fn matches_any(value: &str, hints: &[&str]) -> bool {
    hints.iter().any(|hint| value.contains(hint))
}

fn join_url(base_url: &str, path: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if path == "/" {
        trimmed.to_string()
    } else {
        format!("{trimmed}{path}")
    }
}
