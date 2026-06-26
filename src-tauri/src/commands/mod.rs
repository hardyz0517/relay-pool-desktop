use tauri::State;

use crate::{
    models::{
        collector::{CollectorRunResult, CollectorSnapshot},
        credentials::{StationCredentials, UpdateStationCredentialsInput},
        settings::{AppSettings, UpdateSettingsInput},
        station_keys::{CreateStationKeyInput, StationKey, UpdateStationKeyInput},
        stations::{CreateStationInput, Station, UpdateStationInput},
        AppStatus,
    },
    services::{collectors, database::AppDatabase},
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
    input: CreateStationInput,
) -> Result<Station, String> {
    database.create_station(input)
}

#[tauri::command]
pub fn update_station(
    database: State<'_, AppDatabase>,
    input: UpdateStationInput,
) -> Result<Station, String> {
    database.update_station(input)
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
pub fn list_station_keys(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<Vec<StationKey>, String> {
    database.list_station_keys(station_id)
}

#[tauri::command]
pub fn create_station_key(
    database: State<'_, AppDatabase>,
    input: CreateStationKeyInput,
) -> Result<StationKey, String> {
    database.create_station_key(input)
}

#[tauri::command]
pub fn update_station_key(
    database: State<'_, AppDatabase>,
    input: UpdateStationKeyInput,
) -> Result<StationKey, String> {
    database.update_station_key(input)
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
pub fn get_station_credentials(
    database: State<'_, AppDatabase>,
    station_id: String,
) -> Result<StationCredentials, String> {
    database.get_station_credentials(station_id)
}

#[tauri::command]
pub fn update_station_credentials(
    database: State<'_, AppDatabase>,
    input: UpdateStationCredentialsInput,
) -> Result<StationCredentials, String> {
    database.update_station_credentials(input)
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
    station_id: String,
) -> Result<CollectorRunResult, String> {
    collect_station_info(database, station_id).await
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
    station_id: String,
) -> Result<CollectorRunResult, String> {
    let database = database.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        collectors::collect_station_info(&database, station_id)
    })
    .await
    .map_err(|error| format!("采集任务执行失败: {error}"))?
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
