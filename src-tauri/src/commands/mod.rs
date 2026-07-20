use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::process::Command;
use std::time::{Duration, Instant};
use std::{
    io::Read,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{ipc::Channel, Manager, State};

pub(crate) mod credentials;
pub(crate) mod routing;
pub(crate) mod settings;
pub(crate) mod stations;

use crate::{
    models::{
        capture::{CaptureSessionStatus, CapturedHttpEvent, CapturedHttpEventInput},
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
            PersistStationSessionInput, StationCredentials, UpdateStationCredentialsInput,
            UpdateStationSessionInput,
        },
        group_facts::{
            GroupRateRecord, StationGroupBinding, UpdateStationKeyGroupBindingInput,
            UpsertStationGroupBindingInput,
        },
        pricing::{
            BalanceSnapshot, ModelBasePrice, PricingRule, PricingStatus, RequestKind,
            ResolvedPricingContext, UpsertBalanceSnapshotInput, UpsertModelBasePriceInput,
            UpsertPricingRuleInput,
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
            ChannelMonitorSummary, ChannelStatusSummary, ChannelStatusWorkspace,
            PricingComparisonWorkspace, SaveStationKeyWithDefaultsInput,
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
        data_store::{
            backup::backup_selected_database,
            config::{create_installation_marker, write_config, DataDirConfigV2},
            diagnostic::build_diagnostic_report,
            inspect::inspect_candidate,
            inspect_startup,
            relocation::write_active_data_dir_selection,
            types::{
                ActivationResult, CandidateHealth, CandidateRole, DataStoreCandidate,
                DataStoreStartupState, DataStoreStartupView,
            },
        },
        database::{now_millis_for_services, AppDatabase},
        endpoint_ping::ping_station_endpoint as probe_station_endpoint,
        pricing::{pricing_context_from_pricing_parts, RequestPricingParts},
        proxy::{
            redact_error_message,
            runtime::{ProxyRuntimeState, ProxyStartConfig},
            should_fallback,
        },
        remote_keys,
        secrets::{validation::validate_database_secrets, SecretManager},
        station_endpoints::{build_api_url, url_belongs_to_base},
        updater::{self, PublishedUpdateInspection, UpdaterNetworkConfig},
    },
};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";
const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";

const STATION_KEY_CONNECTIVITY_MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const STATION_KEY_CONNECTIVITY_CANDIDATE_LIMIT: usize = 2;
const STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT: usize = 64 * 1024;
const DEFAULT_STATION_KEY_CONNECTIVITY_MODEL: &str = "gpt-4.1-mini";
#[tauri::command]
pub fn app_status() -> AppStatus {
    AppStatus::default()
}

#[tauri::command]
pub fn get_data_store_startup_state(
    state: State<'_, DataStoreStartupState>,
) -> DataStoreStartupView {
    state.view()
}

#[tauri::command]
pub fn refresh_data_store_candidates(
    state: State<'_, DataStoreStartupState>,
) -> Result<DataStoreStartupView, String> {
    inspect_startup(state.default_data_dir()).map(|state| state.view())
}

#[tauri::command]
pub fn locate_data_store_candidate() -> Result<Option<DataStoreCandidate>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Relay Pool SQLite", &["sqlite3"])
        .pick_file()
    else {
        return Ok(None);
    };
    if path.file_name().and_then(|name| name.to_str()) != Some(DATABASE_FILE) {
        return Err(format!("selected database must be named {DATABASE_FILE}"));
    }
    inspect_candidate(&path, CandidateRole::Located)
        .map(|inspected| inspected.candidate)
        .map(Some)
}

#[tauri::command]
pub fn activate_data_store_candidate(
    state: State<'_, DataStoreStartupState>,
    secrets: State<'_, SecretManager>,
    candidate_path: String,
) -> Result<ActivationResult, String> {
    let candidate_path = PathBuf::from(candidate_path);
    let canonical_path = candidate_path
        .canonicalize()
        .map_err(|error| format!("failed to resolve selected database path: {error}"))?;
    if canonical_path.file_name().and_then(|name| name.to_str()) != Some(DATABASE_FILE) {
        return Err(format!("selected database must be named {DATABASE_FILE}"));
    }

    let inspected = inspect_candidate(&canonical_path, CandidateRole::Located)?;
    if inspected.candidate.health != CandidateHealth::Healthy
        || !inspected.contains_relay_pool_schema
        || !inspected.candidate.schema_compatible
    {
        return Err("selected database is not a healthy Relay Pool database".to_string());
    }
    validate_database_secrets(&canonical_path, secrets.data_key())?;
    backup_selected_database(&canonical_path, state.default_data_dir())?;

    let active_data_dir = canonical_path
        .parent()
        .ok_or_else(|| "selected database path has no parent directory".to_string())?;
    write_config(
        &state.default_data_dir().join(DATA_DIR_CONFIG_FILE),
        &DataDirConfigV2 {
            version: 2,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: None,
            source_data_dir: None,
            updated_at: data_store_updated_at(),
        },
    )?;
    create_installation_marker(state.default_data_dir())?;

    Ok(ActivationResult {
        restart_required: true,
    })
}

#[tauri::command]
pub fn create_new_data_store(
    state: State<'_, DataStoreStartupState>,
    confirmed: bool,
) -> Result<ActivationResult, String> {
    if !confirmed {
        return Err("creating a new data store requires confirmation".to_string());
    }
    let Some(data_dir) = rfd::FileDialog::new().pick_folder() else {
        return Err("no data directory selected".to_string());
    };
    let db_path = data_dir.join(DATABASE_FILE);
    if db_path.exists() {
        return Err(format!(
            "target database already exists: {}",
            db_path.display()
        ));
    }
    let database =
        AppDatabase::initialize_new_at(state.default_data_dir().to_path_buf(), data_dir.clone())?;
    drop(database);
    write_active_data_dir_selection(state.default_data_dir(), &data_dir)?;
    create_installation_marker(state.default_data_dir())?;
    Ok(ActivationResult {
        restart_required: true,
    })
}

#[tauri::command]
pub fn open_data_store_backup_dir(state: State<'_, DataStoreStartupState>) -> Result<(), String> {
    let backups = state.default_data_dir().join("backups");
    std::fs::create_dir_all(&backups).map_err(|error| {
        format!(
            "failed to create backup directory {}: {error}",
            backups.display()
        )
    })?;
    open_path_with_system(&backups)
}

#[tauri::command]
pub fn export_data_store_diagnostic(
    state: State<'_, DataStoreStartupState>,
) -> Result<Option<String>, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name("relay-pool-data-store-diagnostic.json")
        .save_file()
    else {
        return Ok(None);
    };
    let report = build_diagnostic_report(state.default_data_dir(), &state)?;
    let bytes = serde_json::to_vec_pretty(&report)
        .map_err(|error| format!("failed to serialize data-store diagnostic: {error}"))?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            format!(
                "failed to create diagnostic directory {}: {error}",
                parent.display()
            )
        })?;
    }
    std::fs::write(&path, bytes)
        .map_err(|error| format!("failed to write diagnostic {}: {error}", path.display()))?;
    Ok(Some(path.display().to_string()))
}

