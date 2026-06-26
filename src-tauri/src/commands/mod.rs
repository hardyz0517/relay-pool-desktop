use tauri::State;

use crate::{
    models::{
        settings::{AppSettings, UpdateSettingsInput},
        stations::{CreateStationInput, Station, UpdateStationInput},
        AppStatus,
    },
    services::database::AppDatabase,
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
