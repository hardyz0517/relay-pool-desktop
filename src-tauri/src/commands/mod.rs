use crate::models::AppStatus;

#[tauri::command]
pub fn app_status() -> AppStatus {
    AppStatus::default()
}