fn data_store_updated_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
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
    database.ensure_secure_local_access_key()
}

#[tauri::command]
pub fn update_local_access_key(
    database: State<'_, AppDatabase>,
    value: String,
) -> Result<AppSettings, String> {
    database.update_local_access_key(value)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CcswitchImportResult {
    app: String,
    provider_name: String,
    endpoint: String,
}

#[tauri::command]
pub async fn import_relay_pool_to_ccswitch(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<CcswitchImportResult, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(secrets.data_key())?;
    let proxy_status = proxy
        .start(ProxyStartConfig::new(
            database.inner().clone(),
            *secrets.data_key(),
            settings.local_proxy_port,
        ))
        .await?;
    let (result, deeplink) = prepare_ccswitch_import(&database, &proxy_status)?;

    open_url_with_system(&deeplink)?;

    Ok(result)
}

fn prepare_ccswitch_import(
    database: &AppDatabase,
    status: &ProxyStatus,
) -> Result<(CcswitchImportResult, String), String> {
    let local_access_key = database.ensure_secure_local_access_key()?;
    let endpoint = format!("http://{}:{}/v1", status.bind_addr, status.port);
    let homepage = format!("http://{}:{}", status.bind_addr, status.port);
    let provider_name = "Relay Pool Desktop".to_string();
    let deeplink = build_ccswitch_provider_deeplink(
        "codex",
        &provider_name,
        &homepage,
        &endpoint,
        &local_access_key,
    );
    Ok((
        CcswitchImportResult {
            app: "codex".to_string(),
            provider_name,
            endpoint,
        },
        deeplink,
    ))
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let url = validate_external_http_url(&url)?;
    open_url_with_system(url)
}

#[tauri::command]
pub fn updater_network_config() -> UpdaterNetworkConfig {
    updater::network_config()
}

#[tauri::command]
pub async fn inspect_latest_update_manifest(
    current_version: String,
) -> Result<PublishedUpdateInspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        updater::inspect_latest_update_manifest(&current_version)
    })
    .await
    .map_err(|error| format!("Updater manifest task failed: {error}"))?
}

#[tauri::command]
pub fn update_settings(
    database: State<'_, AppDatabase>,
    input: UpdateSettingsInput,
) -> Result<AppSettings, String> {
    database.update_settings(input)
}

#[tauri::command]
pub fn choose_data_dir(database: State<'_, AppDatabase>) -> Result<AppSettings, String> {
    let Some(data_dir) = rfd::FileDialog::new().pick_folder() else {
        return database.get_settings();
    };
    database.set_pending_data_dir(data_dir)
}

