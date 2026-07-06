pub mod probe;
pub mod redaction;
pub mod templates;

use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, OnceLock,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    models::{
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CreateChannelMonitorRunInput,
        },
        station_keys::KeyPoolItem,
    },
    services::{
        channel_monitors::{
            probe::{run_monitor_probe, MonitorProbeResult},
            redaction::redact_monitor_text,
            templates::{render_monitor_request, MonitorTemplateContext},
        },
        database::{now_millis_for_services, AppDatabase},
        endpoint_ping::ping_station_endpoint,
    },
};

const RUNNER_POLL_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_STOP_SLICE: Duration = Duration::from_millis(250);
const DEFAULT_MONITOR_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MONITOR_CHALLENGE: &str = "ping";
const MONITOR_ALREADY_RUNNING_ERROR: &str = "Channel monitor is already running";
static ACTIVE_MONITOR_RUNS: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();

#[allow(dead_code)]
pub fn run_channel_monitor_now(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor_id: &str,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let monitor = database.get_channel_monitor(monitor_id)?;
    run_monitor(database, data_key, monitor)
}

fn run_monitor(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor: ChannelMonitor,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let monitor_id = monitor.id.clone();
    let result = run_monitor_once_guarded(database, data_key, monitor, None);
    schedule_after_started_monitor(database, &monitor_id, result)
}

fn run_monitor_once_guarded(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor: ChannelMonitor,
    stop_requested: Option<&AtomicBool>,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let _guard = MonitorRunGuard::try_start(&monitor.id)?;
    run_monitor_once(database, data_key, monitor, stop_requested)
}

fn run_monitor_once(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor: ChannelMonitor,
    stop_requested: Option<&AtomicBool>,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let template = database.get_channel_monitor_template(&monitor.template_id)?;
    let targets = monitor_targets(database, &monitor)?;

    if targets.is_empty() {
        let run = insert_skipped_run(database, &monitor)?;
        database.update_channel_monitor_after_run(
            &monitor.id,
            &run.status,
            run.finished_at.as_deref().unwrap_or(&run.started_at),
            run.error_message.as_deref(),
        )?;
        return Ok(vec![run]);
    }

    update_station_endpoint_pings(database, &monitor, &targets)?;

    let max_concurrency = if monitor.target_type == "station" {
        monitor.max_concurrency.clamp(1, 16) as usize
    } else {
        1
    };
    let mut runs = Vec::with_capacity(targets.len());
    let mut next_target = 0;
    while next_target < targets.len() {
        if monitor_stop_requested(stop_requested) {
            break;
        }
        let batch_end = (next_target + max_concurrency).min(targets.len());
        let mut handles = Vec::with_capacity(batch_end - next_target);
        for target in &targets[next_target..batch_end] {
            if monitor_stop_requested(stop_requested) {
                break;
            }
            let database = database.clone();
            let data_key = *data_key;
            let monitor = monitor.clone();
            let template = template.clone();
            let target = target.clone();
            handles.push(thread::spawn(move || {
                run_monitor_for_key(&database, &data_key, &monitor, &template, &target)
            }));
        }
        if handles.is_empty() {
            break;
        }
        next_target += handles.len();
        for handle in handles {
            let run = handle
                .join()
                .map_err(|_| "Channel monitor probe worker panicked".to_string())??;
            database.update_channel_monitor_after_run(
                &monitor.id,
                &run.status,
                run.finished_at.as_deref().unwrap_or(&run.started_at),
                run.error_message.as_deref(),
            )?;
            runs.push(run);
        }
    }
    Ok(runs)
}

fn update_station_endpoint_pings(
    database: &AppDatabase,
    monitor: &ChannelMonitor,
    targets: &[KeyPoolItem],
) -> Result<(), String> {
    let mut seen_station_ids = HashSet::new();
    for target in targets {
        if seen_station_ids.insert(target.station_id.clone()) {
            update_station_endpoint_ping(
                database,
                &target.station_id,
                &target.station_base_url,
                monitor.timeout_seconds,
            )?;
        }
    }
    Ok(())
}

fn update_station_endpoint_ping(
    database: &AppDatabase,
    station_id: &str,
    station_base_url: &str,
    timeout_seconds: i64,
) -> Result<(), String> {
    let timeout = Duration::from_secs(timeout_seconds.max(1) as u64);
    let result = ping_station_endpoint(station_base_url, timeout);
    let checked_at = now_string();
    database.upsert_station_endpoint_health(
        station_id,
        &result.status,
        result.latency_ms,
        &checked_at,
        result.error_summary.as_deref(),
    )?;
    Ok(())
}

