mod commands;
mod models;
mod services;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let database = services::database::AppDatabase::initialize(app.handle())?;
            println!(
                "Relay Pool Desktop database initialized at {}",
                database.db_path().display()
            );
            app.manage(database);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::list_stations,
            commands::create_station,
            commands::update_station,
            commands::delete_station,
            commands::reorder_stations,
            commands::get_settings,
            commands::update_settings,
            commands::list_station_keys,
            commands::create_station_key,
            commands::update_station_key,
            commands::delete_station_key,
            commands::reorder_station_keys,
            commands::get_station_credentials,
            commands::update_station_credentials,
            commands::clear_station_credentials,
            commands::detect_station_info,
            commands::collect_station_info,
            commands::detect_sub2api_station,
            commands::collect_sub2api_station,
            commands::list_collector_snapshots,
            commands::get_latest_collector_snapshot,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Relay Pool Desktop");
}