#[tauri::command]
pub fn reset_data_dir(database: State<'_, AppDatabase>) -> Result<AppSettings, String> {
    database.reset_data_dir_to_default()
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
pub fn load_local_routing_workspace(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, String> {
    let settings = database.get_settings()?;
    let proxy_status = proxy.status(settings.local_proxy_port);
    database.load_local_routing_workspace(proxy_status)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderLocalRoutingKeysInput {
    pub station_key_ids: Vec<String>,
}

#[tauri::command]
pub fn reorder_local_routing_keys(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
    input: ReorderLocalRoutingKeysInput,
) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, String> {
    database.reorder_local_routing_keys(input.station_key_ids)?;
    let settings = database.get_settings()?;
    let proxy_status = proxy.status(settings.local_proxy_port);
    database.load_local_routing_workspace(proxy_status)
}

#[tauri::command]
pub async fn start_local_proxy(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let status = crate::services::proxy::startup::start_from_persisted_settings(
        database.inner(),
        *secrets.data_key(),
        proxy.inner(),
    )
    .await?;
    if let Err(error) = database.set_local_proxy_start_on_launch(true) {
        let _ = proxy.stop(status.port).await;
        return Err(error);
    }
    Ok(status)
}

#[tauri::command]
pub async fn stop_local_proxy(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    let status = proxy.stop(settings.local_proxy_port).await?;
    database.set_local_proxy_start_on_launch(false)?;
    Ok(status)
}

#[tauri::command]
pub async fn cleanup_before_update(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    proxy.cleanup_before_update(settings.local_proxy_port).await
}

#[tauri::command]
pub async fn prepare_local_proxy_for_update(
    database: State<'_, AppDatabase>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let _settings = database.get_settings()?;
    proxy.prepare_for_update(Duration::from_secs(30)).await
}

#[tauri::command]
pub async fn restart_local_proxy(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    proxy: State<'_, ProxyRuntimeState>,
) -> Result<ProxyStatus, String> {
    let settings = database.get_settings()?;
    database.migrate_plaintext_secrets(secrets.data_key())?;
    let status = proxy
        .restart(ProxyStartConfig::new(
            database.inner().clone(),
            *secrets.data_key(),
            settings.local_proxy_port,
        ))
        .await?;
    if let Err(error) = database.set_local_proxy_start_on_launch(true) {
        let _ = proxy.stop(status.port).await;
        return Err(error);
    }
    Ok(status)
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
    run_since: Option<String>,
    run_limit: Option<usize>,
) -> Result<Vec<ChannelMonitorSummary>, String> {
    database.list_channel_monitor_summaries(run_since.as_deref(), run_limit)
}

#[tauri::command]
pub fn list_channel_status_summaries(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ChannelStatusSummary>, String> {
    database.list_channel_status_summaries()
}

#[tauri::command]
pub fn load_channel_status_workspace(
    database: State<'_, AppDatabase>,
) -> Result<ChannelStatusWorkspace, String> {
    database.load_channel_status_workspace()
}

#[tauri::command]
pub fn load_pricing_comparison_workspace(
    database: State<'_, AppDatabase>,
) -> Result<PricingComparisonWorkspace, String> {
    database.load_pricing_comparison_workspace()
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
        &station.api_base_url,
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
    response_mode: StationKeyConnectivityResponseMode,
    stream_fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StationKeyConnectivityProbeKind {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StationKeyConnectivityRequestMode {
    Stream,
    NonStream,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StationKeyConnectivityResponseMode {
    Stream,
    NonStreamFallback,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StationKeyConnectivityTestEvent {
    AttemptStarted { model: String, protocol: String },
    Delta { text: String },
    Fallback { reason: String },
}

#[derive(Debug, Clone)]
struct StationKeyConnectivityProbeResult {
    ok: bool,
    status_code: u16,
    duration_ms: i64,
    message: String,
    response_mode: StationKeyConnectivityResponseMode,
    stream_fallback_reason: Option<String>,
}

impl StationKeyConnectivityProbeResult {
    fn success(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: true,
            status_code,
            duration_ms,
            message,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    fn failure(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: false,
            status_code,
            duration_ms,
            message,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    fn with_response_mode(mut self, response_mode: StationKeyConnectivityResponseMode) -> Self {
        self.response_mode = response_mode;
        self
    }

    fn with_stream_fallback_reason(mut self, reason: Option<String>) -> Self {
        self.stream_fallback_reason = reason;
        self
    }
}

#[tauri::command]
pub async fn test_station_key_connectivity(
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    station_key_id: String,
    model: String,
    progress: Channel<StationKeyConnectivityTestEvent>,
) -> Result<StationKeyConnectivityTestResult, String> {
    let database = database.inner().clone();
    let data_key = *secrets.data_key();
    tauri::async_runtime::spawn_blocking(move || {
        test_station_key_connectivity_blocking(
            &database,
            &data_key,
            &station_key_id,
            &model,
            progress,
        )
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
pub fn list_model_base_prices(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ModelBasePrice>, String> {
    database.list_model_base_prices()
}

#[tauri::command]
pub fn upsert_model_base_price(
    database: State<'_, AppDatabase>,
    input: UpsertModelBasePriceInput,
) -> Result<ModelBasePrice, String> {
    database.upsert_model_base_price(input)
}

#[tauri::command]
pub fn reset_model_base_prices_to_builtins(
    database: State<'_, AppDatabase>,
) -> Result<Vec<ModelBasePrice>, String> {
    database.reset_model_base_prices_to_builtins()
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
pub fn resolve_station_key_pricing_context(
    database: State<'_, AppDatabase>,
    station_key_id: String,
    requested_model: String,
    request_kind: Option<RequestKind>,
) -> Result<ResolvedPricingContext, String> {
    let model = requested_model.trim().to_string();
    let economics = database
        .route_candidate_economics_for_model(station_key_id.clone(), Some(model.clone()))?;
    let Some(economics) = economics else {
        return Ok(ResolvedPricingContext {
            station_key_id,
            station_id: "unknown".to_string(),
            requested_model: model,
            resolved_model: "unknown".to_string(),
            request_kind: request_kind.unwrap_or(RequestKind::Text),
            group_binding_id: None,
            base_input_price: None,
            base_output_price: None,
            base_fixed_price: None,
            currency: "unknown".to_string(),
            unit: "per_1m_tokens".to_string(),
            base_price_source: None,
            effective_rate_multiplier: None,
            rate_source: None,
            rate_collected_at: None,
            estimated_input_price: None,
            estimated_output_price: None,
            estimated_fixed_price: None,
            pricing_status: PricingStatus::Unpriced,
            confidence: 0.0,
            source_chain: Vec::new(),
            reason: Some("pricing_not_available".to_string()),
            resolved_at: now_millis_for_services().to_string(),
        });
    };

    let mut context = pricing_context_from_pricing_parts(&RequestPricingParts {
        station_key_id: &station_key_id,
        station_id: None,
        model: Some(&model),
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
    });
    context.request_kind = request_kind.unwrap_or(RequestKind::Text);
    Ok(context)
}

#[tauri::command]
pub fn list_balance_snapshots(
    database: State<'_, AppDatabase>,
) -> Result<Vec<BalanceSnapshot>, String> {
    database.list_balance_snapshots()
}

#[tauri::command]
pub fn list_current_station_balance_snapshots(
    database: State<'_, AppDatabase>,
) -> Result<Vec<BalanceSnapshot>, String> {
    database.list_current_station_balance_snapshots()
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
pub fn mark_change_events_read(
    database: State<'_, AppDatabase>,
    ids: Vec<String>,
) -> Result<Vec<ChangeEvent>, String> {
    database.mark_change_events_read(ids)
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
    secrets: State<'_, SecretManager>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CaptureSessionStatus, String> {
    let station = database.station_for_collector(&station_id)?;
    let credentials = database.get_station_credentials(station_id.clone())?;
    let login_password = if credentials.password_present {
        database.get_station_login_password_with_data_key(station_id.clone(), secrets.data_key())?
    } else {
        None
    };
    let label = capture_window_label(&station_id);
    let endpoint_revision = station.endpoint_revision;
    let script = capture_script(
        &station_id,
        &label,
        credentials.login_username.as_deref(),
        login_password.as_deref(),
    );
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
                let target_url = station.website_url.clone();
                let target = target_url
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
    sessions.start(station_id, label, endpoint_revision)
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
    secrets: State<'_, SecretManager>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: CapturedHttpEventInput,
) -> Result<CaptureSessionStatus, String> {
    let station = database.station_for_collector(&input.station_id)?;
    if !capture_request_belongs_to_station(
        &station.website_url,
        &station.api_base_url,
        &input.request_url,
    ) {
        return Err("捕获事件不属于当前站点 Base URL，已拒绝。".to_string());
    }
    let web_authorization_user_id = web_authorization_candidate_user_id_from_input(&input);
    if let Some(session) = capture::extract_session_credentials(&input) {
        let expected_revision = sessions
            .endpoint_revision(&input.station_id)?
            .ok_or_else(capture_endpoint_revision_missing_message)?;
        database
            .persist_station_session_if_revision(session, expected_revision, secrets.data_key())
            .map_err(capture_endpoint_revision_error)?;
    }
    let station_id = input.station_id.clone();
    let event = capture::sanitize_event(input);
    sessions.push_event(&station_id, event, web_authorization_user_id)
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
    finish_capture_session_from_events(&database, &sessions, station_id, None)
}

fn finish_capture_session_from_events(
    database: &AppDatabase,
    sessions: &capture::session::CaptureSessionStore,
    station_id: String,
    web_authorization_summary: Option<Value>,
) -> Result<CollectorRunResult, String> {
    let events = sessions.take_events(&station_id)?;
    finish_capture_session_with_events(database, station_id, events, web_authorization_summary)
}

fn finish_capture_session_with_events(
    database: &AppDatabase,
    station_id: String,
    events: Vec<CapturedHttpEvent>,
    web_authorization_summary: Option<Value>,
) -> Result<CollectorRunResult, String> {
    let (mut summary, normalized, raw) = capture::summarize_events(&events);
    if let Some(web_authorization_summary) = web_authorization_summary {
        if let Some(summary) = summary.as_object_mut() {
            summary.insert("webAuthorization".to_string(), web_authorization_summary);
        }
    }
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

#[tauri::command]
pub async fn finish_web_authorization_session(
    app: tauri::AppHandle,
    database: State<'_, AppDatabase>,
    secrets: State<'_, SecretManager>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let station = database.station_for_collector(&station_id)?;
    let candidate = sessions
        .web_authorization_candidate(&station_id)?
        .ok_or_else(|| {
            "网页登录授权尚未捕获到用户身份，请在授权窗口完成登录后重试。".to_string()
        })?;
    let expected_revision = sessions
        .endpoint_revision(&station_id)?
        .ok_or_else(capture_endpoint_revision_missing_message)?;
    let cookie_header =
        read_capture_window_cookie_header(app, &station_id, &station.website_url).await?;
    let verified = capture::web_authorization::verify_newapi_cookie_session(
        &station.website_url,
        &cookie_header,
        &candidate.user_id,
        Duration::from_secs(20),
    )?;

    let (_, events) = sessions.commit_web_authorization(&station_id, &candidate, || {
        database
            .persist_station_session_if_revision(
                PersistStationSessionInput {
                    station_id: station_id.clone(),
                    access_token: None,
                    refresh_token: None,
                    cookie: Some(verified.cookie_header),
                    newapi_user_id: Some(verified.newapi_user_id),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: verified.session_source,
                },
                expected_revision,
                secrets.data_key(),
            )
            .map_err(capture_endpoint_revision_error)
    })?;

    finish_capture_session_with_events(
        &database,
        station_id,
        events,
        Some(capture::web_authorization_summary(
            "success",
            Some("web_authorization"),
            true,
        )),
    )
}

fn capture_window_label(station_id: &str) -> String {
    format!(
        "capture-{}",
        station_id.replace(|character: char| !character.is_ascii_alphanumeric(), "-")
    )
}

async fn read_capture_window_cookie_header(
    app: tauri::AppHandle,
    station_id: &str,
    station_website_url: &str,
) -> Result<String, String> {
    let label = capture_window_label(station_id);
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| "网页登录授权窗口不存在，请重新打开授权窗口。".to_string())?;
    let target = tauri::Url::parse(station_website_url)
        .map_err(|error| format!("站点管理地址无法用于读取 Cookie: {error}"))?;

    let cookies = tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(target))
        .await
        .map_err(|error| format!("读取网页登录授权 Cookie 任务失败: {error}"))?
        .map_err(|error| format!("读取网页登录授权 Cookie 失败: {error}"))?;

    let pairs = cookies
        .into_iter()
        .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
        .collect::<Vec<_>>();
    capture::web_authorization::build_cookie_header_from_pairs(&pairs)
        .ok_or_else(|| "网页登录授权未捕获到可用 Cookie，请确认已在授权窗口完成登录。".to_string())
}

fn capture_request_belongs_to_station(
    station_website_url: &str,
    station_api_base_url: &str,
    request_url: &str,
) -> bool {
    [station_website_url, station_api_base_url]
        .into_iter()
        .any(|base_url| url_belongs_to_base(request_url, base_url))
}

fn capture_endpoint_revision_missing_message() -> String {
    "endpoint_revision_changed: 捕获会话已过期，请重新打开网页登录 / 捕获窗口。".to_string()
}

fn capture_endpoint_revision_error(error: String) -> String {
    if error == "station_endpoint_revision_changed" {
        capture_endpoint_revision_missing_message()
    } else {
        error
    }
}

fn web_authorization_candidate_user_id_from_input(
    input: &CapturedHttpEventInput,
) -> Option<String> {
    let fallback_path;
    let request_path = if let Some(path) = input.request_path.as_deref() {
        path
    } else {
        fallback_path = path_from_request_url(&input.request_url);
        &fallback_path
    };
    if !capture::web_authorization::is_newapi_completion_candidate(
        request_path,
        input.status,
        input.response_json.as_ref(),
    ) {
        return None;
    }
    input
        .response_json
        .as_ref()
        .and_then(capture::web_authorization::extract_verified_user_id)
}

fn path_from_request_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let path = without_scheme
        .find('/')
        .map(|index| &without_scheme[index..])
        .unwrap_or("/");
    path.split(['?', '#']).next().unwrap_or("/").to_string()
}

fn capture_script(
    station_id: &str,
    window_label: &str,
    login_username: Option<&str>,
    login_password: Option<&str>,
) -> String {
    let login_username_json =
        serde_json::to_string(&login_username).unwrap_or_else(|_| "null".to_string());
    let login_password_json =
        serde_json::to_string(&login_password).unwrap_or_else(|_| "null".to_string());
    format!(
        r#"
(() => {{
  if (window.__relayPoolCaptureInstalled) return;
  window.__relayPoolCaptureInstalled = true;
  const stationId = {station_id:?};
  const sourceWindowId = {window_label:?};
  const loginUsername = {login_username_json};
  const loginPassword = {login_password_json};
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
  const tryFinishWebAuthorization = (status) => {{
    if (!invoke || !status || !status.webAuthorizationCandidate) return;
    if (window.__relayPoolAuthorizationFinishInFlight) return;
    window.__relayPoolAuthorizationFinishInFlight = true;
    invoke("finish_web_authorization_session", {{ stationId }})
      .catch(() => undefined)
      .finally(() => {{
        window.__relayPoolAuthorizationFinishInFlight = false;
      }});
  }};
  const send = (input) => {{
    if (!invoke) return;
    invoke("record_capture_event", {{ input }})
      .then(tryFinishWebAuthorization)
      .catch(() => undefined);
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
  const setNativeValue = (element, value) => {{
    if (!element || value == null || element.value === value) return false;
    const prototype = Object.getPrototypeOf(element);
    const descriptor = prototype ? Object.getOwnPropertyDescriptor(prototype, "value") : null;
    if (descriptor && descriptor.set) descriptor.set.call(element, value);
    else element.value = value;
    element.dispatchEvent(new Event("input", {{ bubbles: true }}));
    element.dispatchEvent(new Event("change", {{ bubbles: true }}));
    return true;
  }};
  const candidateInput = (selectors) => {{
    for (const selector of selectors) {{
      const found = document.querySelector(selector);
      if (found && !found.disabled && !found.readOnly) return found;
    }}
    return null;
  }};
  const fillLoginForm = () => {{
    try {{
      setNativeValue(candidateInput([
        "input[type='email']",
        "input[name='email']",
        "input[name='username']",
        "input[name='user']",
        "input[autocomplete='username']",
        "input[placeholder*='邮箱']",
        "input[placeholder*='账号']",
        "input[placeholder*='email' i]",
      ]), loginUsername);
      setNativeValue(candidateInput([
        "input[type='password']",
        "input[name='password']",
        "input[autocomplete='current-password']",
        "input[placeholder*='密码']",
        "input[placeholder*='password' i]",
      ]), loginPassword);
      for (const checkbox of Array.from(document.querySelectorAll("input[type='checkbox']"))) {{
        const label = checkbox.closest("label") || (checkbox.id ? document.querySelector(`label[for="${{checkbox.id}}"]`) : null);
        const text = `${{checkbox.name || ""}} ${{checkbox.id || ""}} ${{label ? label.textContent || "" : ""}}`.toLowerCase();
        if (text.includes("agreement") || text.includes("attestation") || text.includes("region") || text.includes("大陆") || text.includes("中华人民共和国") || text.includes("独立陈述")) {{
          if (!checkbox.checked) {{
            checkbox.checked = true;
            checkbox.dispatchEvent(new Event("input", {{ bubbles: true }}));
            checkbox.dispatchEvent(new Event("change", {{ bubbles: true }}));
          }}
        }}
      }}
    }} catch (_) {{}}
  }};
  fillLoginForm();
  const fillTimer = window.setInterval(fillLoginForm, 800);
  window.setTimeout(() => window.clearInterval(fillTimer), 15000);
  try {{
    new MutationObserver(fillLoginForm).observe(document.documentElement, {{ childList: true, subtree: true }});
  }} catch (_) {{}}
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
        .map_err(|error| format!("无法打开外部链接: {error}"))
}

fn open_path_with_system(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer.exe").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(path).spawn();

    result
        .map(|_| ())
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

fn validate_external_http_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("外部链接为空，无法打开。".to_string());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("外部链接包含无效字符，无法打开。".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("只支持打开 HTTP 或 HTTPS 链接。".to_string());
    }
    Ok(trimmed)
}

fn test_station_key_connectivity_blocking(
    database: &AppDatabase,
    data_key: &[u8; 32],
    station_key_id: &str,
    model: &str,
    progress: Channel<StationKeyConnectivityTestEvent>,
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
        discover_station_key_connectivity_models(&key.station_api_base_url, &api_key)
            .unwrap_or_default();
    let requested_model = model.trim().to_string();
    let candidates = station_key_connectivity_model_candidates(
        capabilities.as_ref(),
        Some(requested_model.as_str()),
        &discovered_models,
    );
    let (model, result) = run_station_key_connectivity_model_attempts(&candidates, |candidate| {
        run_station_key_connectivity_single_model_probe(
            &upstream_api_format,
            capabilities.as_ref(),
            |kind| {
                send_station_key_connectivity_probe(
                    &key.station_api_base_url,
                    &api_key,
                    candidate,
                    kind,
                    &progress,
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
        response_mode: result.response_mode,
        stream_fallback_reason: result.stream_fallback_reason,
    })
}

fn build_station_key_connectivity_probe_url(
    base_url: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Result<String, String> {
    let path = match kind {
        StationKeyConnectivityProbeKind::Responses => "/v1/responses",
        StationKeyConnectivityProbeKind::ChatCompletions => "/v1/chat/completions",
    };
    build_api_url(base_url, path)
}

fn build_station_key_connectivity_probe_body(
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    mode: StationKeyConnectivityRequestMode,
) -> Value {
    match kind {
        StationKeyConnectivityProbeKind::Responses => json!({
            "model": model,
            "input": "hi",
            "store": false,
            "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
            "max_output_tokens": 32,
        }),
        StationKeyConnectivityProbeKind::ChatCompletions => json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": "hi",
            }],
            "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
            "max_tokens": 32,
        }),
    }
}

fn station_key_connectivity_protocol_label(kind: StationKeyConnectivityProbeKind) -> String {
    match kind {
        StationKeyConnectivityProbeKind::Responses => "responses".to_string(),
        StationKeyConnectivityProbeKind::ChatCompletions => "chat_completions".to_string(),
    }
}

fn emit_station_key_connectivity_event(
    progress: &Channel<StationKeyConnectivityTestEvent>,
    event: StationKeyConnectivityTestEvent,
) {
    let _ = progress.send(event);
}

fn redact_connectivity_error(message: &str) -> String {
    redact_error_message(&truncate_connectivity_reply(message.trim()))
}

struct StationKeyConnectivitySseDecoder {
    kind: StationKeyConnectivityProbeKind,
    pending: Vec<u8>,
    message: String,
    terminal_seen: bool,
}

impl StationKeyConnectivitySseDecoder {
    fn new(kind: StationKeyConnectivityProbeKind) -> Self {
        Self {
            kind,
            pending: Vec::new(),
            message: String::new(),
            terminal_seen: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(chunk);
        if self.pending.len() > STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT {
            return Err("SSE pending buffer too large".to_string());
        }

        let mut deltas = Vec::new();
        while let Some((boundary, separator_len)) = find_sse_event_boundary(&self.pending) {
            let event_bytes = self.pending[..boundary].to_vec();
            self.pending.drain(..boundary + separator_len);
            let event_text = std::str::from_utf8(&event_bytes)
                .map_err(|_| "SSE event contained invalid UTF-8".to_string())?;
            deltas.extend(self.consume_event(event_text)?);
        }
        Ok(deltas)
    }

    fn finish(self) -> Result<String, String> {
        if !self.pending.is_empty() {
            return Err("SSE stream ended with incomplete event".to_string());
        }
        if !self.terminal_seen {
            return Err("SSE stream ended without terminal signal".to_string());
        }
        Ok(redact_error_message(&truncate_connectivity_reply(
            &self.message,
        )))
    }

    fn consume_event(&mut self, event_text: &str) -> Result<Vec<String>, String> {
        let mut data_lines = Vec::new();
        for raw_line in event_text.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(data) = line.strip_prefix("data:") {
                data_lines.push(data.strip_prefix(' ').unwrap_or(data));
            }
        }
        if data_lines.is_empty() {
            return Ok(Vec::new());
        }
        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            self.terminal_seen = true;
            return Ok(Vec::new());
        }

        let value = serde_json::from_str::<Value>(&data)
            .map_err(|error| format!("Malformed SSE JSON: {error}"))?;
        let delta = match self.kind {
            StationKeyConnectivityProbeKind::Responses => self.consume_responses_event(&value),
            StationKeyConnectivityProbeKind::ChatCompletions => self.consume_chat_event(&value),
        };
        Ok(delta.into_iter().collect())
    }

    fn consume_responses_event(&mut self, value: &Value) -> Option<String> {
        match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                let delta = value.get("delta").and_then(Value::as_str)?;
                self.message.push_str(delta);
                Some(delta.to_string())
            }
            Some("response.completed") => {
                self.terminal_seen = true;
                None
            }
            _ => None,
        }
    }

    fn consume_chat_event(&mut self, value: &Value) -> Option<String> {
        let delta = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)?;
        self.message.push_str(delta);
        Some(delta.to_string())
    }
}

fn find_sse_event_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    for index in 0..bytes.len() {
        if bytes[index] == b'\n' && bytes.get(index + 1) == Some(&b'\n') {
            return Some((index, 2));
        }
        if bytes[index] == b'\r'
            && bytes.get(index + 1) == Some(&b'\n')
            && bytes.get(index + 2) == Some(&b'\r')
            && bytes.get(index + 3) == Some(&b'\n')
        {
            return Some((index, 4));
        }
    }
    None
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
        candidates.push(DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string());
    }
    candidates.truncate(STATION_KEY_CONNECTIVITY_CANDIDATE_LIMIT);
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
        fallback_candidates = vec![DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string()];
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
            DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string(),
            StationKeyConnectivityProbeResult::failure(0, 0, "未执行连通性探测".to_string()),
        )
    })
}

fn run_station_key_connectivity_stream_first_probe<F, E>(
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    mut send_attempt: F,
    mut emit_event: E,
) -> StationKeyConnectivityProbeResult
where
    F: FnMut(StationKeyConnectivityRequestMode) -> StationKeyConnectivityProbeResult,
    E: FnMut(StationKeyConnectivityTestEvent),
{
    emit_event(StationKeyConnectivityTestEvent::AttemptStarted {
        model: model.to_string(),
        protocol: station_key_connectivity_protocol_label(kind),
    });

    let stream_result = send_attempt(StationKeyConnectivityRequestMode::Stream);
    if stream_result.ok {
        return stream_result.with_response_mode(StationKeyConnectivityResponseMode::Stream);
    }

    let fallback_reason = redact_connectivity_error(&stream_result.message);
    emit_event(StationKeyConnectivityTestEvent::Fallback {
        reason: fallback_reason.clone(),
    });
    let fallback_result = send_attempt(StationKeyConnectivityRequestMode::NonStream);
    let duration_ms = stream_result
        .duration_ms
        .saturating_add(fallback_result.duration_ms);

    if fallback_result.ok {
        return StationKeyConnectivityProbeResult::success(
            fallback_result.status_code,
            duration_ms,
            fallback_result.message,
        )
        .with_response_mode(StationKeyConnectivityResponseMode::NonStreamFallback)
        .with_stream_fallback_reason(Some(fallback_reason));
    }

    StationKeyConnectivityProbeResult::failure(
        fallback_result.status_code,
        duration_ms,
        format!(
            "Stream: {}; Non-stream fallback: {}",
            stream_result.message, fallback_result.message
        ),
    )
    .with_response_mode(StationKeyConnectivityResponseMode::NonStreamFallback)
    .with_stream_fallback_reason(Some(fallback_reason))
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
        let mut chat_result = chat_result;
        chat_result.duration_ms = duration_ms;
        return chat_result;
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
    progress: &Channel<StationKeyConnectivityTestEvent>,
) -> StationKeyConnectivityProbeResult {
    run_station_key_connectivity_stream_first_probe(
        model,
        kind,
        |mode| match mode {
            StationKeyConnectivityRequestMode::Stream => {
                send_station_key_connectivity_stream_probe_attempt(
                    base_url, api_key, model, kind, progress,
                )
            }
            StationKeyConnectivityRequestMode::NonStream => {
                send_station_key_connectivity_non_stream_probe_attempt(
                    base_url, api_key, model, kind,
                )
            }
        },
        |event| emit_station_key_connectivity_event(progress, event),
    )
}

fn send_station_key_connectivity_non_stream_probe_attempt(
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> StationKeyConnectivityProbeResult {
    let url = match build_station_key_connectivity_probe_url(base_url, kind) {
        Ok(url) => url,
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                0,
                redact_error_message(&format!("API Base URL 无效: {error}")),
            );
        }
    };
    let body = build_station_key_connectivity_probe_body(
        model,
        kind,
        StationKeyConnectivityRequestMode::NonStream,
    );
    let started = Instant::now();
    let response_result = ureq::post(&url)
        .timeout(STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_json(body);
    let (status_code, response_text) = match response_result {
        Ok(response) => response_text_pair(response),
        Err(ureq::Error::Status(_, response)) => response_text_pair(response),
        Err(error) => {
            let duration_ms = elapsed_ms(started);
            return StationKeyConnectivityProbeResult::failure(
                0,
                duration_ms,
                redact_error_message(&format!("{error}")),
            );
        }
    };
    let duration_ms = elapsed_ms(started);
    if (200..300).contains(&status_code) {
        let message =
            extract_station_key_connectivity_reply(&response_text, kind).unwrap_or_else(|| {
                match kind {
                    StationKeyConnectivityProbeKind::Responses => "Responses 连通正常".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions => {
                        "Chat Completions 连通正常".to_string()
                    }
                }
            });
        return StationKeyConnectivityProbeResult::success(status_code, duration_ms, message);
    }
    StationKeyConnectivityProbeResult::failure(
        status_code,
        duration_ms,
        response_error_message(&response_text, status_code),
    )
}

fn send_station_key_connectivity_stream_probe_attempt(
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    progress: &Channel<StationKeyConnectivityTestEvent>,
) -> StationKeyConnectivityProbeResult {
    let url = match build_station_key_connectivity_probe_url(base_url, kind) {
        Ok(url) => url,
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                0,
                redact_error_message(&format!("API Base URL 无效: {error}")),
            );
        }
    };
    let body = build_station_key_connectivity_probe_body(
        model,
        kind,
        StationKeyConnectivityRequestMode::Stream,
    );
    let started = Instant::now();
    let response_result = ureq::post(&url)
        .timeout(STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "text/event-stream")
        .send_json(body);

    let response = match response_result {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => {
            let (status_code, response_text) = response_text_pair(response);
            return StationKeyConnectivityProbeResult::failure(
                status_code,
                elapsed_ms(started),
                response_error_message(&response_text, status_code),
            );
        }
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                elapsed_ms(started),
                redact_error_message(&format!("{error}")),
            );
        }
    };

    let status_code = response.status();
    if !(200..300).contains(&status_code) {
        let (status_code, response_text) = response_text_pair(response);
        return StationKeyConnectivityProbeResult::failure(
            status_code,
            elapsed_ms(started),
            response_error_message(&response_text, status_code),
        );
    }

    let content_type = response
        .header("content-type")
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.contains("text/event-stream") {
        let (_status_code, _response_text) = response_text_pair(response);
        return StationKeyConnectivityProbeResult::failure(
            status_code,
            elapsed_ms(started),
            redact_connectivity_error(&format!(
                "Expected text/event-stream response, got {}",
                if content_type.is_empty() {
                    "missing content-type"
                } else {
                    content_type.as_str()
                }
            )),
        );
    }

    let mut reader = response.into_reader();
    let mut decoder = StationKeyConnectivitySseDecoder::new(kind);
    let mut buffer = [0_u8; 2048];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                return StationKeyConnectivityProbeResult::failure(
                    status_code,
                    elapsed_ms(started),
                    redact_connectivity_error(&format!("Failed to read SSE stream: {error}")),
                );
            }
        };
        let deltas = match decoder.push(&buffer[..count]) {
            Ok(deltas) => deltas,
            Err(error) => {
                return StationKeyConnectivityProbeResult::failure(
                    status_code,
                    elapsed_ms(started),
                    redact_connectivity_error(&error),
                );
            }
        };
        for delta in deltas {
            emit_station_key_connectivity_event(
                progress,
                StationKeyConnectivityTestEvent::Delta { text: delta },
            );
        }
    }

    match decoder.finish() {
        Ok(message) if !message.trim().is_empty() => {
            StationKeyConnectivityProbeResult::success(status_code, elapsed_ms(started), message)
        }
        Ok(_) => StationKeyConnectivityProbeResult::success(
            status_code,
            elapsed_ms(started),
            match kind {
                StationKeyConnectivityProbeKind::Responses => {
                    "Responses streaming connected".to_string()
                }
                StationKeyConnectivityProbeKind::ChatCompletions => {
                    "Chat Completions streaming connected".to_string()
                }
            },
        ),
        Err(error) => StationKeyConnectivityProbeResult::failure(
            status_code,
            elapsed_ms(started),
            redact_connectivity_error(&error),
        ),
    }
}

fn discover_station_key_connectivity_models(base_url: &str, api_key: &str) -> Option<Vec<String>> {
    let url = build_api_url(base_url, "/v1/models").ok()?;
    let response = ureq::get(&url)
        .timeout(STATION_KEY_CONNECTIVITY_MODEL_DISCOVERY_TIMEOUT)
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

fn elapsed_ms(started: Instant) -> i64 {
    started.elapsed().as_millis().min(i64::MAX as u128) as i64
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

fn extract_station_key_connectivity_reply(
    response_text: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(response_text).ok()?;
    let reply = match kind {
        StationKeyConnectivityProbeKind::Responses => extract_responses_reply_text(&parsed),
        StationKeyConnectivityProbeKind::ChatCompletions => extract_chat_reply_text(&parsed),
    }?;
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(redact_error_message(&truncate_connectivity_reply(trimmed)))
    }
}

fn extract_responses_reply_text(value: &Value) -> Option<String> {
    value
        .get("output_text")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("output")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|item| {
                    item.get("content")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find_map(|content| {
                            content
                                .get("text")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                })
        })
}

fn extract_chat_reply_text(value: &Value) -> Option<String> {
    let message = value.pointer("/choices/0/message")?;
    message
        .get("content")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|content| {
                    content
                        .get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
        })
}

fn truncate_connectivity_reply(value: &str) -> String {
    const MAX_REPLY_CHARS: usize = 240;
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(MAX_REPLY_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
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
    fn capture_request_belongs_to_management_base_when_station_url_uses_v1() {
        assert!(capture_request_belongs_to_station(
            "https://relay.example.com",
            "https://relay.example.com/v1",
            "https://relay.example.com/api/v1/auth/login"
        ));
    }

    #[test]
    fn capture_request_rejects_other_station_origins() {
        assert!(!capture_request_belongs_to_station(
            "https://relay.example.com",
            "https://relay.example.com/v1",
            "https://other.example.com/api/v1/auth/login"
        ));
    }

    #[test]
    fn capture_accepts_configured_origins_and_rejects_lookalikes() {
        assert!(capture_request_belongs_to_station(
            "https://console.example:443",
            "https://api.example/v1",
            "https://console.example/api/user/self",
        ));
        assert!(capture_request_belongs_to_station(
            "https://console.example",
            "https://api.example/v1",
            "https://api.example/v1/models",
        ));
        assert!(!capture_request_belongs_to_station(
            "https://console.example",
            "https://api.example/v1",
            "https://console.example.evil.test/api/user/self",
        ));
    }

    #[test]
    fn captured_newapi_self_event_marks_web_authorization_candidate() {
        let input = CapturedHttpEventInput {
            station_id: "station-1".to_string(),
            source_window_id: "capture-station-1".to_string(),
            page_url: "https://relay.example/console".to_string(),
            request_url: "https://relay.example/api/user/self".to_string(),
            request_path: Some("/api/user/self".to_string()),
            method: "GET".to_string(),
            status: Some(200),
            content_type: Some("application/json".to_string()),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            response_kind: Some("json".to_string()),
            response_size: None,
            response_json: Some(json!({ "success": true, "data": { "id": 42 } })),
            response_text: None,
            error_message: None,
        };

        assert_eq!(
            web_authorization_candidate_user_id_from_input(&input).as_deref(),
            Some("42")
        );
    }

    #[test]
    fn capture_script_invokes_web_authorization_finish_after_candidate() {
        let script = capture_script("station-1", "capture-station-1", None, None);

        assert!(script.contains("finish_web_authorization_session"));
        assert!(script.contains("webAuthorizationCandidate"));
        assert!(script.contains("__relayPoolAuthorizationFinishInFlight"));
    }

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
    fn ccswitch_import_ensures_placeholder_key_before_building_deeplink() {
        let database = AppDatabase::new_in_memory_for_tests().expect("database");
        let status = ProxyStatus {
            running: true,
            lifecycle: crate::models::proxy::ProxyLifecycle::Running,
            bind_addr: "127.0.0.1".to_string(),
            port: 8787,
            started_at: None,
            last_error: None,
            active_requests: 0,
            request_count: 0,
        };

        let (_, deeplink) = prepare_ccswitch_import(&database, &status).expect("import plan");
        let persisted = database.get_local_access_key().expect("persisted key");

        assert_ne!(persisted, "sk-local-pool-change-me");
        assert!(deeplink.contains(&format!("apiKey={}", encode_query_param(&persisted))));
    }

    #[test]
    fn external_url_validation_accepts_http_urls() {
        assert_eq!(
            validate_external_http_url(" https://api.example.test/v1 "),
            Ok("https://api.example.test/v1")
        );
        assert_eq!(
            validate_external_http_url("HTTP://api.example.test"),
            Ok("HTTP://api.example.test")
        );
    }

    #[test]
    fn external_url_validation_rejects_non_http_urls() {
        let error = validate_external_http_url("ccswitch://v1/import?resource=provider")
            .expect_err("custom schemes should not be accepted by the station URL opener");

        assert!(error.contains("HTTP"));
    }

    #[test]
    fn station_key_connectivity_probe_uses_low_token_responses_request() {
        let body = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            StationKeyConnectivityRequestMode::NonStream,
        );

        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["input"], "hi");
        assert_eq!(body["store"], false);
        assert_eq!(body["max_output_tokens"], 32);
    }

    #[test]
    fn station_key_connectivity_stream_bodies_request_streaming() {
        let responses = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            StationKeyConnectivityRequestMode::Stream,
        );
        let chat = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
            StationKeyConnectivityRequestMode::Stream,
        );

        assert_eq!(responses["model"], "gpt-test");
        assert_eq!(responses["input"], "hi");
        assert_eq!(responses["stream"], true);
        assert_eq!(chat["model"], "gpt-test");
        assert_eq!(chat["messages"][0]["content"], "hi");
        assert_eq!(chat["stream"], true);
    }

    #[test]
    fn station_key_connectivity_responses_sse_decodes_split_deltas() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        assert!(decoder
            .push(br#"data: {"type":"response.output_text.delta","delta":"Hel"#)
            .unwrap()
            .is_empty());
        assert_eq!(decoder.push(br#"lo"}"#).unwrap(), Vec::<String>::new());
        assert_eq!(
            decoder
                .push(b"\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"!\"}\n\ndata: {\"type\":\"response.completed\"}\n\n")
                .unwrap(),
            vec!["Hello".to_string(), "!".to_string()]
        );
        assert_eq!(decoder.finish().unwrap(), "Hello!");
    }

    #[test]
    fn station_key_connectivity_responses_sse_accepts_done_sentinel() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let deltas = decoder
            .push(
                b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\ndata: [DONE]\n\n",
            )
            .unwrap();

        assert_eq!(deltas, vec!["Hi".to_string()]);
        assert_eq!(decoder.finish().unwrap(), "Hi");
    }

    #[test]
    fn station_key_connectivity_chat_sse_decodes_crlf_comments_and_done() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::ChatCompletions);

        let deltas = decoder
            .push(
                b": keep-alive\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
            )
            .unwrap();

        assert_eq!(deltas, vec!["Hi".to_string()]);
        assert_eq!(decoder.finish().unwrap(), "Hi");
    }

    #[test]
    fn station_key_connectivity_sse_rejects_malformed_json() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let error = decoder
            .push(b"data: {not-json}\n\n")
            .expect_err("malformed SSE JSON should fail the stream attempt");

        assert!(error.contains("SSE"));
    }

    #[test]
    fn station_key_connectivity_sse_rejects_missing_terminal_signal() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let deltas = decoder
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n")
            .unwrap();
        assert_eq!(deltas, vec!["partial".to_string()]);

        let error = decoder
            .finish()
            .expect_err("closing without response.completed should fail");

        assert!(error.contains("terminal"));
    }

    #[test]
    fn station_key_connectivity_sse_rejects_oversized_pending_data() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);
        let oversized = vec![b'a'; STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT + 1];

        let error = decoder
            .push(&oversized)
            .expect_err("oversized pending data should fail");

        assert!(error.contains("too large"));
    }

    #[test]
    fn station_key_connectivity_stream_success_does_not_retry_non_stream() {
        let mut attempted_modes = Vec::new();
        let mut events = Vec::new();

        let result = run_station_key_connectivity_stream_first_probe(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            |mode| {
                attempted_modes.push(mode);
                StationKeyConnectivityProbeResult::success(200, 15, "stream ok".to_string())
            },
            |event| events.push(event),
        );

        assert_eq!(
            attempted_modes,
            vec![StationKeyConnectivityRequestMode::Stream]
        );
        assert!(result.ok);
        assert_eq!(
            result.response_mode,
            StationKeyConnectivityResponseMode::Stream
        );
        assert_eq!(result.stream_fallback_reason, None);
        assert!(matches!(
            events.first(),
            Some(StationKeyConnectivityTestEvent::AttemptStarted { model, .. }) if model == "gpt-test"
        ));
    }

    #[test]
    fn station_key_connectivity_stream_failure_retries_once_non_stream() {
        let mut attempted_modes = Vec::new();
        let mut events = Vec::new();

        let result = run_station_key_connectivity_stream_first_probe(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            |mode| {
                attempted_modes.push(mode);
                match mode {
                    StationKeyConnectivityRequestMode::Stream => {
                        StationKeyConnectivityProbeResult::failure(
                            200,
                            9,
                            "missing terminal signal".to_string(),
                        )
                    }
                    StationKeyConnectivityRequestMode::NonStream => {
                        StationKeyConnectivityProbeResult::success(
                            200,
                            14,
                            "fallback ok".to_string(),
                        )
                    }
                }
            },
            |event| events.push(event),
        );

        assert_eq!(
            attempted_modes,
            vec![
                StationKeyConnectivityRequestMode::Stream,
                StationKeyConnectivityRequestMode::NonStream,
            ]
        );
        assert!(result.ok);
        assert_eq!(
            result.response_mode,
            StationKeyConnectivityResponseMode::NonStreamFallback
        );
        assert_eq!(
            result.stream_fallback_reason,
            Some("missing terminal signal".to_string())
        );
        assert!(events.iter().any(|event| matches!(
            event,
            StationKeyConnectivityTestEvent::Fallback { reason } if reason == "missing terminal signal"
        )));
    }

    #[test]
    fn station_key_connectivity_extracts_responses_reply_text() {
        let value = json!({
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Hi! What can I help you with?"
                }]
            }]
        });

        assert_eq!(
            extract_station_key_connectivity_reply(
                &value.to_string(),
                StationKeyConnectivityProbeKind::Responses
            ),
            Some("Hi! What can I help you with?".to_string())
        );
    }

    #[test]
    fn station_key_connectivity_extracts_chat_reply_text() {
        let value = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hi there"
                }
            }]
        });

        assert_eq!(
            extract_station_key_connectivity_reply(
                &value.to_string(),
                StationKeyConnectivityProbeKind::ChatCompletions
            ),
            Some("Hi there".to_string())
        );
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
        )
        .expect("build responses probe URL");

        assert_eq!(url, "https://relay.example/v1/responses");
    }

    #[test]
    fn station_key_connectivity_probe_uses_complete_api_namespace() {
        let url = build_station_key_connectivity_probe_url(
            "https://relay.example/api/v3",
            StationKeyConnectivityProbeKind::Responses,
        )
        .expect("build API namespace responses probe URL");

        assert_eq!(url, "https://relay.example/api/v3/responses");
    }

    #[test]
    fn station_key_connectivity_candidates_use_discovered_model_when_not_configured() {
        let discovered = vec!["claude-test".to_string()];
        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["claude-test"]);
    }

    #[test]
    fn station_key_connectivity_candidates_do_not_default_to_retired_gpt_4o_mini() {
        let candidates = station_key_connectivity_model_candidates(None, None, &[]);

        assert_eq!(candidates, vec!["gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_candidates_keep_fastest_discovered_models() {
        let discovered = vec![
            "codex-auto-review".to_string(),
            "gpt-5.4".to_string(),
            "gpt-5.5".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["codex-auto-review", "gpt-5.4"]);
    }

    #[test]
    fn station_key_connectivity_candidates_are_capped_for_interactive_tests() {
        let discovered = vec![
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
            "gpt-5.4".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini"]);
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

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini"]);
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
        assert_eq!(result.message, "Chat Completions 连通正常");
    }

    #[test]
    fn station_key_connectivity_network_error_does_not_switch_protocol() {
        let candidates = vec!["gpt-4.1-mini".to_string()];
        let mut attempted = Vec::new();

        let (_model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::Auto,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match kind {
                            StationKeyConnectivityProbeKind::Responses => {
                                StationKeyConnectivityProbeResult::failure(
                                    0,
                                    9,
                                    "Network Error".to_string(),
                                )
                            }
                            StationKeyConnectivityProbeKind::ChatCompletions => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    13,
                                    "Chat Completions 连通正常".to_string(),
                                )
                            }
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![(
                "gpt-4.1-mini".to_string(),
                StationKeyConnectivityProbeKind::Responses,
            )]
        );
        assert!(!result.ok);
        assert_eq!(result.status_code, 0);
    }

    #[test]
    fn station_key_connectivity_chat_probe_uses_low_token_request() {
        let body = build_station_key_connectivity_probe_body(
            "claude-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
            StationKeyConnectivityRequestMode::NonStream,
        );

        assert_eq!(body["model"], "claude-test");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], 32);
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
