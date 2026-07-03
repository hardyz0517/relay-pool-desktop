use tauri::{Manager, State};

use crate::{
    models::{
        capture::{CaptureSessionStatus, CapturedHttpEventInput},
        change_events::{ChangeEvent, UpsertChangeEventInput},
        collector::{CollectorRunResult, CollectorSnapshot},
        collector_runs::CollectorRun,
        credentials::{StationCredentials, UpdateStationCredentialsInput},
        group_facts::{GroupRateRecord, StationGroupBinding, UpsertStationGroupBindingInput},
        pricing::{BalanceSnapshot, PricingRule, UpsertBalanceSnapshotInput, UpsertPricingRuleInput},
        proxy::{ProxyStatus, RequestLog},
        routing::{
            ModelAlias, RouteSimulationInput, RouteSimulationResult, StationKeyCapabilities,
            StationKeyHealth, UpdateStationKeyCapabilitiesInput, UpsertModelAliasInput,
        },
        secrets::{SecretMigrationReport, SecretScanFinding},
        settings::{AppSettings, UpdateSettingsInput},
        station_keys::KeyPoolItem,
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
        stations::{CreateStationInput, Station, UpdateStationInput},
        AppStatus,
    },
    services::{
        capture, collectors, database::AppDatabase, proxy::runtime::ProxyRuntimeState,
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
pub fn get_station_key_health(
    database: State<'_, AppDatabase>,
    station_key_id: String,
) -> Result<StationKeyHealth, String> {
    database.get_station_key_health(station_key_id)
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
