pub mod probe;
pub mod redaction;
pub mod templates;

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use crate::{
    models::{
        channel_monitors::{ChannelMonitor, ChannelMonitorRun, CreateChannelMonitorRunInput},
        station_keys::KeyPoolItem,
    },
    services::{
        channel_monitors::{
            probe::{run_monitor_probe, MonitorProbeResult},
            redaction::redact_monitor_text,
            templates::{render_monitor_request, MonitorTemplateContext},
        },
        database::{now_millis_for_services, AppDatabase},
    },
};

const RUNNER_POLL_INTERVAL: Duration = Duration::from_secs(30);
const RUNNER_STOP_SLICE: Duration = Duration::from_millis(250);
const DEFAULT_MONITOR_MODEL: &str = "gpt-4o-mini";
const DEFAULT_MONITOR_CHALLENGE: &str = "relay-pool-monitor-ping";

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
        database.schedule_next_channel_monitor_run(&monitor.id)?;
        return Ok(vec![run]);
    }

    let mut runs = Vec::with_capacity(targets.len());
    for target in targets {
        let run = run_monitor_for_key(database, data_key, &monitor, &template, &target)?;
        database.update_channel_monitor_after_run(
            &monitor.id,
            &run.status,
            run.finished_at.as_deref().unwrap_or(&run.started_at),
            run.error_message.as_deref(),
        )?;
        runs.push(run);
    }
    database.schedule_next_channel_monitor_run(&monitor.id)?;
    Ok(runs)
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
    template: &crate::models::channel_monitors::ChannelMonitorRequestTemplate,
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
        database.record_station_key_failure(
            station_key_id,
            &short_error(
                result
                    .error_summary
                    .as_deref()
                    .unwrap_or("Channel monitor probe failed"),
            ),
            &finished_at,
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
    database.record_station_key_failure(station_key_id, &error_message, &finished_at)?;
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
                    if let Err(error) = run_monitor(&database, &data_key, monitor) {
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
            stations::CreateStationInput,
        },
        services::database::AppDatabase,
    };
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
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
