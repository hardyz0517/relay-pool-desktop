use std::{
    collections::VecDeque,
    sync::{mpsc, Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};
use ureq::Agent;

use crate::{
    models::{
        collector::{CollectorEvent, CollectorRunResult},
        stations::Station,
    },
    services::{
        database::AppDatabase,
        outbound::{agent_builder_for_proxy, resolve_proxy_config, ProxyConfig},
    },
};

const PROBE_PATHS: [&str; 5] = [
    "/",
    "/api/pricing",
    "/api/ratio_config",
    "/api/models",
    "/v1/models",
];

const LOGIN_PATHS: [&str; 3] = ["/api/v1/auth/login", "/auth/login", "/api/login"];

const AUTH_PROBE_PATHS: [&str; 7] = [
    "/api/v1/auth/me",
    "/api/v1/user/profile",
    "/api/v1/groups/available",
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

fn effective_station_proxy(
    database: &AppDatabase,
    station: &Station,
) -> Result<ProxyConfig, String> {
    let settings = database.get_settings()?;
    Ok(resolve_proxy_config(
        &station.collector_proxy_mode,
        station.collector_proxy_url.clone(),
        &settings.collector_proxy_mode,
        settings.collector_proxy_url,
    ))
}

pub fn detect_station(
    database: &AppDatabase,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    run_probe(database, station_id, ProbeMode::Detect)
}

pub fn collect_login_state(
    database: &AppDatabase,
    data_key: &[u8; 32],
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
    let Some(login_password) =
        database.get_station_login_password_with_data_key(station_id.clone(), data_key)?
    else {
        return Ok(login_state_manual_required(
            station_id,
            station.name,
            "登录密码不可解密，无法执行登录态采集。",
        ));
    };

    let config = ProbeMode::Collect.config();
    let proxy = effective_station_proxy(database, &station)?;
    let agent = agent_builder_for_proxy(&proxy)?
        .timeout_connect(config.connect_timeout)
        .timeout_read(config.read_timeout)
        .timeout_write(config.connect_timeout)
        .build();

    let login_attempt = attempt_login(&agent, &station.base_url, &username, &login_password)?;
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
            Some(
                "已保存登录信息；采集信息会尝试登录态接口，验证码或 2FA 场景可使用网页登录捕获。"
                    .to_string(),
            ),
        )?;
    }

    let config = mode.config();
    let proxy = effective_station_proxy(database, &station)?;
    let probe_results = run_endpoint_probes(&station.base_url, config, &proxy);
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
        "webviewNote": "验证码、2FA 或魔改登录场景可使用网页登录捕获。",
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

fn run_endpoint_probes(
    base_url: &str,
    config: ProbeConfig,
    proxy: &ProxyConfig,
) -> Vec<EndpointProbe> {
    let started_at = Instant::now();
    let worker_count = config.max_concurrency.min(PROBE_PATHS.len());
    let queue = Arc::new(Mutex::new(VecDeque::from(PROBE_PATHS.to_vec())));
    let (sender, receiver) = mpsc::channel();

    for _ in 0..worker_count {
        let worker_queue = Arc::clone(&queue);
        let worker_sender = sender.clone();
        let worker_base_url = base_url.to_string();
        let worker_proxy = proxy.clone();
        thread::spawn(move || {
            let agent = match agent_builder_for_proxy(&worker_proxy) {
                Ok(builder) => builder
                    .timeout_connect(config.connect_timeout)
                    .timeout_read(config.read_timeout)
                    .timeout_write(config.connect_timeout)
                    .build(),
                Err(error) => {
                    loop {
                        let Some(path) = worker_queue
                            .lock()
                            .ok()
                            .and_then(|mut paths| paths.pop_front())
                        else {
                            break;
                        };
                        let url = join_url(&worker_base_url, path);
                        let probe = EndpointProbe::Error(ProbeError {
                            path,
                            url,
                            label: "network_error".to_string(),
                            message: error.clone(),
                        });
                        if worker_sender.send(probe).is_err() {
                            break;
                        }
                    }
                    return;
                }
            };

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

pub(crate) struct LoginProbeOutcome {
    pub token_present: bool,
    pub login_message: Option<String>,
    pub manual_required: Option<String>,
}

pub(crate) struct LoginTokenOutcome {
    pub access_token: Option<String>,
}

pub(crate) fn test_login_credentials(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginProbeOutcome, String> {
    let config = ProbeMode::Collect.config();
    let proxy = ProxyConfig::direct();
    let agent = agent_builder_for_proxy(&proxy)?
        .timeout_connect(config.connect_timeout)
        .timeout_read(config.read_timeout)
        .timeout_write(config.connect_timeout)
        .build();
    let attempt = attempt_login(&agent, base_url, username, password)?;
    Ok(LoginProbeOutcome {
        token_present: attempt.token.is_some(),
        login_message: attempt.login_message,
        manual_required: attempt.manual_required,
    })
}

pub(crate) fn login_access_token(
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginTokenOutcome, String> {
    login_access_token_with_proxy(base_url, username, password, &ProxyConfig::direct())
}

pub(crate) fn login_access_token_with_proxy(
    base_url: &str,
    username: &str,
    password: &str,
    proxy: &ProxyConfig,
) -> Result<LoginTokenOutcome, String> {
    let config = ProbeMode::Collect.config();
    let agent = agent_builder_for_proxy(proxy)?
        .timeout_connect(config.connect_timeout)
        .timeout_read(config.read_timeout)
        .timeout_write(config.connect_timeout)
        .build();
    let attempt = attempt_login(&agent, base_url, username, password)?;
    Ok(LoginTokenOutcome {
        access_token: attempt.token,
    })
}

pub(crate) fn login_access_token_with_budget(
    base_url: &str,
    username: &str,
    password: &str,
    budget: Duration,
) -> Result<LoginTokenOutcome, String> {
    login_access_token_with_budget_and_proxy(
        base_url,
        username,
        password,
        budget,
        &ProxyConfig::direct(),
    )
}

pub(crate) fn login_access_token_with_budget_and_proxy(
    base_url: &str,
    username: &str,
    password: &str,
    budget: Duration,
    proxy: &ProxyConfig,
) -> Result<LoginTokenOutcome, String> {
    let config = ProbeMode::Collect.config();
    let deadline = LoginAttemptDeadline::new(budget);
    let attempt =
        attempt_login_with_budget(base_url, username, password, config, &deadline, proxy)?;
    Ok(LoginTokenOutcome {
        access_token: attempt.token,
    })
}

fn attempt_login(
    agent: &Agent,
    base_url: &str,
    username: &str,
    password: &str,
) -> Result<LoginAttempt, String> {
    let username_variants = [
        ("email", username),
        ("username", username),
        ("user", username),
    ];

    for path in LOGIN_PATHS {
        for (field, value) in username_variants {
            let payload = json!({
                field: value,
                "password": password,
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
                let attempt = login_attempt_from_response(path, status, &parsed);
                if attempt.token.is_some() || attempt.manual_required.is_some() {
                    return Ok(attempt);
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

struct LoginAttemptDeadline {
    started_at: Instant,
    budget: Duration,
}

impl LoginAttemptDeadline {
    fn new(budget: Duration) -> Self {
        Self {
            started_at: Instant::now(),
            budget,
        }
    }

    fn remaining(&self) -> Result<Duration, String> {
        self.budget
            .checked_sub(self.started_at.elapsed())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| "task_budget_exhausted".to_string())
    }
}

fn attempt_login_with_budget(
    base_url: &str,
    username: &str,
    password: &str,
    config: ProbeConfig,
    deadline: &LoginAttemptDeadline,
    proxy: &ProxyConfig,
) -> Result<LoginAttempt, String> {
    let username_variants = [
        ("email", username),
        ("username", username),
        ("user", username),
    ];

    for path in LOGIN_PATHS {
        for (field, value) in username_variants {
            let payload = json!({
                field: value,
                "password": password,
            });
            let url = join_url(base_url, path);
            let mut transient_attempted = false;

            loop {
                let response =
                    login_candidate_request(&url, payload.clone(), config, deadline, proxy)?;
                match response {
                    LoginCandidateResponse::Response(response) => {
                        let status = response.status();
                        if status == 429 {
                            if transient_attempted {
                                return Ok(login_transient_failure("rate_limited"));
                            }
                            transient_attempted = true;
                            let delay = retry_after_duration(&response).unwrap_or_default();
                            let remaining = deadline.remaining()?;
                            if delay >= remaining {
                                return Err("task_budget_exhausted".to_string());
                            }
                            if !delay.is_zero() {
                                thread::sleep(delay);
                            }
                            continue;
                        }

                        if (500..=599).contains(&status) {
                            if transient_attempted {
                                return Ok(login_transient_failure("upstream_5xx"));
                            }
                            transient_attempted = true;
                            continue;
                        }

                        let body = response
                            .into_string()
                            .map_err(|error| format!("读取登录响应失败: {error}"))?;
                        let parsed = serde_json::from_str::<Value>(&body).ok();
                        if let Some(parsed) = parsed {
                            let attempt = login_attempt_from_response(path, status, &parsed);
                            if attempt.token.is_some() || attempt.manual_required.is_some() {
                                return Ok(attempt);
                            }
                        }
                        if (200..300).contains(&status) {
                            return Ok(LoginAttempt {
                                token: None,
                                login_message: Some(
                                    "登录接口返回成功，但未识别到 token 字段。".to_string(),
                                ),
                                manual_required: Some("登录响应未返回可用 token。".to_string()),
                            });
                        }
                        break;
                    }
                    LoginCandidateResponse::Transient(error) => {
                        if transient_attempted {
                            return Ok(login_transient_failure(&error));
                        }
                        transient_attempted = true;
                    }
                }
            }
        }
    }

    Ok(LoginAttempt {
        token: None,
        login_message: Some("未能从登录接口获取 token。".to_string()),
        manual_required: Some("登录失败，账号密码可能无效或站点字段已魔改。".to_string()),
    })
}

enum LoginCandidateResponse {
    Response(ureq::Response),
    Transient(String),
}

fn login_candidate_request(
    url: &str,
    payload: Value,
    config: ProbeConfig,
    deadline: &LoginAttemptDeadline,
    proxy: &ProxyConfig,
) -> Result<LoginCandidateResponse, String> {
    let remaining = deadline.remaining()?;
    let timeout = remaining
        .min(config.read_timeout)
        .max(Duration::from_millis(1));
    let agent = agent_builder_for_proxy(proxy)?
        .timeout_connect(
            timeout
                .min(config.connect_timeout)
                .max(Duration::from_millis(1)),
        )
        .timeout_read(timeout)
        .timeout_write(
            timeout
                .min(config.connect_timeout)
                .max(Duration::from_millis(1)),
        )
        .build();

    match agent.post(url).send_json(payload) {
        Ok(response) => Ok(LoginCandidateResponse::Response(response)),
        Err(ureq::Error::Status(_, response)) => Ok(LoginCandidateResponse::Response(response)),
        Err(error) => {
            if deadline.remaining().is_err() {
                Err("task_budget_exhausted".to_string())
            } else {
                Ok(LoginCandidateResponse::Transient(format!(
                    "network_timeout:{error}"
                )))
            }
        }
    }
}

fn login_attempt_from_response(path: &str, status: u16, parsed: &Value) -> LoginAttempt {
    if let Some(token) = extract_token(&parsed) {
        return LoginAttempt {
            token: Some(token),
            login_message: Some(format!("已从 {path} 读取登录 token。")),
            manual_required: None,
        };
    }
    if is_region_restricted_login(&parsed, status) {
        return LoginAttempt {
            token: None,
            login_message: Some(shorten_error(&parsed.to_string())),
            manual_required: Some(
                "登录接口返回地区限制，当前网络可能需要代理；请在设置或站点代理中启用可访问该中转站的代理。"
                    .to_string(),
            ),
        };
    }
    if needs_manual_login(&parsed, status) {
        return LoginAttempt {
            token: None,
            login_message: Some(shorten_error(&parsed.to_string())),
            manual_required: Some("接口需要验证码、2FA 或额外登录步骤。".to_string()),
        };
    }
    LoginAttempt {
        token: None,
        login_message: Some(shorten_error(&parsed.to_string())),
        manual_required: None,
    }
}

fn is_region_restricted_login(value: &Value, status: u16) -> bool {
    if status != 403 {
        return false;
    }
    let text = value.to_string().to_lowercase();
    text.contains("region_restricted")
        || text.contains("地区")
        || text.contains("区域")
        || text.contains("region")
}

fn retry_after_duration(response: &ureq::Response) -> Option<Duration> {
    response
        .header("retry-after")
        .and_then(|value| value.trim().parse::<u64>().ok())
        .map(Duration::from_secs)
}

fn login_transient_failure(kind: &str) -> LoginAttempt {
    LoginAttempt {
        token: None,
        login_message: Some(kind.to_string()),
        manual_required: Some(kind.to_string()),
    }
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
        401 | 403 => "接口需要登录；保存账号密码后可重试采集，验证码或 2FA 场景可使用网页登录捕获",
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
        "需要登录" if needs_login => {
            "接口需要登录；请先保存站点登录账号和密码，验证码或 2FA 场景可使用网页登录捕获。"
                .to_string()
        }
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

    collect_group_endpoint_records(raw, &mut groups, &mut rate_multipliers);
    collect_structured_groups(raw, &mut groups);
    collect_structured_rate_multipliers(raw, &mut rate_multipliers);

    for item in &matches {
        let field = item
            .get("field")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_lowercase();
        let path = item
            .get("path")
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
        if let Some(group) = normalize_group_value(&field, &value) {
            push_unique_value(&mut groups, group);
        }
        if let Some(rate_multiplier) = normalize_rate_multiplier_value(&field, &path, &value) {
            push_unique_rate_multiplier(&mut rate_multipliers, rate_multiplier);
        }
        if matches_any(&field, &["api_key", "key", "token"]) {
            push_unique_value(&mut keys, redact_value(&value));
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

fn collect_group_endpoint_records(
    raw: &Value,
    groups: &mut Vec<Value>,
    rate_multipliers: &mut Vec<Value>,
) {
    let Some(responses) = raw.get("responses").and_then(Value::as_array) else {
        return;
    };

    let mut group_names_by_id = Map::new();

    for response in responses {
        let path = response
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if path != "/api/v1/groups/available" {
            continue;
        }
        if let Some(payload) = response.get("json") {
            collect_available_group_payload(
                payload,
                groups,
                rate_multipliers,
                &mut group_names_by_id,
            );
        }
    }

    for response in responses {
        let path = response
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if path != "/api/v1/groups/rates" {
            continue;
        }
        if let Some(payload) = response.get("json") {
            collect_user_group_rate_payload(payload, rate_multipliers, &group_names_by_id);
        }
    }
}

fn collect_available_group_payload(
    value: &Value,
    groups: &mut Vec<Value>,
    rate_multipliers: &mut Vec<Value>,
    group_names_by_id: &mut Map<String, Value>,
) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_available_group_payload(item, groups, rate_multipliers, group_names_by_id);
            }
        }
        Value::Object(map) => {
            if let Some(group) = group_identifier_from_available_group_record(map) {
                push_unique_value(groups, group.clone());
                if let Some(id) = map.get("id").and_then(scalar_value) {
                    group_names_by_id.insert(stable_value_key(&id), group.clone());
                }
                if let Some(multiplier) = map.get("rate_multiplier").and_then(scalar_value) {
                    upsert_rate_multiplier_by_group(rate_multipliers, group, multiplier);
                }
                return;
            }

            for field in ["data", "items", "groups", "records"] {
                if let Some(child) = map.get(field) {
                    collect_available_group_payload(
                        child,
                        groups,
                        rate_multipliers,
                        group_names_by_id,
                    );
                }
            }
        }
        _ => {}
    }
}

fn collect_user_group_rate_payload(
    value: &Value,
    rate_multipliers: &mut Vec<Value>,
    group_names_by_id: &Map<String, Value>,
) {
    match value {
        Value::Object(map) => {
            if is_numeric_rate_map(map) {
                for (group_id, multiplier) in map {
                    let Some(multiplier) = scalar_value(multiplier) else {
                        continue;
                    };
                    let group = group_names_by_id
                        .get(group_id)
                        .cloned()
                        .unwrap_or_else(|| Value::String(group_id.clone()));
                    upsert_rate_multiplier_by_group(rate_multipliers, group, multiplier);
                }
                return;
            }

            for field in ["data", "rates", "items", "groups", "records"] {
                if let Some(child) = map.get(field) {
                    collect_user_group_rate_payload(child, rate_multipliers, group_names_by_id);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_user_group_rate_payload(item, rate_multipliers, group_names_by_id);
            }
        }
        _ => {}
    }
}

fn is_numeric_rate_map(map: &Map<String, Value>) -> bool {
    !map.is_empty()
        && map
            .iter()
            .all(|(key, value)| key.parse::<i64>().is_ok() && scalar_value(value).is_some())
}

fn group_identifier_from_available_group_record(map: &Map<String, Value>) -> Option<Value> {
    if !map.contains_key("rate_multiplier") {
        return None;
    }

    find_record_scalar(map, &["name", "group_name", "group", "tier"])
}

fn collect_structured_groups(value: &Value, groups: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                let field = key.to_lowercase();
                if is_group_collection_field(&field) {
                    collect_group_collection(child, groups);
                } else if is_group_rate_map_field(&field) {
                    collect_group_keys(child, groups);
                } else if field == "group" {
                    if let Value::Object(group) = child {
                        if let Some(group) = group_identifier_from_record(group) {
                            push_unique_value(groups, group);
                        }
                    }
                }
                collect_structured_groups(child, groups);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_structured_groups(item, groups);
            }
        }
        _ => {}
    }
}

fn collect_group_collection(value: &Value, groups: &mut Vec<Value>) {
    match value {
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::Object(map) => {
                        if let Some(group) = group_identifier_from_record(map) {
                            push_unique_value(groups, group);
                        }
                    }
                    _ => {
                        if let Some(group) = scalar_value(item) {
                            push_unique_value(groups, group);
                        }
                    }
                }
            }
        }
        Value::Object(map) => {
            if let Some(group) = group_identifier_from_record(map) {
                push_unique_value(groups, group);
            } else {
                for (name, child) in map {
                    if !is_metadata_key(name) && !child.is_null() {
                        push_unique_value(groups, Value::String(name.clone()));
                    }
                }
            }
        }
        _ => {
            if let Some(group) = scalar_value(value) {
                push_unique_value(groups, group);
            }
        }
    }
}

fn collect_group_keys(value: &Value, groups: &mut Vec<Value>) {
    if let Value::Object(map) = value {
        for (name, child) in map {
            if !is_metadata_key(name) && scalar_value(child).is_some() {
                push_unique_value(groups, Value::String(name.clone()));
            }
        }
    }
}

fn collect_structured_rate_multipliers(value: &Value, rate_multipliers: &mut Vec<Value>) {
    match value {
        Value::Object(map) => {
            if let Some(rate) = rate_multiplier_from_record(map) {
                push_unique_rate_multiplier(rate_multipliers, rate);
            }
            for (key, child) in map {
                let field = key.to_lowercase();
                if is_group_rate_map_field(&field) {
                    collect_rate_multiplier_map(child, rate_multipliers);
                } else if field == "group" {
                    if let Value::Object(group) = child {
                        if let Some(rate) = rate_multiplier_from_embedded_group_record(group) {
                            push_unique_rate_multiplier(rate_multipliers, rate);
                        }
                    }
                }
                collect_structured_rate_multipliers(child, rate_multipliers);
            }
        }
        Value::Array(items) => {
            for item in items {
                collect_structured_rate_multipliers(item, rate_multipliers);
            }
        }
        _ => {}
    }
}

fn collect_rate_multiplier_map(value: &Value, rate_multipliers: &mut Vec<Value>) {
    if let Value::Object(map) = value {
        for (group, multiplier) in map {
            if is_metadata_key(group) {
                continue;
            }
            if let Some(multiplier) = scalar_value(multiplier) {
                push_unique_rate_multiplier(
                    rate_multipliers,
                    json!({
                        "group": group,
                        "multiplier": multiplier,
                    }),
                );
            }
        }
    }
}

fn rate_multiplier_from_record(map: &Map<String, Value>) -> Option<Value> {
    let multiplier = find_record_scalar(
        map,
        &[
            "rate_multiplier",
            "group_rate",
            "group_ratio",
            "ratio",
            "multiplier",
        ],
    )?;
    let group = group_identifier_for_rate_record(map);

    group.map(|group| {
        json!({
            "group": group,
            "multiplier": multiplier,
        })
    })
}

fn rate_multiplier_from_embedded_group_record(map: &Map<String, Value>) -> Option<Value> {
    let multiplier = find_record_scalar(map, &["rate_multiplier", "group_rate", "group_ratio"])?;
    let group = group_identifier_from_record(map)?;

    Some(json!({
        "group": group,
        "multiplier": multiplier,
    }))
}

fn group_identifier_from_record(map: &Map<String, Value>) -> Option<Value> {
    find_record_scalar(map, &["group_name", "group", "group_id", "tier", "name"])
}

fn group_identifier_for_rate_record(map: &Map<String, Value>) -> Option<Value> {
    find_record_scalar(map, &["group_name", "group", "group_id", "tier"])
}

fn find_record_scalar(map: &Map<String, Value>, fields: &[&str]) -> Option<Value> {
    for field in fields {
        if let Some(value) = map.get(*field).and_then(scalar_value) {
            return Some(value);
        }
    }
    None
}

fn is_group_collection_field(field: &str) -> bool {
    matches!(
        field,
        "groups" | "group_list" | "group_infos" | "group_info"
    )
}

fn is_group_rate_map_field(field: &str) -> bool {
    matches!(
        field,
        "group_ratio"
            | "group_ratios"
            | "group_rate"
            | "group_rates"
            | "group_multiplier"
            | "group_multipliers"
            | "rate_multipliers"
    )
}

fn is_model_rate_field(field: &str) -> bool {
    field.contains("model") && matches_any(field, &["ratio", "multiplier", "rate"])
}

fn is_metadata_key(field: &str) -> bool {
    matches!(
        field.to_lowercase().as_str(),
        "id" | "name" | "data" | "message" | "code" | "status" | "success" | "total" | "count"
    )
}

fn normalize_group_value(field: &str, value: &Value) -> Option<Value> {
    if field.contains("group_name") || field == "group" || field.ends_with("_group") {
        return scalar_value(value);
    }

    None
}

fn normalize_rate_multiplier_value(field: &str, path: &str, value: &Value) -> Option<Value> {
    if is_model_rate_field(field)
        || field.contains("image")
        || field.contains("peak")
        || path.contains("model")
        || path.contains("image")
        || path.contains("peak")
        || field == "groups"
    {
        return None;
    }
    if !matches_any(field, &["rate_multiplier", "ratio", "multiplier"]) {
        return None;
    }

    scalar_value(value)
}

fn scalar_value(value: &Value) -> Option<Value> {
    match value {
        Value::String(_) | Value::Number(_) | Value::Bool(_) => Some(value.clone()),
        _ => None,
    }
}

fn push_unique_value(items: &mut Vec<Value>, value: Value) {
    let key = stable_value_key(&value);
    if items.iter().any(|item| stable_value_key(item) == key) {
        return;
    }
    items.push(value);
}

fn push_unique_rate_multiplier(items: &mut Vec<Value>, value: Value) {
    let value_key = stable_value_key(&value);
    if items.iter().any(|item| stable_value_key(item) == value_key) {
        return;
    }

    if scalar_value(&value).is_some() {
        if items
            .iter()
            .any(|item| item.get("group").is_some() && item.get("multiplier").is_some())
        {
            return;
        }
        if items.iter().any(|item| {
            item.get("multiplier")
                .map(stable_value_key)
                .map(|key| key == value_key)
                .unwrap_or(false)
        }) {
            return;
        }
    }

    items.push(value);
}

fn upsert_rate_multiplier_by_group(items: &mut Vec<Value>, group: Value, multiplier: Value) {
    let group_key = stable_value_key(&group);
    let value = json!({
        "group": group,
        "multiplier": multiplier,
    });

    if let Some(existing) = items.iter_mut().find(|item| {
        item.get("group")
            .map(stable_value_key)
            .map(|key| key == group_key)
            .unwrap_or(false)
    }) {
        *existing = value;
    } else {
        items.push(value);
    }
}

fn stable_value_key(value: &Value) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| value.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{credentials::UpdateStationCredentialsInput, stations::CreateStationInput},
        services::{database::AppDatabase, secrets::crypto::generate_data_key},
    };
    use serde_json::Value;
    use std::{
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        sync::{Arc, Mutex},
        thread,
    };

    #[test]
    fn collect_login_state_uses_saved_password_and_normalizes_account_data() {
        let server = TestCollectorServer::start();
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let station = database
            .create_station(CreateStationInput {
                name: "collector test".to_string(),
                station_type: "sub2api".to_string(),
                base_url: server.base_url.clone(),
                collector_proxy_mode: "inherit".to_string(),
                collector_proxy_url: None,
                api_key: "sk-test-routing".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let data_key = generate_data_key();
        database
            .update_station_credentials_with_data_key(
                UpdateStationCredentialsInput {
                    station_id: station.id.clone(),
                    login_username: Some("user@example.test".to_string()),
                    login_password: Some("correct-password".to_string()),
                    remember_password: true,
                },
                &data_key,
            )
            .expect("credentials");

        let result = collect_login_state(&database, &data_key, station.id).expect("collect");
        let normalized = result.snapshot.normalized_json;
        let raw_text = serde_json::to_string(&result.snapshot.raw_json_redacted).expect("raw");

        assert_eq!(result.snapshot.status, "success");
        assert_eq!(
            server.last_login_password(),
            Some("correct-password".to_string())
        );
        assert_eq!(
            normalized.get("balance").and_then(Value::as_f64),
            Some(42.5)
        );
        assert_eq!(
            normalized
                .get("groups")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
        );
        assert_eq!(
            normalized
                .get("rateMultipliers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
        );
        assert!(!raw_text.contains("correct-password"));
        assert!(!raw_text.contains("collector-token-secret"));
    }

    #[test]
    fn normalize_probe_counts_group_lists_and_group_ratio_maps() {
        let raw = json!({
            "responses": [
                {
                    "path": "/api/v1/auth/me",
                    "json": {
                        "data": {
                            "balance": 12.0,
                            "groups": ["default", "vip"]
                        }
                    }
                },
                {
                    "path": "/api/v1/groups/rates",
                    "json": {
                        "data": {
                            "group_ratio": {
                                "default": 1.0,
                                "vip": 1.5
                            },
                            "model_ratio": {
                                "gpt-4o-mini": 0.7
                            }
                        }
                    }
                }
            ]
        });

        let normalized = normalize_probe(&raw);

        assert_eq!(
            normalized
                .get("groups")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
        );
        assert_eq!(
            normalized
                .get("rateMultipliers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(2),
        );
    }

    #[test]
    fn login_region_restricted_reports_proxy_needed_not_captcha() {
        let attempt = login_attempt_from_response(
            "/api/v1/auth/login",
            403,
            &json!({
                "code": 403,
                "message": "当前地区暂不支持注册或登录",
                "reason": "REGION_RESTRICTED"
            }),
        );

        assert_eq!(attempt.token, None);
        let manual_required = attempt.manual_required.expect("manual required");
        assert!(manual_required.contains("当前网络"));
        assert!(manual_required.contains("代理"));
        assert!(!manual_required.contains("验证码"));
        assert!(!manual_required.contains("2FA"));
    }

    #[test]
    fn normalize_probe_prefers_embedded_group_name_and_excludes_image_multiplier() {
        let raw = json!({
            "responses": [
                {
                    "path": "/api/v1/keys",
                    "json": {
                        "data": {
                            "items": [
                                {
                                    "group_id": 8,
                                    "group": {
                                        "id": 8,
                                        "name": "plus",
                                        "image_rate_multiplier": 1,
                                        "rate_multiplier": 1,
                                        "status": "active"
                                    }
                                }
                            ]
                        }
                    }
                }
            ]
        });

        let normalized = normalize_probe(&raw);

        assert_eq!(
            normalized.get("groups").and_then(Value::as_array).cloned(),
            Some(vec![Value::String("plus".to_string())]),
        );
        assert_eq!(
            normalized
                .get("rateMultipliers")
                .and_then(Value::as_array)
                .map(Vec::len),
            Some(1),
        );
    }

    #[test]
    fn normalize_probe_collects_all_available_groups_without_image_rates() {
        let raw = json!({
            "responses": [
                {
                    "path": "/api/v1/groups/available",
                    "json": {
                        "data": [
                            {
                                "id": 8,
                                "name": "plus",
                                "rate_multiplier": 1,
                                "peak_rate_multiplier": 1.2,
                                "image_rate_multiplier": 3,
                                "status": "active"
                            },
                            {
                                "id": 9,
                                "name": "pro",
                                "rate_multiplier": 1.5,
                                "peak_rate_multiplier": 2,
                                "image_rate_multiplier": 4,
                                "status": "active"
                            }
                        ]
                    }
                },
                {
                    "path": "/api/v1/keys",
                    "json": {
                        "data": {
                            "items": [
                                {
                                    "group": {
                                        "id": 8,
                                        "name": "plus",
                                        "rate_multiplier": 1
                                    }
                                }
                            ]
                        }
                    }
                }
            ]
        });

        let normalized = normalize_probe(&raw);

        assert_eq!(
            normalized.get("groups").and_then(Value::as_array).cloned(),
            Some(vec![
                Value::String("plus".to_string()),
                Value::String("pro".to_string()),
            ]),
        );
        assert_eq!(
            normalized
                .get("rateMultipliers")
                .and_then(Value::as_array)
                .cloned(),
            Some(vec![
                json!({ "group": "plus", "multiplier": 1 }),
                json!({ "group": "pro", "multiplier": 1.5 }),
            ]),
        );
    }

    #[test]
    fn normalize_probe_joins_user_group_rates_to_available_group_names() {
        let raw = json!({
            "responses": [
                {
                    "path": "/api/v1/groups/available",
                    "json": {
                        "data": [
                            {
                                "id": 8,
                                "name": "plus",
                                "rate_multiplier": 1,
                                "image_rate_multiplier": 3
                            },
                            {
                                "id": 9,
                                "name": "pro",
                                "rate_multiplier": 1.5,
                                "image_rate_multiplier": 4
                            },
                            {
                                "id": 10,
                                "name": "backup",
                                "rate_multiplier": 2
                            }
                        ]
                    }
                },
                {
                    "path": "/api/v1/groups/rates",
                    "json": {
                        "data": {
                            "8": 0.8,
                            "9": 1.2
                        }
                    }
                }
            ]
        });

        let normalized = normalize_probe(&raw);

        assert_eq!(
            normalized
                .get("rateMultipliers")
                .and_then(Value::as_array)
                .cloned(),
            Some(vec![
                json!({ "group": "plus", "multiplier": 0.8 }),
                json!({ "group": "pro", "multiplier": 1.2 }),
                json!({ "group": "backup", "multiplier": 2 }),
            ]),
        );
    }

    #[test]
    fn login_recovery_stops_when_budget_is_exhausted() {
        let server = SlowLoginServer::start();

        let outcome = login_access_token_with_budget(
            &server.base_url,
            "user@example.test",
            "secret",
            Duration::from_millis(30),
        );

        assert!(matches!(outcome, Err(error) if error.contains("task_budget_exhausted")));
        assert_eq!(server.request_count(), 1);
    }

    #[test]
    fn login_recovery_does_not_restart_candidate_sequence() {
        let server = FlakyLoginServer::start();

        let outcome = login_access_token_with_budget(
            &server.base_url,
            "user@example.test",
            "secret",
            Duration::from_secs(1),
        )
        .expect("login outcome");

        assert_eq!(outcome.access_token.as_deref(), Some("fresh-token"));
        assert!(server.request_count() <= 2);
    }

    #[test]
    fn login_recovery_rejects_retry_after_that_exceeds_budget() {
        let server = RateLimitedLoginServer::start();

        let outcome = login_access_token_with_budget(
            &server.base_url,
            "user@example.test",
            "secret",
            Duration::from_millis(100),
        );

        assert!(matches!(outcome, Err(error) if error.contains("task_budget_exhausted")));
        assert_eq!(server.request_count(), 1);
    }

    struct TestCollectorServer {
        base_url: String,
        last_login_password: Arc<Mutex<Option<String>>>,
    }

    struct SlowLoginServer {
        base_url: String,
        request_count: Arc<Mutex<usize>>,
    }

    struct FlakyLoginServer {
        base_url: String,
        request_count: Arc<Mutex<usize>>,
    }

    struct RateLimitedLoginServer {
        base_url: String,
        request_count: Arc<Mutex<usize>>,
    }

    impl TestCollectorServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind test server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let last_login_password = Arc::new(Mutex::new(None));
            let captured_password = Arc::clone(&last_login_password);

            thread::spawn(move || {
                for stream in listener.incoming().take(8).flatten() {
                    handle_test_request(stream, Arc::clone(&captured_password));
                }
            });

            Self {
                base_url,
                last_login_password,
            }
        }

        fn last_login_password(&self) -> Option<String> {
            self.last_login_password
                .lock()
                .ok()
                .and_then(|value| value.clone())
        }
    }

    impl SlowLoginServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind slow login server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let request_count = Arc::new(Mutex::new(0));
            let captured_count = Arc::clone(&request_count);

            thread::spawn(move || {
                for stream in listener.incoming().take(3).flatten() {
                    handle_slow_login_request(stream, Arc::clone(&captured_count));
                }
            });

            Self {
                base_url,
                request_count,
            }
        }

        fn request_count(&self) -> usize {
            self.request_count.lock().map(|count| *count).unwrap_or(0)
        }
    }

    impl FlakyLoginServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind flaky login server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let request_count = Arc::new(Mutex::new(0));
            let captured_count = Arc::clone(&request_count);

            thread::spawn(move || {
                for stream in listener.incoming().take(3).flatten() {
                    handle_flaky_login_request(stream, Arc::clone(&captured_count));
                }
            });

            Self {
                base_url,
                request_count,
            }
        }

        fn request_count(&self) -> usize {
            self.request_count.lock().map(|count| *count).unwrap_or(0)
        }
    }

    impl RateLimitedLoginServer {
        fn start() -> Self {
            let listener = TcpListener::bind("127.0.0.1:0").expect("bind rate login server");
            let base_url = format!("http://{}", listener.local_addr().expect("addr"));
            let request_count = Arc::new(Mutex::new(0));
            let captured_count = Arc::clone(&request_count);

            thread::spawn(move || {
                for stream in listener.incoming().take(2).flatten() {
                    handle_rate_limited_login_request(stream, Arc::clone(&captured_count));
                }
            });

            Self {
                base_url,
                request_count,
            }
        }

        fn request_count(&self) -> usize {
            self.request_count.lock().map(|count| *count).unwrap_or(0)
        }
    }

    fn handle_test_request(mut stream: TcpStream, last_login_password: Arc<Mutex<Option<String>>>) {
        let request = read_http_request(&mut stream);
        let path = request
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("/");
        let body = request.split("\r\n\r\n").nth(1).unwrap_or("");

        let (status, response) = match path {
            "/api/v1/auth/login" => {
                let parsed = serde_json::from_str::<Value>(body).expect("login json");
                let password = parsed
                    .get("password")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if let Ok(mut captured) = last_login_password.lock() {
                    *captured = Some(password.clone());
                }
                if password == "correct-password" {
                    (
                        "200 OK",
                        json!({ "data": { "access_token": "collector-token-secret" } }),
                    )
                } else {
                    (
                        "401 Unauthorized",
                        json!({ "message": "invalid credentials" }),
                    )
                }
            }
            "/api/v1/auth/me" => (
                "200 OK",
                json!({
                    "data": {
                        "balance": 42.5,
                        "group_name": "pro",
                    }
                }),
            ),
            "/api/v1/groups/available" => (
                "200 OK",
                json!({
                    "data": [
                        {
                            "id": 8,
                            "name": "plus",
                            "rate_multiplier": 1,
                            "peak_rate_multiplier": 1.2,
                            "image_rate_multiplier": 3,
                            "status": "active"
                        },
                        {
                            "id": 9,
                            "name": "pro",
                            "rate_multiplier": 1.25,
                            "peak_rate_multiplier": 2,
                            "image_rate_multiplier": 4,
                            "status": "active"
                        }
                    ]
                }),
            ),
            "/api/v1/groups/rates" => (
                "200 OK",
                json!({
                    "data": {
                        "groups": [
                            { "group_name": "pro", "rate_multiplier": 1.25 }
                        ]
                    }
                }),
            ),
            _ => ("404 Not Found", json!({ "message": "not found" })),
        };
        let text = response.to_string();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn handle_slow_login_request(mut stream: TcpStream, request_count: Arc<Mutex<usize>>) {
        let _request = read_http_request(&mut stream);
        if let Ok(mut count) = request_count.lock() {
            *count += 1;
        }
        thread::sleep(Duration::from_millis(120));
        write_json_response(
            stream,
            "200 OK",
            json!({ "data": { "access_token": "late-token" } }),
            None,
        );
    }

    fn handle_flaky_login_request(mut stream: TcpStream, request_count: Arc<Mutex<usize>>) {
        let _request = read_http_request(&mut stream);
        let attempt = request_count
            .lock()
            .map(|mut count| {
                *count += 1;
                *count
            })
            .unwrap_or(1);
        if attempt == 1 {
            write_json_response(
                stream,
                "502 Bad Gateway",
                json!({ "message": "temporary upstream failure" }),
                None,
            );
        } else {
            write_json_response(
                stream,
                "200 OK",
                json!({ "data": { "access_token": "fresh-token" } }),
                None,
            );
        }
    }

    fn handle_rate_limited_login_request(mut stream: TcpStream, request_count: Arc<Mutex<usize>>) {
        let _request = read_http_request(&mut stream);
        if let Ok(mut count) = request_count.lock() {
            *count += 1;
        }
        write_json_response(
            stream,
            "429 Too Many Requests",
            json!({ "message": "slow down" }),
            Some("Retry-After: 1\r\n"),
        );
    }

    fn write_json_response(
        mut stream: TcpStream,
        status: &str,
        response: Value,
        extra_header: Option<&str>,
    ) {
        let text = response.to_string();
        let extra_header = extra_header.unwrap_or("");
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\n{extra_header}Content-Length: {}\r\nConnection: close\r\n\r\n{text}",
            text.len()
        );
        stream
            .write_all(response.as_bytes())
            .expect("write response");
    }

    fn read_http_request(stream: &mut TcpStream) -> String {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 1024];
        let mut header_end = None;
        let mut content_length = 0_usize;

        loop {
            let read = stream.read(&mut buffer).expect("read request");
            if read == 0 {
                break;
            }
            bytes.extend_from_slice(&buffer[..read]);
            if header_end.is_none() {
                if let Some(position) = bytes.windows(4).position(|window| window == b"\r\n\r\n") {
                    header_end = Some(position + 4);
                    let headers = String::from_utf8_lossy(&bytes[..position]);
                    content_length = headers
                        .lines()
                        .find_map(|line| {
                            let (name, value) = line.split_once(':')?;
                            if name.eq_ignore_ascii_case("content-length") {
                                value.trim().parse::<usize>().ok()
                            } else {
                                None
                            }
                        })
                        .unwrap_or(0);
                }
            }
            if let Some(end) = header_end {
                if bytes.len().saturating_sub(end) >= content_length {
                    break;
                }
            }
        }

        String::from_utf8_lossy(&bytes).to_string()
    }
}
