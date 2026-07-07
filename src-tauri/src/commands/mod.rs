use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::process::Command;
use std::time::{Duration, Instant};
use tauri::{Manager, State};

use crate::{
    models::{
        capture::{CaptureSessionStatus, CapturedHttpEventInput},
        change_events::{ChangeEvent, UpsertChangeEventInput},
        channel_monitors::{
            ChannelMonitor, ChannelMonitorRequestTemplate, ChannelMonitorRun,
            CreateChannelMonitorInput, CreateChannelMonitorTemplateInput,
            UpdateChannelMonitorInput, UpdateChannelMonitorTemplateInput,
        },
        collector::{
            CollectorRunResult, CollectorSnapshot, StationLoginTestInput, StationLoginTestResult,
        },
        collector_runs::CollectorRun,
        credentials::{
            StationCredentials, UpdateStationCredentialsInput, UpdateStationSessionInput,
        },
        group_facts::{
            GroupRateRecord, StationGroupBinding, UpdateStationKeyGroupBindingInput,
            UpsertStationGroupBindingInput,
        },
        pricing::{
            BalanceSnapshot, PricingRule, UpsertBalanceSnapshotInput, UpsertPricingRuleInput,
        },
        proxy::{ProxyStatus, RequestLog, UpstreamApiFormat},
        remote_keys::{
            BindRemoteStationKeyInput, CreateLocalStationKeyFromRemoteResult,
            CreateRemoteStationKeyInput, CreateRemoteStationKeyResult, RemoteKeyCapability,
            RemoteKeyScanResult, RemoteStationKey,
        },
        routing::{
            ModelAlias, RouteSimulationInput, RouteSimulationResult, StationKeyCapabilities,
            StationKeyHealth, UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput,
        },
        secrets::{SecretMigrationReport, SecretScanFinding},
        settings::{AppSettings, UpdateSettingsInput},
        shared_capabilities::{
            ChannelMonitorSummary, SaveStationKeyWithDefaultsInput,
            SaveStationKeyWithDefaultsResult, StationGroupOption,
        },
        station_keys::KeyPoolItem,
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
        stations::{
            CreateStationInput, EndpointPingResult, Station, StationEndpointHealth,
            UpdateStationInput,
        },
        AppStatus,
    },
    services::{
        capture, collectors,
        database::{now_millis_for_services, AppDatabase},
        endpoint_ping::ping_station_endpoint as probe_station_endpoint,
        proxy::{
            build_upstream_url, redact_error_message, runtime::ProxyRuntimeState, should_fallback,
        },
        remote_keys,
        secrets::SecretManager,
    },
};

#[tauri::command]
pub fn app_status() -> AppStatus {
    AppStatus::default()
}

#[tauri::command]
pub fn list_stations(database: State<'_, AppDatabase>) -> Result<Vec<Station>, String> {
    database.list_stations()
}

#[tauri::command]
pub fn create_station(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: CreateStationInput,
) -> Result<Station, String> {
    database.create_station_with_data_key(input, Some(secrets.data_key()))
}

#[tauri::command]
pub fn update_station(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: UpdateStationInput,
) -> Result<Station, String> {
    database.update_station_with_data_key(input, Some(secrets.data_key()))
}

#[tauri::command]
pub fn delete_station(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_station(id)
}

#[tauri::command]
pub fn reorder_stations(
    database: State<'_, AppDatabase>,
    station_ids: Vec<String>,
) -> Result<Vec<Station>, String> {
    database.reorder_stations(station_ids)
}

#[tauri::command]
pub fn get_settings(database: State<'_, AppDatabase>) -> Result<AppSettings, String> {
    database.get_settings()
}