fn schedule_after_started_monitor<T>(
    database: &AppDatabase,
    monitor_id: &str,
    result: Result<T, String>,
) -> Result<T, String> {
    if let Err(error) = result.as_ref() {
        if is_monitor_already_running_error(error) {
            return result;
        }
    }
    let schedule_result = database.schedule_next_channel_monitor_run(monitor_id);
    match (result, schedule_result) {
        (Ok(value), Ok(_)) => Ok(value),
        (Err(error), Ok(_)) => Err(error),
        (Ok(_), Err(schedule_error)) => Err(schedule_error),
        (Err(error), Err(schedule_error)) => Err(format!(
            "{error}; failed to schedule next channel monitor run: {schedule_error}"
        )),
    }
}

fn monitor_stop_requested(stop_requested: Option<&AtomicBool>) -> bool {
    stop_requested.is_some_and(|stop_requested| stop_requested.load(Ordering::Relaxed))
}

fn is_monitor_already_running_error(error: &str) -> bool {
    error == MONITOR_ALREADY_RUNNING_ERROR
}

struct MonitorRunGuard {
    monitor_id: String,
}

impl MonitorRunGuard {
    fn try_start(monitor_id: &str) -> Result<Self, String> {
        let active_runs = ACTIVE_MONITOR_RUNS.get_or_init(|| Mutex::new(HashSet::new()));
        let mut active_runs = active_runs
            .lock()
            .map_err(|_| "Channel monitor run guard is unavailable".to_string())?;
        if !active_runs.insert(monitor_id.to_string()) {
            return Err(MONITOR_ALREADY_RUNNING_ERROR.to_string());
        }
        Ok(Self {
            monitor_id: monitor_id.to_string(),
        })
    }
}

impl Drop for MonitorRunGuard {
    fn drop(&mut self) {
        if let Some(active_runs) = ACTIVE_MONITOR_RUNS.get() {
            if let Ok(mut active_runs) = active_runs.lock() {
                active_runs.remove(&self.monitor_id);
            }
        }
    }
}

fn monitor_targets(
    database: &AppDatabase,
    monitor: &ChannelMonitor,
) -> Result<Vec<KeyPoolItem>, String> {
    let keys = database.list_key_pool_items()?;
    let targets = keys
        .into_iter()
        .filter(|key| key.enabled)
        .filter(|key| match monitor.target_type.as_str() {
            "station_key" => monitor.station_key_id.as_deref() == Some(key.id.as_str()),
            "station" => key.station_id == monitor.station_id,
            _ => false,
        })
        .collect();
    Ok(targets)
}

fn run_monitor_for_key(
    database: &AppDatabase,
    data_key: &[u8; 32],
    monitor: &ChannelMonitor,
    template: &ChannelMonitorRequestTemplate,
    target: &KeyPoolItem,
) -> Result<ChannelMonitorRun, String> {
    let started_at = now_string();
    let model = monitor_model(monitor);
    let context = MonitorTemplateContext {
        model: model.clone(),
        max_tokens: 1,
        stream: false,
        challenge: DEFAULT_MONITOR_CHALLENGE.to_string(),
    };

    let api_key = match database.resolve_station_key_secret_with_data_key(data_key, &target.id) {
        Ok(api_key) => api_key,
        Err(error) => {
            return insert_failed_key_run(
                database,
                monitor,
                &target.id,
                &started_at,
                &model,
                None,
                short_error(&error),
            )
        }
    };
    let request = match render_monitor_request(template, &context) {
        Ok(request) => request,
        Err(error) => {
            return insert_failed_key_run(
                database,
                monitor,
                &target.id,
                &started_at,
                &model,
                None,
                short_error(&error),
            )
        }
    };
    let result = run_monitor_probe(
        &target.station_base_url,
        &api_key,
        &request,
        monitor.timeout_seconds,
    );
    insert_probe_run(database, monitor, &target.id, &started_at, &model, result)
}

