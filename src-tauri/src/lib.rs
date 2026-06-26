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
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Relay Pool Desktop");
}