#[tauri::command]
pub fn get_local_access_key(database: State<'_, AppDatabase>) -> Result<String, String> {
    database.get_local_access_key()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcswitchImportResult {
    app: String,
    provider_name: String,
    endpoint: String,
}

#[tauri::command]
pub fn import_relay_pool_to_ccswitch(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<CcswitchImportResult, String> {
    let settings = database.get_settings()?;
    let local_access_key = database.get_local_access_key()?;
    if local_access_key.trim().is_empty() {
        return Err("本地访问密钥为空，无法导入 CCSwitch。".to_string());
    }

    database.migrate_plaintext_secrets(secrets.data_key())?;
    let proxy_status = proxy.start(
        database.inner().clone(),
        *secrets.data_key(),
        settings.local_proxy_port,
    )?;
    let endpoint = format!("http://{}:{}/v1", proxy_status.bind_addr, proxy_status.port);
    let homepage = format!("http://{}:{}", proxy_status.bind_addr, proxy_status.port);
    let provider_name = "Relay Pool Desktop".to_string();
    let deeplink = build_ccswitch_provider_deeplink(
        "codex",
        &provider_name,
        &homepage,
        &endpoint,
        &local_access_key,
    );

    open_url_with_system(&deeplink)?;

    Ok(CcswitchImportResult {
        app: "codex".to_string(),
        provider_name,
        endpoint,
    })
}

#[tauri::command]
pub fn update_settings(
    database: State<'_, AppDatabase>,
    input: UpdateSettingsInput,
) -> Result<AppSettings, String> {
    database.update_settings(input)
}

#[tauri::command]
pub fn get_proxy_status(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    Ok(proxy.status(settings.local_proxy_port))
}

#[tauri::command]
pub fn start_local_proxy(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(secrets.data_key())?;
    proxy.start(
        database.inner().clone(),
        *secrets.data_key(),
        settings.local_proxy_port,
    )
}

#[tauri::command]
pub fn stop_local_proxy(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    proxy.stop(settings.local_proxy_port)
}

#[tauri::command]
pub fn restart_local_proxy(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(secrets.data_key())?;
    proxy.restart(
        database.inner().clone(),
        *secrets.data_key(),
        settings.local_proxy_port,
    )
}

#[tauri::command]
pub fn list_request_logs(database: State<'_, AppDatabase>) -> Result<Vec<RequestLog>, String> {
    database.list_request_logs()
}

#[tauri::command]
pub fn clear_request_logs(database: State<'_, AppDatabase>) -> Result<(), String> {
    database.clear_request_logs()
}

#[tauri::command]
pub fn get_secret_migration_status(
    database: State<'_, AppDatabase>,
) -> Result<SecretMigrationReport, String> {
    database.secret_migration_status()
}

#[tauri::command]
pub fn run_secret_safety_scan(
    database: State<'_, AppDatabase>,
) -> Result<Vec<SecretScanFinding>, String> {
    database.run_secret_safety_scan()
}

#[tauri::command]
pub fn list_station_keys(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<StationKey>, String> {
    database.list_station_keys(station_id)
}

#[tauri::command]
pub fn create_station_key(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: CreateStationKeyInput,
) -> Result<StationKey, String> {
    database.create_station_key_with_data_key(input, secrets.data_key())
}

#[tauri::command]
pub fn update_station_key(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: UpdateStationKeyInput,
) -> Result<StationKey, String> {
    database.update_station_key_with_data_key(input, secrets.data_key())
}

#[tauri::command]
pub fn save_station_key_with_defaults(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: SaveStationKeyWithDefaultsInput,
) -> Result<SaveStationKeyWithDefaultsResult, String> {
    database.save_station_key_with_defaults(secrets.data_key(), input)
}

#[tauri::command]
pub fn update_station_key_group_binding(
    database: State<'_, AppDatabase>,
    input: UpdateStationKeyGroupBindingInput,
) -> Result<StationKey, String> {
    database.update_station_key_group_binding(input)
}

#[tauri::command]
pub fn delete_station_key(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_station_key(id)
}

#[tauri::command]
pub fn reorder_station_keys(
    database: State<'_, AppDatabase>,
    station_id: String,
    key_ids: Vec<String>,
) -> Result<Vec<StationKey>, String> {
    database.reorder_station_keys(station_id, key_ids)
}

#[tauri::command]
pub fn get_remote_key_capability(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<RemoteKeyCapability, String> {
    remote_keys::remote_key_capability(&database, station_id)
}

#[tauri::command]
pub fn list_remote_station_keys(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<RemoteStationKey>, String> {
    remote_keys::list_remote_keys(&database, station_id)
}

#[tauri::command]
pub async fn scan_remote_station_keys(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
) -> Result<RemoteKeyScanResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        remote_keys::scan_remote_keys(&database, &data_key, station_id)
    })
    .await
    .map_err(|error| format!("远端 Key 扫描任务执行失败: {error}"))?
}

#[tauri::command]
pub async fn create_remote_station_key(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: CreateRemoteStationKeyInput,
) -> Result<CreateRemoteStationKeyResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        remote_keys::create_remote_key(&database, &data_key, input)
    })
    .await
    .map_err(|error| format!("远端 Key 创建任务执行失败: {error}"))?
}

#[tauri::command]
pub async fn create_local_station_key_from_remote(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    remote_key_id: String,
    station_id: String,
) -> Result<CreateLocalStationKeyFromRemoteResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        remote_keys::create_local_key_from_remote_key(
            &database,
            &data_key,
            station_id,
            remote_key_id,
        )
    })
    .await
    .map_err(|error| format!("远端 Key 同步本地任务执行失败: {error}"))?
}

#[tauri::command]
pub fn bind_remote_station_key(
    database: State<'_, AppDatabase>,
    input: BindRemoteStationKeyInput,
) -> Result<Vec<RemoteStationKey>, String> {
    remote_keys::bind_remote_key(&database, input)
}

#[tauri::command]
pub fn unbind_remote_station_key(
    database: State<'_, AppDatabase>,
    remote_key_id: String,
    station_id: String,
) -> Result<Vec<RemoteStationKey>, String> {
    database.unbind_remote_station_key(remote_key_id, station_id)
}

#[tauri::command]
pub fn list_key_pool_items(database: State<'_, AppDatabase>) -> Result<Vec<KeyPoolItem>, String> {
    database.list_key_pool_items()
}

#[tauri::command]
pub fn reorder_key_pool(
    database: State<'_, AppDatabase>,
    key_ids: Vec<String>,
) -> Result<Vec<KeyPoolItem>, String> {
    database.reorder_key_pool(key_ids)
}

#[tauri::command]
pub fn get_station_key_capabilities(
    database: State<'_, AppDatabase>,
    station_key_id: String,
) -> Result<StationKeyCapabilities, String> {
    database.get_station_key_capabilities(station_key_id)
}

#[tauri::command]
pub fn update_station_key_capabilities(
    database: State<'_, AppDatabase>,
    input: UpdateStationKeyCapabilitiesInput,
) -> Result<StationKeyCapabilities, String> {
    database.update_station_key_capabilities(input)
}

#[tauri::command]
pub fn list_model_aliases(database: State<'_, AppDatabase>) -> Result<Vec<ModelAlias>, String> {
    database.list_model_aliases()
}

#[tauri::command]
pub fn upsert_model_alias(
    database: State<'_, AppDatabase>,
    input: UpsertModelAliasInput,
) -> Result<ModelAlias, String> {
    database.upsert_model_alias(input)
}

#[tauri::command]
pub fn delete_model_alias(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_model_alias(id)
}

#[tauri::command]
pub fn list_station_key_health(
    database: State<'_, AppDatabase>,
) -> Result<Vec<StationKeyHealth>, String> {
    database.list_station_key_health()
}

#[tauri::command]
pub fn list_station_endpoint_health(
    database: State<'_, AppDatabase>,
) -> Result<Vec<StationEndpointHealth>, String> {
    database.list_station_endpoint_health()
}