fn insert_probe_run(
    database: &AppDatabase,
    monitor: &ChannelMonitor,
    station_key_id: &str,
    started_at: &str,
    model: &str,
    result: MonitorProbeResult,
) -> Result<ChannelMonitorRun, String> {
    let finished_at = now_string();
    let duration_ms = duration_between(started_at, &finished_at);
    if result.ok {
        database.record_station_key_success(station_key_id, result.latency_ms, &finished_at)?;
    } else {
        database.record_station_key_failure_with_threshold(
            station_key_id,
            &short_error(
                result
                    .error_summary
                    .as_deref()
                    .unwrap_or("Channel monitor probe failed"),
            ),
            &finished_at,
            monitor.consecutive_failure_threshold,
        )?;
    }
    database.insert_channel_monitor_run(CreateChannelMonitorRunInput {
        monitor_id: monitor.id.clone(),
        template_id: monitor.template_id.clone(),
        station_id: monitor.station_id.clone(),
        station_key_id: Some(station_key_id.to_string()),
        status: if result.ok { "success" } else { "failed" }.to_string(),
        started_at: started_at.to_string(),
        finished_at: Some(finished_at),
        duration_ms,
        http_status: result.status_code.map(i64::from),
        latency_ms: Some(result.latency_ms),
        response_model: Some(model.to_string()),
        fallback_model: None,
        error_message: result.error_summary.map(|error| short_error(&error)),
    })
}

fn insert_failed_key_run(
    database: &AppDatabase,
    monitor: &ChannelMonitor,
    station_key_id: &str,
    started_at: &str,
    model: &str,
    http_status: Option<i64>,
    error_message: String,
) -> Result<ChannelMonitorRun, String> {
    let finished_at = now_string();
    database.record_station_key_failure_with_threshold(
        station_key_id,
        &error_message,
        &finished_at,
        monitor.consecutive_failure_threshold,
    )?;
    database.insert_channel_monitor_run(CreateChannelMonitorRunInput {
        monitor_id: monitor.id.clone(),
        template_id: monitor.template_id.clone(),
        station_id: monitor.station_id.clone(),
        station_key_id: Some(station_key_id.to_string()),
        status: "failed".to_string(),
        started_at: started_at.to_string(),
        finished_at: Some(finished_at.clone()),
        duration_ms: duration_between(started_at, &finished_at),
        http_status,
        latency_ms: None,
        response_model: Some(model.to_string()),
        fallback_model: None,
        error_message: Some(error_message),
    })
}

fn insert_skipped_run(
    database: &AppDatabase,
    monitor: &ChannelMonitor,
) -> Result<ChannelMonitorRun, String> {
    let started_at = now_string();
    let finished_at = now_string();
    database.insert_channel_monitor_run(CreateChannelMonitorRunInput {
        monitor_id: monitor.id.clone(),
        template_id: monitor.template_id.clone(),
        station_id: monitor.station_id.clone(),
        station_key_id: monitor.station_key_id.clone(),
        status: "skipped".to_string(),
        started_at: started_at.clone(),
        finished_at: Some(finished_at.clone()),
        duration_ms: duration_between(&started_at, &finished_at),
        http_status: None,
        latency_ms: None,
        response_model: Some(monitor_model(monitor)),
        fallback_model: None,
        error_message: Some("No enabled station keys matched monitor target".to_string()),
    })
}

fn monitor_model(monitor: &ChannelMonitor) -> String {
    monitor
        .fallback_models
        .first()
        .cloned()
        .unwrap_or_else(|| DEFAULT_MONITOR_MODEL.to_string())
}

fn duration_between(started_at: &str, finished_at: &str) -> Option<i64> {
    let started_at = started_at.parse::<i64>().ok()?;
    let finished_at = finished_at.parse::<i64>().ok()?;
    Some((finished_at - started_at).max(0))
}

fn short_error(error: &str) -> String {
    let redacted = redact_monitor_text(error);
    redacted.chars().take(240).collect()
}

fn now_string() -> String {
    now_millis_for_services().to_string()
}

pub struct ChannelMonitorRunnerState {
    stop_requested: Arc<AtomicBool>,
    handle: Mutex<Option<JoinHandle<()>>>,
}

impl ChannelMonitorRunnerState {
    pub fn start(database: AppDatabase, data_key: [u8; 32]) -> Self {
        let stop_requested = Arc::new(AtomicBool::new(false));
        let thread_stop = Arc::clone(&stop_requested);
        let handle = thread::spawn(move || runner_loop(database, data_key, thread_stop));
        Self {
            stop_requested,
            handle: Mutex::new(Some(handle)),
        }
    }

    #[allow(dead_code)]
    pub fn stop(&self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
    }
}

