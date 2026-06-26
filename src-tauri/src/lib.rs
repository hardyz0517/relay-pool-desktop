mod commands;
mod models;
mod services;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_status])
        .run(tauri::generate_context!())
        .expect("failed to run Relay Pool Desktop");
}