#[tauri::command]
pub fn list_channel_monitors(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelMonitor>, String> {
    database.list_channel_monitors()
}

#[tauri::command]
pub fn list_channel_monitor_summaries(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelMonitorSummary>, String> {
    database.list_channel_monitor_summaries()
}

#[tauri::command]
pub fn create_channel_monitor(
    database: State<'_, AppDatabase>,
    input: CreateChannelMonitorInput,
) -> Result<ChannelMonitor, String> {
    database.create_channel_monitor(input)
}

#[tauri::command]
pub fn update_channel_monitor(
    database: State<'_, AppDatabase>,
    input: UpdateChannelMonitorInput,
) -> Result<ChannelMonitor, String> {
    database.update_channel_monitor(input)
}

#[tauri::command]
pub fn delete_channel_monitor(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_channel_monitor(id)
}

#[tauri::command]
pub fn list_channel_monitor_runs(
    database: State<'_, AppDatabase>,
    monitor_id: String,
) -> Result<Vec<ChannelMonitorRun>, String> {
    database.list_channel_monitor_runs(monitor_id)
}

#[tauri::command]
pub fn list_channel_monitor_templates(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelMonitorRequestTemplate>, String> {
    database.list_channel_monitor_templates()
}

#[tauri::command]
pub fn create_channel_monitor_template(
    database: State<'_, AppDatabase>,
    input: CreateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, String> {
    database.create_channel_monitor_template(input)
}

#[tauri::command]
pub fn update_channel_monitor_template(
    database: State<'_, AppDatabase>,
    input: UpdateChannelMonitorTemplateInput,
) -> Result<ChannelMonitorRequestTemplate, String> {
    database.update_channel_monitor_template(input)
}

#[tauri::command]
pub fn duplicate_channel_monitor_template(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChannelMonitorRequestTemplate, String> {
    database.duplicate_channel_monitor_template(id)
}

#[tauri::command]
pub fn delete_channel_monitor_template(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<(), String> {
    database.delete_channel_monitor_template(id)
}

#[tauri::command]
pub async fn run_channel_monitor_now(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    monitor_id: String,
) -> Result<Vec<ChannelMonitorRun>, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::channel_monitors::run_channel_monitor_now(
            &database,
            &data_key,
            &monitor_id,
        )
    })
    .await
    .map_err(|error| format!("Channel monitor run failed to join: {error}"))?
}

#[tauri::command]
pub fn get_station_key_health(
    database: State<'_, AppDatabase>,
    station_key_id: String,
) -> Result<StationKeyHealth, String> {
    database.get_station_key_health(station_key_id)
}

#[tauri::command]
pub fn ping_station_endpoint(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<EndpointPingResult, String> {
    ping_station_endpoint_for_tests(&database, station_id, 5)
}

fn ping_station_endpoint_for_tests(
    database: &AppDatabase,
    station_id: String,
    timeout_seconds: u64,
) -> Result<EndpointPingResult, String> {
    let station = database
        .list_stations()?
        .into_iter()
        .find(|station| station.id == station_id)
        .ok_or_else(|| "未找到要 PING 的中转站".to_string())?;
    let checked_at = now_millis_for_services().to_string();
    let probe = probe_station_endpoint(
        &station.base_url,
        Duration::from_secs(timeout_seconds.max(1)),
    );
    let health = database.upsert_station_endpoint_health(
        &station.id,
        &probe.status,
        probe.latency_ms,
        &checked_at,
        probe.error_summary.as_deref(),
    )?;
    Ok(EndpointPingResult {
        station_id: health.station_id,
        ok: probe.ok,
        status: health.status,
        latency_ms: health.latency_ms,
        checked_at: health.checked_at.unwrap_or(checked_at),
        error_summary: health.error_summary,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyConnectivityTestResult {
    station_key_id: String,
    ok: bool,
    status_code: u16,
    duration_ms: i64,
    model: String,
    message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StationKeyConnectivityProbeKind {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone)]
struct StationKeyConnectivityProbeResult {
    ok: bool,
    status_code: u16,
    duration_ms: i64,
    message: String,
}

impl StationKeyConnectivityProbeResult {
    fn success(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: true,
            status_code,
            duration_ms,
            message,
        }
    }

    fn failure(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: false,
            status_code,
            duration_ms,
            message,
        }
    }
}

#[tauri::command]
pub async fn test_station_key_connectivity(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_key_id: String,
) -> Result<StationKeyConnectivityTestResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        test_station_key_connectivity_blocking(&database, &data_key, &station_key_id)
    })
    .await
    .map_err(|error| format!("测试密钥连通性任务失败: {error}"))?
}

#[tauri::command]
pub fn simulate_route(
    database: State<'_, AppDatabase>,
    input: RouteSimulationInput,
) -> Result<RouteSimulationResult, String> {
    database.simulate_route(input)
}

#[tauri::command]
pub fn list_pricing_rules(database: State<'_, AppDatabase>) -> Result<Vec<PricingRule>, String> {
    database.list_pricing_rules()
}

#[tauri::command]
pub fn upsert_pricing_rule(
    database: State<'_, AppDatabase>,
    input: UpsertPricingRuleInput,
) -> Result<PricingRule, String> {
    database.upsert_pricing_rule(input)
}

#[tauri::command]
pub fn delete_pricing_rule(database: State<'_, AppDatabase>, id: String) -> Result<(), String> {
    database.delete_pricing_rule(id)
}

#[tauri::command]
pub fn list_balance_snapshots(
    database: State<'_, AppDatabase>,
) -> Result<Vec<BalanceSnapshot>, String> {
    database.list_balance_snapshots()
}

#[tauri::command]
pub fn list_balance_snapshots_for_station(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<BalanceSnapshot>, String> {
    database.list_balance_snapshots_for_station(station_id)
}

#[tauri::command]
pub fn upsert_balance_snapshot(
    database: State<'_, AppDatabase>,
    input: UpsertBalanceSnapshotInput,
) -> Result<BalanceSnapshot, String> {
    database.upsert_balance_snapshot(input)
}

#[tauri::command]
pub fn list_station_group_bindings(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<StationGroupBinding>, String> {
    database.list_station_group_bindings(station_id)
}

#[tauri::command]
pub fn list_station_group_options(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<StationGroupOption>, String> {
    database.list_station_group_options(station_id)
}

#[tauri::command]
pub fn upsert_station_group_binding(
    database: State<'_, AppDatabase>,
    input: UpsertStationGroupBindingInput,
) -> Result<StationGroupBinding, String> {
    database.upsert_station_group_binding(input)
}

#[tauri::command]
pub fn list_group_rate_records(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<GroupRateRecord>, String> {
    database.list_group_rate_records(station_id)
}

#[tauri::command]
pub fn list_collector_runs(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<CollectorRun>, String> {
    database.list_collector_runs(station_id)
}

#[tauri::command]
pub fn list_change_events(database: State<'_, AppDatabase>) -> Result<Vec<ChangeEvent>, String> {
    database.list_change_events()
}

#[tauri::command]
pub fn clear_change_events(database: State<'_, AppDatabase>) -> Result<(), String> {
    database.clear_change_events()
}

#[tauri::command]
pub fn list_change_events_for_station(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<ChangeEvent>, String> {
    database.list_change_events_for_station(station_id)
}

#[tauri::command]
pub fn upsert_change_event(
    database: State<'_, AppDatabase>,
    input: UpsertChangeEventInput,
) -> Result<ChangeEvent, String> {
    database.upsert_change_event(input)
}

#[tauri::command]
pub fn mark_change_event_read(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChangeEvent, String> {
    database.mark_change_event_read(id)
}

#[tauri::command]
pub fn dismiss_change_event(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChangeEvent, String> {
    database.dismiss_change_event(id)
}

#[tauri::command]
pub fn resolve_change_event(
    database: State<'_, AppDatabase>,
    id: String,
) -> Result<ChangeEvent, String> {
    database.resolve_change_event(id)
}

#[tauri::command]
pub fn get_station_credentials(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<StationCredentials, String> {
    database.get_station_credentials(station_id)
}

#[tauri::command]
pub fn update_station_credentials(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: UpdateStationCredentialsInput,
) -> Result<StationCredentials, String> {
    database.update_station_credentials_with_data_key(input, secrets.data_key())
}

#[tauri::command]
pub fn update_station_session(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    input: UpdateStationSessionInput,
) -> Result<StationCredentials, String> {
    database.update_station_session_with_data_key(input, secrets.data_key())
}

#[tauri::command]
pub fn clear_station_credentials(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<StationCredentials, String> {
    database.clear_station_credentials(station_id)
}

#[tauri::command]
pub async fn detect_sub2api_station(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    detect_station_info(database, station_id).await
}

#[tauri::command]
pub async fn collect_sub2api_station(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    collect_station_info(database, secrets, station_id).await
}

#[tauri::command]
pub async fn detect_station_info(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        collectors::detect_station_info(&database, station_id)
    })
    .await
    .map_err(|error| format!("采集任务执行失败: {error}"))?
}

#[tauri::command]
pub async fn collect_station_info(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        collectors::collect_station_info(&database, &data_key, station_id)
    })
    .await
    .map_err(|error| format!("采集任务执行失败: {error}"))?
}

#[tauri::command]
pub async fn collect_station_task(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
    task_type: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        let task = match task_type.as_str() {
            "detect" => collectors::adapters::CollectorTask::Detect,
            "balance" => collectors::adapters::CollectorTask::Balance,
            "groups" => collectors::adapters::CollectorTask::Groups,
            "models" => collectors::adapters::CollectorTask::Models,
            "full" => collectors::adapters::CollectorTask::Full,
            _ => return Err("未知采集任务类型".to_string()),
        };
        collectors::collect_station_task(&database, &data_key, station_id, task)
    })
    .await
    .map_err(|error| format!("采集任务执行失败: {error}"))?
}

#[tauri::command]
pub async fn test_station_login(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        collectors::test_station_login(&database, &data_key, station_id)
    })
    .await
    .map_err(|error| format!("登录测试执行失败: {error}"))?
}

#[tauri::command]
pub async fn test_station_login_input(
    input: StationLoginTestInput,
) -> Result<StationLoginTestResult, String> {
    tauri::async_runtime::spawn_blocking(move || collectors::test_station_login_input(input))
        .await
        .map_err(|error| format!("连通性测试执行失败: {error}"))?
}

#[tauri::command]
pub fn list_collector_snapshots(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<CollectorSnapshot>, String> {
    database.list_collector_snapshots(station_id)
}

#[tauri::command]
pub fn get_latest_collector_snapshot(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Option<CollectorSnapshot>, String> {
    database.get_latest_collector_snapshot(station_id)
}

#[tauri::command]
pub async fn start_capture_session(
    app: tauri::AppHandle,
    database: State<'_, AppDatabase>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CaptureSessionStatus, String> {
    let station = database.station_for_collector(&station_id)?;
    let label = capture_window_label(&station_id);
    let script = capture_script(&station_id, &label);
    let app_handle = app.clone();
    let label_for_start = label.clone();
    tauri::async_runtime::spawn_blocking(move || {
        if let Some(window) = app_handle.get_webview_window(&label_for_start) {
            window
                .set_focus()
                .map_err(|error| format!("聚焦捕获窗口失败: {error}"))?;
        } else {
            tauri::WebviewWindowBuilder::new(
                &app_handle,
                label_for_start.clone(),
                tauri::WebviewUrl::External(
                    "about:blank"
                        .parse()
                        .map_err(|error| format!("捕获窗口初始化失败: {error}"))?,
                ),
            )
            .title(format!("网页登录 / 捕获 - {}", station.name))
            .inner_size(1100.0, 760.0)
            .initialization_script(&script)
            .build()
            .map_err(|error| format!("打开网页登录窗口失败: {error}"))?;
            if let Some(window) = app_handle.get_webview_window(&label_for_start) {
                let target = station
                    .base_url
                    .parse()
                    .map_err(|error| format!("Base URL 无法作为网页登录地址打开: {error}"))?;
                let navigator = window.clone();
                window
                    .run_on_main_thread(move || {
                        let _ = navigator.navigate(target);
                    })
                    .map_err(|error| format!("安排捕获窗口导航失败: {error}"))?;
            }
        }
        Ok::<(), String>(())
    })
    .await
    .map_err(|error| format!("打开网页登录窗口失败: {error}"))??;
    sessions.start(station_id, label)
}

#[tauri::command]
pub fn get_capture_session_status(
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CaptureSessionStatus, String> {
    sessions.status(&station_id)
}

#[tauri::command]
pub fn record_capture_event(
    database: State<'_, AppDatabase>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: CapturedHttpEventInput,
) -> Result<CaptureSessionStatus, String> {
    let station = database.station_for_collector(&input.station_id)?;
    if !input
        .request_url
        .starts_with(station.base_url.trim_end_matches('/'))
    {
        return Err("捕获事件不属于当前站点 Base URL，已拒绝。".to_string());
    }
    let station_id = input.station_id.clone();
    let event = capture::sanitize_event(input);
    sessions.push_event(&station_id, event)
}

#[tauri::command]
pub fn clear_capture_session(
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CaptureSessionStatus, String> {
    sessions.clear(&station_id)
}

#[tauri::command]
pub fn close_capture_session(
    app: tauri::AppHandle,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CaptureSessionStatus, String> {
    let label = capture_window_label(&station_id);
    if let Some(window) = app.get_webview_window(&label) {
        window
            .close()
            .map_err(|error| format!("关闭网页登录窗口失败: {error}"))?;
    }
    sessions.clear(&station_id)
}

#[tauri::command]
pub fn finish_capture_session(
    database: State<'_, AppDatabase>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let events = sessions.take_events(&station_id)?;
    let (summary, normalized, raw) = capture::summarize_events(&events);
    let status = normalized
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("partial")
        .to_string();
    let error_message = if events.is_empty() {
        Some("未捕获到后台接口响应，请确认已在网页登录窗口完成登录并打开后台页面。".to_string())
    } else {
        None
    };
    let snapshot = database.insert_collector_snapshot(
        &station_id,
        "webview-capture",
        &status,
        summary,
        normalized,
        Some(raw),
        error_message,
    )?;
    Ok(CollectorRunResult {
        snapshot,
        events: Vec::new(),
    })
}

fn capture_window_label(station_id: &str) -> String {
    format!(
        "capture-{}",
        station_id.replace(|character: char| !character.is_ascii_alphanumeric(), "-")
    )
}

fn capture_script(station_id: &str, window_label: &str) -> String {
    format!(
        r#"
(() => {{
  if (window.__relayPoolCaptureInstalled) return;
  window.__relayPoolCaptureInstalled = true;
  const stationId = {station_id:?};
  const sourceWindowId = {window_label:?};
  const limit = 4000;
  const invoke = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    ? window.__TAURI_INTERNALS__.invoke
    : null;
  const pathFromUrl = (url) => {{
    try {{ return new URL(url, window.location.href).pathname || "/"; }}
    catch (_) {{ return "/"; }}
  }};
  const contentTypeOf = (headers) => {{
    try {{ return headers && headers.get ? (headers.get("content-type") || "") : ""; }}
    catch (_) {{ return ""; }}
  }};
  const send = (input) => {{
    if (!invoke) return;
    invoke("record_capture_event", {{ input }}).catch(() => undefined);
  }};
  const buildBase = (url, method, startedAt) => ({{
    stationId,
    sourceWindowId,
    pageUrl: window.location.href,
    requestUrl: String(new URL(url, window.location.href)),
    requestPath: pathFromUrl(url),
    method,
    startedAt,
  }});
  const originalFetch = window.fetch;
  window.fetch = async function(input, init) {{
    const url = typeof input === "string" ? input : (input && input.url) || String(input);
    const method = (init && init.method) || (input && input.method) || "GET";
    const startedAt = new Date().toISOString();
    const started = performance.now();
    try {{
      const response = await originalFetch.apply(this, arguments);
      const clone = response.clone();
      const contentType = contentTypeOf(response.headers);
      const base = buildBase(url, method, startedAt);
      if (contentType.includes("json")) {{
        clone.json().then((json) => send({{
          ...base,
          status: response.status,
          contentType,
          finishedAt: new Date().toISOString(),
          durationMs: Math.round(performance.now() - started),
          responseKind: "json",
          responseJson: json,
          responseSize: JSON.stringify(json).length,
        }})).catch(() => undefined);
      }} else {{
        clone.text().then((text) => send({{
          ...base,
          status: response.status,
          contentType,
          finishedAt: new Date().toISOString(),
          durationMs: Math.round(performance.now() - started),
          responseKind: contentType.includes("html") ? "html" : "text",
          responseText: text.slice(0, limit),
          responseSize: text.length,
        }})).catch(() => undefined);
      }}
      return response;
    }} catch (error) {{
      send({{
        ...buildBase(url, method, startedAt),
        finishedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - started),
        responseKind: "error",
        errorMessage: error && error.message ? error.message : String(error),
      }});
      throw error;
    }}
  }};
  const originalOpen = XMLHttpRequest.prototype.open;
  const originalSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url) {{
    this.__relayPoolCapture = {{ method: method || "GET", url: String(url), startedAt: new Date().toISOString(), started: performance.now() }};
    return originalOpen.apply(this, arguments);
  }};
  XMLHttpRequest.prototype.send = function() {{
    this.addEventListener("loadend", function() {{
      const meta = this.__relayPoolCapture;
      if (!meta) return;
      const contentType = this.getResponseHeader("content-type") || "";
      let responseText = "";
      try {{ responseText = typeof this.responseText === "string" ? this.responseText : ""; }} catch (_) {{}}
      let responseJson = null;
      if (contentType.includes("json") && responseText) {{
        try {{ responseJson = JSON.parse(responseText); }} catch (_) {{}}
      }}
      send({{
        ...buildBase(meta.url, meta.method, meta.startedAt),
        status: this.status,
        contentType,
        finishedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - meta.started),
        responseKind: responseJson ? "json" : (contentType.includes("html") ? "html" : "text"),
        responseJson,
        responseText: responseJson ? null : responseText.slice(0, limit),
        responseSize: responseText.length,
      }});
    }});
    return originalSend.apply(this, arguments);
  }};
}})();
"#
    )
}

fn build_ccswitch_provider_deeplink(
    app: &str,
    provider_name: &str,
    homepage: &str,
    endpoint: &str,
    api_key: &str,
) -> String {
    let usage_script = general_purpose::STANDARD.encode(build_ccswitch_usage_script());
    let mut entries = vec![
        ("resource", "provider".to_string()),
        ("app", app.to_string()),
        ("name", provider_name.to_string()),
        ("homepage", homepage.to_string()),
        ("endpoint", endpoint.to_string()),
        ("apiKey", api_key.to_string()),
        ("configFormat", "json".to_string()),
        ("usageEnabled", "true".to_string()),
        ("usageScript", usage_script),
        ("usageAutoInterval", "30".to_string()),
        ("enabled", "true".to_string()),
    ];
    if app == "codex" {
        entries.insert(2, ("model", "gpt-5.4".to_string()));
    }

    let query = entries
        .into_iter()
        .map(|(key, value)| format!("{}={}", encode_query_param(key), encode_query_param(&value)))
        .collect::<Vec<_>>()
        .join("&");

    format!("ccswitch://v1/import?{query}")
}

fn build_ccswitch_usage_script() -> &'static str {
    r#"({
    request: {
      url: "{{baseUrl}}/usage",
      method: "GET",
      headers: { "Authorization": "Bearer {{apiKey}}" }
    },
    extractor: function(response) {
      const remaining = response?.remaining ?? response?.quota?.remaining ?? response?.balance;
      const unit = response?.unit ?? response?.quota?.unit ?? "USD";
      return {
        isValid: response?.is_active ?? response?.isValid ?? true,
        remaining,
        unit
      };
    }
  })"#
}

fn encode_query_param(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

struct SystemUrlLauncher {
    program: &'static str,
    args: Vec<String>,
}

fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    #[cfg(target_os = "windows")]
    {
        return SystemUrlLauncher {
            program: "rundll32.exe",
            args: vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()],
        };
    }

    #[cfg(target_os = "macos")]
    {
        return SystemUrlLauncher {
            program: "open",
            args: vec![url.to_string()],
        };
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        return SystemUrlLauncher {
            program: "xdg-open",
            args: vec![url.to_string()],
        };
    }
}

fn open_url_with_system(url: &str) -> Result<(), String> {
    let launcher = system_url_launcher(url);
    let result = Command::new(launcher.program).args(launcher.args).spawn();

    result
        .map(|_| ())
        .map_err(|error| format!("无法打开 CCSwitch 导入链接: {error}"))
}

fn test_station_key_connectivity_blocking(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_key_id: &str,
) -> Result<StationKeyConnectivityTestResult, String> {
    let key = database
        .list_key_pool_items()?
        .into_iter()
        .find(|item| item.id == station_key_id)
        .ok_or_else(|| "Station Key 不存在，无法测试连通性".to_string())?;
    if !key.api_key_present {
        return Err("该密钥没有保存 API Key，无法测试连通性。".to_string());
    }

    let api_key = database.resolve_station_key_secret_with_data_key(data_key, station_key_id)?;
    let capabilities = database
        .get_station_key_capabilities(station_key_id.to_string())
        .ok();
    let upstream_api_format = database
        .proxy_route_candidates_with_data_key(data_key)
        .ok()
        .and_then(|candidates| {
            candidates
                .into_iter()
                .find(|candidate| candidate.station_key_id == station_key_id)
                .map(|candidate| candidate.upstream_api_format)
        })
        .unwrap_or(UpstreamApiFormat::Auto);
    let discovered_models =
        discover_station_key_connectivity_models(&key.station_base_url, &api_key)
            .unwrap_or_default();
    let candidates =
        station_key_connectivity_model_candidates(capabilities.as_ref(), None, &discovered_models);
    let (model, result) = run_station_key_connectivity_model_attempts(&candidates, |candidate| {
        run_station_key_connectivity_single_model_probe(
            &upstream_api_format,
            capabilities.as_ref(),
            |kind| {
                send_station_key_connectivity_probe(
                    &key.station_base_url,
                    &api_key,
                    candidate,
                    kind,
                )
            },
        )
    });

    record_station_key_connectivity_result(
        database,
        station_key_id,
        result.ok,
        result.duration_ms,
        &result.message,
    )?;

    Ok(StationKeyConnectivityTestResult {
        station_key_id: station_key_id.to_string(),
        ok: result.ok,
        status_code: result.status_code,
        duration_ms: result.duration_ms,
        model,
        message: result.message,
    })
}

fn build_station_key_connectivity_probe_url(
    base_url: &str,
    kind: StationKeyConnectivityProbeKind,
) -> String {
    let path = match kind {
        StationKeyConnectivityProbeKind::Responses => "/v1/responses",
        StationKeyConnectivityProbeKind::ChatCompletions => "/v1/chat/completions",
    };
    build_upstream_url(base_url, path)
}

fn build_station_key_connectivity_probe_body(
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Value {
    match kind {
        StationKeyConnectivityProbeKind::Responses => json!({
            "model": model,
            "input": "ping",
            "store": false,
            "max_output_tokens": 1,
        }),
        StationKeyConnectivityProbeKind::ChatCompletions => json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": "ping",
            }],
            "stream": false,
            "max_tokens": 16,
        }),
    }
}