impl Drop for ChannelMonitorRunnerState {
    fn drop(&mut self) {
        self.stop_requested.store(true, Ordering::Relaxed);
        if let Ok(mut handle) = self.handle.lock() {
            if let Some(handle) = handle.take() {
                let _ = handle.join();
            }
        }
    }
}

fn runner_loop(database: AppDatabase, data_key: [u8; 32], stop_requested: Arc<AtomicBool>) {
    while !stop_requested.load(Ordering::Relaxed) {
        let now = now_string();
        match database.due_channel_monitors(&now) {
            Ok(monitors) => {
                for monitor in monitors {
                    if stop_requested.load(Ordering::Relaxed) {
                        break;
                    }
                    let monitor_id = monitor.id.clone();
                    let result = run_monitor_once_guarded(
                        &database,
                        &data_key,
                        monitor,
                        Some(&stop_requested),
                    );
                    if let Err(error) =
                        schedule_after_started_monitor(&database, &monitor_id, result)
                    {
                        eprintln!("Channel monitor runner failed: {error}");
                    }
                }
            }
            Err(error) => eprintln!("Channel monitor runner could not query due monitors: {error}"),
        }
        sleep_until_next_poll(&stop_requested);
    }
}

fn sleep_until_next_poll(stop_requested: &AtomicBool) {
    let mut slept = Duration::ZERO;
    while slept < RUNNER_POLL_INTERVAL && !stop_requested.load(Ordering::Relaxed) {
        thread::sleep(RUNNER_STOP_SLICE);
        slept += RUNNER_STOP_SLICE;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{
            channel_monitors::{CreateChannelMonitorInput, CreateChannelMonitorTemplateInput},
            station_keys::{CreateStationKeyInput, UpdateStationKeyInput},
            stations::CreateStationInput,
        },
        services::database::AppDatabase,
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::{
            atomic::{AtomicUsize, Ordering as AtomicOrdering},
            mpsc, Arc,
        },
        thread,
        time::Duration,
    };

    #[test]
    fn manual_monitor_run_updates_station_key_health() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [7_u8; 32];
        let (base_url, received) = spawn_upstream(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 29\r\n\r\n{\"model\":\"gpt-test\",\"ok\":true}",
        );
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "manual monitor station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url,
                    api_key: "sk-manual-monitor".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let template = database
            .create_channel_monitor_template(CreateChannelMonitorTemplateInput {
                name: "Manual monitor template".to_string(),
                endpoint_kind: "chat_completions".to_string(),
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                request_body_json: r#"{
                    "model": "{{model}}",
                    "max_tokens": "{{max_tokens}}",
                    "stream": "{{stream}}",
                    "messages": [{ "role": "user", "content": "{{challenge}}" }]
                }"#
                .to_string(),
                enabled: true,
                note: None,
            })
            .expect("template");
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Manual key monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id.clone()),
                template_id: template.id,
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["gpt-test".to_string()],
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");
        let stored_runs = database
            .list_channel_monitor_runs(monitor.id)
            .expect("stored runs");
        let health = database
            .get_station_key_health(key.id)
            .expect("station key health");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert_eq!(runs[0].http_status, Some(200));
        assert_eq!(stored_runs.len(), 1);
        assert_eq!(stored_runs[0].status, "success");
        assert_eq!(health.success_count, 1);
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.consecutive_failures, 0);
        assert!(health.avg_latency_ms.is_some());
        assert!(raw_request.contains("Authorization: Bearer sk-manual-monitor"));
        assert!(raw_request.contains(r#""model":"gpt-test""#));
        assert!(raw_request.contains(r#""max_tokens":1"#));
        assert!(raw_request.contains(r#""stream":false"#));
    }

    #[test]
    fn manual_monitor_run_fails_enabled_key_without_api_key() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [9_u8; 32];
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "missing secret station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url: "http://127.0.0.1:9".to_string(),
                    api_key: "sk-to-be-cleared".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        database
            .clear_station_key_secret_for_tests(&key.id)
            .expect("clear station key secret");
        let template = database
            .create_channel_monitor_template(CreateChannelMonitorTemplateInput {
                name: "Missing secret template".to_string(),
                endpoint_kind: "chat_completions".to_string(),
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                request_body_json: r#"{ "model": "{{model}}" }"#.to_string(),
                enabled: true,
                note: None,
            })
            .expect("template");
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Missing secret key monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id.clone()),
                template_id: template.id,
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
        let stored_runs = database
            .list_channel_monitor_runs(monitor.id)
            .expect("stored runs");
        let health = database
            .get_station_key_health(key.id)
            .expect("station key health");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "failed");
        assert_eq!(
            runs[0].station_key_id.as_deref(),
            Some(health.station_key_id.as_str())
        );
        assert!(runs[0].error_message.is_some());
        assert_eq!(stored_runs.len(), 1);
        assert_eq!(stored_runs[0].status, "failed");
        assert_eq!(health.success_count, 0);
        assert_eq!(health.failure_count, 1);
        assert_eq!(health.consecutive_failures, 1);
        assert!(health.last_error_summary.is_some());
    }

    #[test]
    fn monitor_failure_threshold_controls_key_cooldown() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [8_u8; 32];
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "threshold station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url: "http://127.0.0.1:9".to_string(),
                    api_key: "sk-threshold".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let template = database
            .create_channel_monitor_template(CreateChannelMonitorTemplateInput {
                name: "Threshold template".to_string(),
                endpoint_kind: "chat_completions".to_string(),
                method: "POST".to_string(),
                path: "/v1/chat/completions".to_string(),
                request_body_json: r#"{ "model": "{{model}}" }"#.to_string(),
                enabled: true,
                note: None,
            })
            .expect("template");
        let first_key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let high_threshold_key = database
            .create_station_key_with_data_key(
                CreateStationKeyInput {
                    station_id: station.id.clone(),
                    name: "high threshold key".to_string(),
                    api_key: "sk-high-threshold".to_string(),
                    enabled: true,
                    priority: Some(20),
                    group_name: None,
                    tier_label: None,
                    group_binding_id: None,
                    group_id_hash: None,
                    rate_multiplier: None,
                    rate_source: None,
                    balance_scope: None,
                    note: None,
                },
                &data_key,
            )
            .expect("second key");
        database
            .clear_station_key_secret_for_tests(&first_key.id)
            .expect("clear first key secret");
        database
            .clear_station_key_secret_for_tests(&high_threshold_key.id)
            .expect("clear second key secret");
        let immediate_monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Immediate cooldown monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id.clone(),
                station_key_id: Some(first_key.id.clone()),
                template_id: template.id.clone(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 1,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("immediate monitor");
        let tolerant_monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Tolerant cooldown monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(high_threshold_key.id.clone()),
                template_id: template.id,
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 20,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("tolerant monitor");

        run_channel_monitor_now(&database, &data_key, &immediate_monitor.id)
            .expect("first monitor run");
        for _ in 0..3 {
            run_channel_monitor_now(&database, &data_key, &tolerant_monitor.id)
                .expect("tolerant monitor run");
        }

        let immediate_health = database
            .get_station_key_health(first_key.id)
            .expect("immediate health");
        let tolerant_health = database
            .get_station_key_health(high_threshold_key.id)
            .expect("tolerant health");
        assert_eq!(immediate_health.consecutive_failures, 1);
        assert!(
            immediate_health.cooldown_until.is_some(),
            "threshold 1 monitor should cool down after first failure"
        );
        assert_eq!(tolerant_health.consecutive_failures, 3);
        assert_eq!(
            tolerant_health.cooldown_until, None,
            "threshold 20 monitor should not cool down after 3 failures"
        );
    }

    #[test]
    fn built_in_template_uses_monitor_fallback_model() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [5_u8; 32];
        let (base_url, received) = spawn_upstream(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 29\r\n\r\n{\"model\":\"custom-model\",\"ok\":true}",
        );
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "builtin template station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url,
                    api_key: "sk-builtin-template".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Built-in model monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id),
                template_id: "builtin-openai-chat-low-token".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["custom-monitor-model".to_string()],
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert!(
            raw_request.contains(r#""model":"custom-monitor-model""#),
            "built-in template should render monitor fallback model: {raw_request}"
        );
        assert!(
            !raw_request.contains("gpt-4o-mini"),
            "built-in template should not hard-code gpt-4o-mini"
        );
    }

    #[test]
    fn built_in_responses_low_token_template_uses_compact_non_stored_request() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [6_u8; 32];
        let (base_url, received) = spawn_upstream(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 28\r\n\r\n{\"id\":\"resp-test\",\"ok\":true}",
        );
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "responses low token station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url,
                    api_key: "sk-responses-low-token".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Responses low token monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id),
                template_id: "builtin-openai-responses-low-token".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["gpt-test".to_string()],
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
        let raw_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("upstream request");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "success");
        assert!(raw_request.contains(r#""input":"ping""#));
        assert!(raw_request.contains(r#""instructions":"Reply with OK only.""#));
        assert!(raw_request.contains(r#""max_output_tokens":1"#));
        assert!(raw_request.contains(r#""store":false"#));
        assert!(raw_request.contains(r#""stream":false"#));
    }

    #[test]
    fn built_in_responses_monitor_does_not_fall_back_to_chat_when_endpoint_is_unsupported() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [12_u8; 32];
        let (base_url, received) = spawn_path_aware_upstream();
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "responses fallback station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url,
                    api_key: "sk-responses-fallback".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Responses fallback monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id),
                template_id: "builtin-openai-responses-low-token".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["gpt-test".to_string()],
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
        let first_request = received
            .recv_timeout(Duration::from_secs(2))
            .expect("responses request");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "failed");
        assert_eq!(runs[0].http_status, Some(404));
        assert!(first_request.starts_with("POST /v1/responses HTTP/1.1"));
        assert!(
            received.recv_timeout(Duration::from_millis(200)).is_err(),
            "selected Responses monitor must not issue a Chat Completions fallback request"
        );
    }

    #[test]
    fn station_monitor_applies_max_concurrency_to_key_probes() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [6_u8; 32];
        let (base_url, max_active, _received) =
            spawn_counting_upstream(3, Duration::from_millis(250));
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "concurrent station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url,
                    api_key: "sk-concurrent-1".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        for index in 2..=3 {
            database
                .create_station_key_with_data_key(
                    CreateStationKeyInput {
                        station_id: station.id.clone(),
                        name: format!("concurrent key {index}"),
                        api_key: format!("sk-concurrent-{index}"),
                        enabled: true,
                        priority: Some(index),
                        group_name: None,
                        tier_label: None,
                        group_binding_id: None,
                        group_id_hash: None,
                        rate_multiplier: None,
                        rate_source: None,
                        balance_scope: None,
                        note: None,
                    },
                    &data_key,
                )
                .expect("extra key");
        }
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Concurrent station monitor".to_string(),
                target_type: "station".to_string(),
                station_id: station.id,
                station_key_id: None,
                template_id: "builtin-openai-chat-low-token".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 2,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["gpt-concurrency".to_string()],
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");

        assert_eq!(runs.len(), 3);
        assert_eq!(
            max_active.load(AtomicOrdering::SeqCst),
            2,
            "station monitor should run more than one probe but cap at max_concurrency"
        );
    }

    #[test]
    fn overlapping_runs_for_same_monitor_are_rejected() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [10_u8; 32];
        let (base_url, accepted) = spawn_delayed_upstream(Duration::from_millis(400));
        let station = database
            .create_station_with_data_key(
                CreateStationInput {
                    name: "overlap station".to_string(),
                    station_type: "openai-compatible".to_string(),
                    base_url,
                    api_key: "sk-overlap".to_string(),
                    enabled: true,
                    credit_per_cny: 1.0,
                    low_balance_threshold_cny: None,
                    collection_interval_minutes: 5,
                    note: None,
                },
                Some(&data_key),
            )
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "Overlap monitor".to_string(),
                target_type: "station_key".to_string(),
                station_id: station.id,
                station_key_id: Some(key.id),
                template_id: "builtin-openai-chat-low-token".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: vec!["gpt-overlap".to_string()],
                note: None,
            })
            .expect("monitor");
        let first_database = database.clone();
        let first_monitor_id = monitor.id.clone();
        let first_run = thread::spawn(move || {
            run_channel_monitor_now(&first_database, &data_key, &first_monitor_id)
        });
        accepted
            .recv_timeout(Duration::from_secs(2))
            .expect("first request accepted");
        let due_before_overlap = database
            .due_channel_monitors(&now_string())
            .expect("due before overlap");

        let error = run_channel_monitor_now(&database, &[10_u8; 32], &monitor.id)
            .expect_err("overlapping run should be rejected");
        let due_after_overlap = database
            .due_channel_monitors(&now_string())
            .expect("due after overlap");
        let first_result = first_run.join().expect("first run joined");

        assert!(
            error.contains("already running"),
            "overlap error should be explicit: {error}"
        );
        assert!(
            due_before_overlap.iter().any(|item| item.id == monitor.id),
            "monitor should be due before rejected overlap"
        );
        assert!(
            due_after_overlap.iter().any(|item| item.id == monitor.id),
            "rejected overlap must not advance next_run_at"
        );
        assert!(first_result.expect("first run").len() == 1);
    }

    #[test]
    fn started_monitor_error_still_advances_next_schedule() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let monitor = station_monitor(&database, "schedule error");
        let before_due = database
            .due_channel_monitors(&now_string())
            .expect("due monitors before error");

        let result = schedule_after_started_monitor(
            &database,
            &monitor.id,
            Err::<Vec<ChannelMonitorRun>, String>("synthetic run error".to_string()),
        );

        let after_due = database
            .due_channel_monitors(&now_string())
            .expect("due monitors after error");
        assert_eq!(
            result.expect_err("original error preserved"),
            "synthetic run error"
        );
        assert!(before_due.iter().any(|item| item.id == monitor.id));
        assert!(!after_due.iter().any(|item| item.id == monitor.id));
    }

    #[test]
    fn stopped_station_monitor_starts_no_key_runs() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [3_u8; 32];
        let monitor = station_monitor(&database, "stopped monitor");
        let stop_requested = AtomicBool::new(true);

        let runs = run_monitor_once(&database, &data_key, monitor.clone(), Some(&stop_requested))
            .expect("stopped monitor run");
        let stored_runs = database
            .list_channel_monitor_runs(monitor.id)
            .expect("stored runs");

        assert!(runs.is_empty());
        assert!(stored_runs.is_empty());
    }

    #[test]
    fn station_monitor_skips_when_no_enabled_keys_match() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let data_key = [4_u8; 32];
        let station = database
            .create_station(CreateStationInput {
                name: "disabled key station".to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: "https://example.test".to_string(),
                api_key: "sk-disabled-key".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        let key = database
            .list_station_keys(station.id.clone())
            .expect("keys")
            .remove(0);
        database
            .update_station_key(UpdateStationKeyInput {
                id: key.id.clone(),
                station_id: key.station_id.clone(),
                name: key.name.clone(),
                api_key: None,
                enabled: false,
                priority: key.priority,
                group_name: key.group_name.clone(),
                tier_label: key.tier_label.clone(),
                group_binding_id: key.group_binding_id.clone(),
                group_id_hash: key.group_id_hash.clone(),
                rate_multiplier: key.rate_multiplier,
                rate_source: key.rate_source.clone(),
                balance_scope: key.balance_scope.clone(),
                status: key.status.clone(),
                note: key.note.clone(),
            })
            .expect("disable key");
        let monitor = database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: "No enabled keys monitor".to_string(),
                target_type: "station".to_string(),
                station_id: station.id,
                station_key_id: None,
                template_id: "builtin-openai-chat-default".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("monitor");

        let runs = run_channel_monitor_now(&database, &data_key, &monitor.id).expect("manual run");
        let health = database
            .get_station_key_health(key.id)
            .expect("station key health");

        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].status, "skipped");
        assert_eq!(runs[0].station_key_id, None);
        assert_eq!(health.success_count, 0);
        assert_eq!(health.failure_count, 0);
        assert_eq!(health.consecutive_failures, 0);
    }

    fn station_monitor(database: &AppDatabase, name: &str) -> ChannelMonitor {
        let station = database
            .create_station(CreateStationInput {
                name: name.to_string(),
                station_type: "openai-compatible".to_string(),
                base_url: "https://example.test".to_string(),
                api_key: "sk-test-monitor".to_string(),
                enabled: true,
                credit_per_cny: 1.0,
                low_balance_threshold_cny: None,
                collection_interval_minutes: 5,
                note: None,
            })
            .expect("station");
        database
            .create_channel_monitor(CreateChannelMonitorInput {
                name: format!("{name} monitor"),
                target_type: "station".to_string(),
                station_id: station.id,
                station_key_id: None,
                template_id: "builtin-openai-chat-default".to_string(),
                enabled: true,
                interval_seconds: 60,
                jitter_seconds: 0,
                timeout_seconds: 5,
                max_concurrency: 1,
                consecutive_failure_threshold: 3,
                fallback_models: Vec::new(),
                note: None,
            })
            .expect("monitor")
    }

    fn spawn_upstream(response: &'static str) -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let address = listener.local_addr().expect("local addr");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            stream
                .set_read_timeout(Some(Duration::from_secs(2)))
                .expect("read timeout");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let size = stream.read(&mut buffer).expect("read request");
                if size == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..size]);
                if request_is_complete(&request) {
                    break;
                }
            }
            sender
                .send(String::from_utf8_lossy(&request).to_string())
                .expect("send request");
            stream
                .write_all(response.as_bytes())
                .expect("write response");
        });
        (format!("http://{address}"), receiver)
    }

    fn spawn_path_aware_upstream() -> (String, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let address = listener.local_addr().expect("local addr");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            for _ in 0..2 {
                let (mut stream, _) = listener.accept().expect("accept");
                stream
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .expect("read timeout");
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                loop {
                    let size = stream.read(&mut buffer).expect("read request");
                    if size == 0 {
                        break;
                    }
                    request.extend_from_slice(&buffer[..size]);
                    if request_is_complete(&request) {
                        break;
                    }
                }
                let raw_request = String::from_utf8_lossy(&request).to_string();
                let response = if raw_request.starts_with("POST /v1/responses ") {
                    "HTTP/1.1 404 Not Found\r\nContent-Type: application/json\r\nContent-Length: 56\r\n\r\n{\"error\":{\"message\":\"responses endpoint unsupported\"}}"
                } else {
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}"
                };
                sender.send(raw_request).expect("send request");
                stream
                    .write_all(response.as_bytes())
                    .expect("write response");
            }
        });
        (format!("http://{address}"), receiver)
    }

    fn spawn_counting_upstream(
        expected_requests: usize,
        response_delay: Duration,
    ) -> (String, Arc<AtomicUsize>, mpsc::Receiver<String>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        listener
            .set_nonblocking(true)
            .expect("set nonblocking upstream");
        let address = listener.local_addr().expect("local addr");
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let (sender, receiver) = mpsc::channel();
        let thread_active = Arc::clone(&active);
        let thread_max_active = Arc::clone(&max_active);
        thread::spawn(move || {
            let mut accepted = 0;
            let mut idle_rounds = 0;
            while accepted < expected_requests && idle_rounds < 200 {
                match listener.accept() {
                    Ok((stream, _)) => {
                        accepted += 1;
                        idle_rounds = 0;
                        let handler_active = Arc::clone(&thread_active);
                        let handler_max_active = Arc::clone(&thread_max_active);
                        let handler_sender = sender.clone();
                        thread::spawn(move || {
                            handle_counted_connection(
                                stream,
                                response_delay,
                                handler_active,
                                handler_max_active,
                                handler_sender,
                            );
                        });
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        idle_rounds += 1;
                        thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("accept counted upstream: {error}"),
                }
            }
        });
        (format!("http://{address}"), max_active, receiver)
    }

    fn spawn_delayed_upstream(response_delay: Duration) -> (String, mpsc::Receiver<()>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind upstream");
        let address = listener.local_addr().expect("local addr");
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            sender.send(()).expect("send accepted");
            let mut request = Vec::new();
            let mut buffer = [0_u8; 1024];
            loop {
                let size = stream.read(&mut buffer).expect("read request");
                if size == 0 {
                    break;
                }
                request.extend_from_slice(&buffer[..size]);
                if request_is_complete(&request) {
                    break;
                }
            }
            thread::sleep(response_delay);
            stream
                .write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
                )
                .expect("write response");
        });
        (format!("http://{address}"), receiver)
    }

    fn handle_counted_connection(
        mut stream: std::net::TcpStream,
        response_delay: Duration,
        active: Arc<AtomicUsize>,
        max_active: Arc<AtomicUsize>,
        sender: mpsc::Sender<String>,
    ) {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .expect("read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        loop {
            let size = stream.read(&mut buffer).expect("read request");
            if size == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..size]);
            if request_is_complete(&request) {
                break;
            }
        }
        let active_now = active.fetch_add(1, AtomicOrdering::SeqCst) + 1;
        max_active.fetch_max(active_now, AtomicOrdering::SeqCst);
        thread::sleep(response_delay);
        active.fetch_sub(1, AtomicOrdering::SeqCst);
        sender
            .send(String::from_utf8_lossy(&request).to_string())
            .expect("send request");
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 11\r\n\r\n{\"ok\":true}",
            )
            .expect("write response");
    }

    fn request_is_complete(request: &[u8]) -> bool {
        let Some(header_end) = request.windows(4).position(|item| item == b"\r\n\r\n") else {
            return false;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]);
        let content_length = headers
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
        request.len() >= header_end + 4 + content_length
    }
}
