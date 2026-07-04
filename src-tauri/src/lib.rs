mod commands;
mod models;
mod services;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let secret_manager = services::secrets::SecretManager::initialize()?;
            let database = services::database::AppDatabase::initialize(app.handle())?;
            println!(
                "Relay Pool Desktop database initialized at {}",
                database.db_path().display()
            );
            app.manage(secret_manager);
            app.manage(database);
            app.manage(services::capture::session::CaptureSessionStore::default());
            app.manage(services::proxy::runtime::ProxyRuntimeState::default());
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
            commands::get_local_access_key,
            commands::import_relay_pool_to_ccswitch,
            commands::update_settings,
            commands::get_proxy_status,
            commands::start_local_proxy,
            commands::stop_local_proxy,
            commands::restart_local_proxy,
            commands::list_request_logs,
            commands::clear_request_logs,
            commands::get_secret_migration_status,
            commands::run_secret_safety_scan,
            commands::list_station_keys,
            commands::create_station_key,
            commands::update_station_key,
            commands::update_station_key_group_binding,
            commands::delete_station_key,
            commands::reorder_station_keys,
            commands::list_key_pool_items,
            commands::reorder_key_pool,
            commands::get_station_key_capabilities,
            commands::update_station_key_capabilities,
            commands::list_model_aliases,
            commands::upsert_model_alias,
            commands::delete_model_alias,
            commands::list_station_key_health,
            commands::get_station_key_health,
            commands::simulate_route,
            commands::list_pricing_rules,
            commands::upsert_pricing_rule,
            commands::delete_pricing_rule,
            commands::list_balance_snapshots,
            commands::upsert_balance_snapshot,
            commands::list_station_group_bindings,
            commands::upsert_station_group_binding,
            commands::list_group_rate_records,
            commands::list_collector_runs,
            commands::list_change_events,
            commands::upsert_change_event,
            commands::mark_change_event_read,
            commands::dismiss_change_event,
            commands::resolve_change_event,
            commands::get_station_credentials,
            commands::update_station_credentials,
            commands::update_station_session,
            commands::clear_station_credentials,
            commands::detect_station_info,
            commands::collect_station_info,
            commands::collect_station_task,
            commands::test_station_login,
            commands::detect_sub2api_station,
            commands::collect_sub2api_station,
            commands::list_collector_snapshots,
            commands::get_latest_collector_snapshot,
            commands::start_capture_session,
            commands::get_capture_session_status,
            commands::record_capture_event,
            commands::finish_capture_session,
            commands::clear_capture_session,
            commands::close_capture_session,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Relay Pool Desktop");
}