fn should_try_station_key_connectivity_chat_fallback(
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
    status_code: u16,
) -> bool {
    if !matches!(
        upstream_api_format,
        UpstreamApiFormat::Auto | UpstreamApiFormat::CustomOpenAiCompatible
    ) {
        return false;
    }
    if capabilities
        .map(|capabilities| !capabilities.supports_chat_completions)
        .unwrap_or(false)
    {
        return false;
    }
    matches!(status_code, 404 | 405 | 501) || should_fallback(status_code)
}

fn station_key_connectivity_model_candidates(
    capabilities: Option<&StationKeyCapabilities>,
    configured_model: Option<&str>,
    discovered_models: &[String],
) -> Vec<String> {
    let mut candidates = Vec::new();
    let blocked_models = capabilities
        .map(|capabilities| {
            capabilities
                .model_blocklist
                .iter()
                .map(|model| normalize_connectivity_model(model))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    push_station_key_connectivity_model_candidate(
        &mut candidates,
        configured_model,
        &blocked_models,
    );
    if let Some(capabilities) = capabilities {
        let explicit_models = if capabilities.model_allowlist.is_empty() {
            capabilities.preferred_models.as_slice()
        } else {
            capabilities.model_allowlist.as_slice()
        };
        let mut explicit_models = explicit_models.to_vec();
        explicit_models.sort_by_key(|model| connectivity_model_priority(model));
        for model in &explicit_models {
            push_station_key_connectivity_model_candidate(
                &mut candidates,
                Some(model.as_str()),
                &blocked_models,
            );
        }
    }
    let mut discovered_models = discovered_models.iter().enumerate().collect::<Vec<_>>();
    discovered_models.sort_by_key(|(index, model)| (connectivity_model_priority(model), *index));
    for (_, model) in discovered_models {
        push_station_key_connectivity_model_candidate(
            &mut candidates,
            Some(model.as_str()),
            &blocked_models,
        );
    }
    if candidates.is_empty() {
        candidates.push("gpt-4o-mini".to_string());
    }
    candidates.truncate(8);
    candidates
}

fn push_station_key_connectivity_model_candidate(
    candidates: &mut Vec<String>,
    model: Option<&str>,
    blocked_models: &[String],
) {
    let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return;
    };
    let normalized = normalize_connectivity_model(model);
    if blocked_models.iter().any(|blocked| blocked == &normalized) {
        return;
    }
    if !candidates
        .iter()
        .any(|candidate| normalize_connectivity_model(candidate) == normalized)
    {
        candidates.push(model.to_string());
    }
}

fn connectivity_model_priority(model: &str) -> i32 {
    let normalized = normalize_connectivity_model(model);
    if normalized.contains("nano") {
        return 0;
    }
    if normalized.contains("mini") {
        return 1;
    }
    if normalized.contains("lite") {
        return 2;
    }
    if normalized.contains("flash") {
        return 3;
    }
    if normalized.contains("haiku") {
        return 4;
    }
    if normalized.contains("turbo") {
        return 5;
    }
    if normalized == "deepseek-chat" || normalized.ends_with("-chat") {
        return 6;
    }
    20
}

fn normalize_connectivity_model(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

fn run_station_key_connectivity_model_attempts<F>(
    candidates: &[String],
    mut probe: F,
) -> (String, StationKeyConnectivityProbeResult)
where
    F: FnMut(&str) -> StationKeyConnectivityProbeResult,
{
    let fallback_candidates;
    let candidates = if candidates.is_empty() {
        fallback_candidates = vec!["gpt-4o-mini".to_string()];
        fallback_candidates.as_slice()
    } else {
        candidates
    };
    let mut last = None;
    for model in candidates {
        let result = probe(model);
        if result.ok {
            return (model.clone(), result);
        }
        last = Some((model.clone(), result));
    }
    last.unwrap_or_else(|| {
        (
            "gpt-4o-mini".to_string(),
            StationKeyConnectivityProbeResult::failure(0, 0, "未执行连通性探测".to_string()),
        )
    })
}

fn run_station_key_connectivity_single_model_probe<F>(
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
    mut send_probe: F,
) -> StationKeyConnectivityProbeResult
where
    F: FnMut(StationKeyConnectivityProbeKind) -> StationKeyConnectivityProbeResult,
{
    let response_result = send_probe(StationKeyConnectivityProbeKind::Responses);
    if response_result.ok {
        return response_result;
    }
    if !should_try_station_key_connectivity_chat_fallback(
        upstream_api_format,
        capabilities,
        response_result.status_code,
    ) {
        return response_result;
    }

    let chat_result = send_probe(StationKeyConnectivityProbeKind::ChatCompletions);
    let duration_ms = response_result
        .duration_ms
        .saturating_add(chat_result.duration_ms);
    if chat_result.ok {
        return StationKeyConnectivityProbeResult::success(
            chat_result.status_code,
            duration_ms,
            format!(
                "Responses 直连返回 HTTP {}，Chat Completions 兼容探测正常",
                response_result.status_code
            ),
        );
    }

    StationKeyConnectivityProbeResult::failure(
        chat_result.status_code,
        duration_ms,
        format!(
            "Responses: {}; Chat Completions: {}",
            response_result.message, chat_result.message
        ),
    )
}

fn send_station_key_connectivity_probe(
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> StationKeyConnectivityProbeResult {
    let url = build_station_key_connectivity_probe_url(base_url, kind);
    let body = build_station_key_connectivity_probe_body(model, kind);
    let started = Instant::now();
    let response_result = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_json(body);
    let duration_ms = started.elapsed().as_millis().min(i64::MAX as u128) as i64;
    let (status_code, response_text) = match response_result {
        Ok(response) => response_text_pair(response),
        Err(ureq::Error::Status(_, response)) => response_text_pair(response),
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                duration_ms,
                redact_error_message(&format!("{error}")),
            );
        }
    };
    if (200..300).contains(&status_code) {
        let message = match kind {
            StationKeyConnectivityProbeKind::Responses => "Responses 连通正常",
            StationKeyConnectivityProbeKind::ChatCompletions => "Chat Completions 连通正常",
        };
        return StationKeyConnectivityProbeResult::success(
            status_code,
            duration_ms,
            message.to_string(),
        );
    }
    StationKeyConnectivityProbeResult::failure(
        status_code,
        duration_ms,
        response_error_message(&response_text, status_code),
    )
}

fn discover_station_key_connectivity_models(base_url: &str, api_key: &str) -> Option<Vec<String>> {
    let url = build_upstream_url(base_url, "/v1/models");
    let response = ureq::get(&url)
        .timeout(std::time::Duration::from_secs(10))
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .call()
        .ok()?;
    if !(200..300).contains(&response.status()) {
        return None;
    }
    let body = response.into_string().ok()?;
    let value = serde_json::from_str::<Value>(&body).ok()?;
    let models = model_ids_from_models_response(&value);
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

fn model_ids_from_models_response(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .filter(|model| !model.trim().is_empty())
        .map(|model| model.trim().to_string())
        .collect()
}

fn response_text_pair(response: ureq::Response) -> (u16, String) {
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    (status, text)
}

fn response_error_message(response_text: &str, status_code: u16) -> String {
    let parsed = serde_json::from_str::<Value>(response_text).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
        })
        .unwrap_or(response_text)
        .trim();
    let fallback = if message.is_empty() {
        format!("Responses 返回 HTTP {status_code}")
    } else {
        message.to_string()
    };
    redact_error_message(&fallback)
}

fn record_station_key_connectivity_result(
    database: &AppDatabase,
    station_key_id: &str,
    ok: bool,
    duration_ms: i64,
    message: &str,
) -> Result<(), String> {
    let now = now_millis_for_services().to_string();
    if ok {
        database.record_station_key_success(station_key_id, duration_ms, &now)?;
        database.touch_station_key_usage(station_key_id, "healthy", None, Some(&now))
    } else {
        database.record_station_key_failure(station_key_id, message, &now)?;
        database.touch_station_key_usage(station_key_id, "error", None, Some(&now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(target_os = "windows")]
    fn ccswitch_protocol_urls_use_windows_file_protocol_handler() {
        let launcher = system_url_launcher("ccswitch://v1/import?resource=provider");

        assert_eq!(launcher.program, "rundll32.exe");
        assert_eq!(
            launcher.args,
            vec![
                "url.dll,FileProtocolHandler",
                "ccswitch://v1/import?resource=provider"
            ]
        );
    }

    #[test]
    fn ccswitch_deeplink_matches_sub2api_codex_import_shape() {
        let deeplink = build_ccswitch_provider_deeplink(
            "codex",
            "Relay Pool Desktop",
            "http://127.0.0.1:8787",
            "http://127.0.0.1:8787/v1",
            "sk test",
        );

        assert!(deeplink.starts_with("ccswitch://v1/import?"));
        assert!(deeplink.contains("resource=provider"));
        assert!(deeplink.contains("app=codex"));
        assert!(deeplink.contains("model=gpt-5.4"));
        assert!(deeplink.contains("name=Relay+Pool+Desktop"));
        assert!(deeplink.contains("homepage=http%3A%2F%2F127.0.0.1%3A8787"));
        assert!(deeplink.contains("endpoint=http%3A%2F%2F127.0.0.1%3A8787%2Fv1"));
        assert!(deeplink.contains("apiKey=sk+test"));
        assert!(deeplink.contains("configFormat=json"));
        assert!(deeplink.contains("usageEnabled=true"));
        assert!(deeplink.contains("usageAutoInterval=30"));
        assert!(deeplink.contains("usageScript="));
    }

    #[test]
    fn station_key_connectivity_probe_uses_low_token_responses_request() {
        let body = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
        );

        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["input"], "ping");
        assert_eq!(body["store"], false);
        assert_eq!(body["max_output_tokens"], 1);
    }

    #[test]
    fn station_key_connectivity_candidates_choose_lowest_allowed_model() {
        let capabilities = StationKeyCapabilities {
            station_key_id: "key-lowest".to_string(),
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: false,
            supports_stream: true,
            supports_tools: false,
            supports_vision: false,
            supports_reasoning: false,
            model_allowlist: vec![
                "gpt-4.1".to_string(),
                "gpt-4.1-mini".to_string(),
                "claude-sonnet-4".to_string(),
            ],
            model_blocklist: Vec::new(),
            preferred_models: vec!["gpt-4.1".to_string()],
            only_use_as_backup: false,
            routing_tags: Vec::new(),
            updated_at: "0".to_string(),
        };

        let candidates = station_key_connectivity_model_candidates(Some(&capabilities), None, &[]);

        assert_eq!(candidates[0], "gpt-4.1-mini");
        assert!(!candidates.contains(&"gpt-4o-mini".to_string()));
    }

    #[test]
    fn station_key_connectivity_probe_posts_to_responses_endpoint() {
        let url = build_station_key_connectivity_probe_url(
            "https://relay.example/v1",
            StationKeyConnectivityProbeKind::Responses,
        );

        assert_eq!(url, "https://relay.example/v1/responses");
    }

    #[test]
    fn station_key_connectivity_candidates_use_discovered_model_when_not_configured() {
        let discovered = vec!["claude-test".to_string()];
        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["claude-test"]);
    }

    #[test]
    fn station_key_connectivity_candidates_keep_multiple_discovered_models() {
        let discovered = vec![
            "codex-auto-review".to_string(),
            "gpt-5.4".to_string(),
            "gpt-5.5".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["codex-auto-review", "gpt-5.4", "gpt-5.5"]);
    }

    #[test]
    fn station_key_connectivity_candidates_sort_discovered_models_by_lowest_cost() {
        let discovered = vec![
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini", "gpt-4.1"]);
    }

    #[test]
    fn station_key_connectivity_attempts_next_model_after_503() {
        let candidates = vec!["codex-auto-review".to_string(), "gpt-5.4".to_string()];
        let mut attempted = Vec::new();

        let (model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                attempted.push(candidate.to_string());
                if candidate == "gpt-5.4" {
                    StationKeyConnectivityProbeResult::success(
                        200,
                        42,
                        "Chat Completions 连通正常".to_string(),
                    )
                } else {
                    StationKeyConnectivityProbeResult::failure(
                        503,
                        12,
                        "Service temporarily unavailable".to_string(),
                    )
                }
            });

        assert_eq!(attempted, vec!["codex-auto-review", "gpt-5.4"]);
        assert_eq!(model, "gpt-5.4");
        assert!(result.ok);
    }

    #[test]
    fn station_key_connectivity_attempts_next_model_after_responses_and_chat_fail() {
        let candidates = vec!["codex-auto-review".to_string(), "gpt-5.4".to_string()];
        let mut attempted = Vec::new();

        let (model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::Auto,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match (candidate, kind) {
                            ("gpt-5.4", StationKeyConnectivityProbeKind::ChatCompletions) => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    11,
                                    "Chat Completions 连通正常".to_string(),
                                )
                            }
                            _ => StationKeyConnectivityProbeResult::failure(
                                503,
                                7,
                                "Service temporarily unavailable".to_string(),
                            ),
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
                (
                    "gpt-5.4".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "gpt-5.4".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
            ]
        );
        assert_eq!(model, "gpt-5.4");
        assert!(result.ok);
        assert_eq!(result.status_code, 200);
        assert_eq!(result.duration_ms, 18);
        assert_eq!(
            result.message,
            "Responses 直连返回 HTTP 503，Chat Completions 兼容探测正常"
        );
    }

    #[test]
    fn station_key_connectivity_chat_probe_uses_low_token_request() {
        let body = build_station_key_connectivity_probe_body(
            "claude-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
        );

        assert_eq!(body["model"], "claude-test");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "ping");
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], 16);
    }

    #[test]
    fn station_key_connectivity_auto_format_can_fallback_to_chat_on_503() {
        assert!(should_try_station_key_connectivity_chat_fallback(
            &UpstreamApiFormat::Auto,
            None,
            503,
        ));
    }
}
