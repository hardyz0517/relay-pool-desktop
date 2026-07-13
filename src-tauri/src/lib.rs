mod commands;
mod models;
mod services;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let secret_manager = services::secrets::SecretManager::initialize()?;
            let database = services::database::AppDatabase::initialize(app.handle())?;
            let data_key = *secret_manager.data_key();
            let channel_monitor_runner =
                services::channel_monitors::ChannelMonitorRunnerState::start(
                    database.clone(),
                    data_key,
                );
            let station_collector_runner =
                services::station_collectors::StationCollectorRunnerState::start(
                    database.clone(),
                    data_key,
                );
            println!(
                "Relay Pool Desktop database initialized at {}",
                database.db_path().display()
            );
            app.manage(secret_manager);
            app.manage(database);
            app.manage(channel_monitor_runner);
            app.manage(station_collector_runner);
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
            commands::update_local_access_key,
            commands::import_relay_pool_to_ccswitch,
            commands::open_external_url,
            commands::latest_update_manifest_version,
            commands::update_settings,
            commands::choose_data_dir,
            commands::reset_data_dir,
            commands::get_proxy_status,
            commands::load_local_routing_workspace,
            commands::reorder_local_routing_keys,
            commands::start_local_proxy,
            commands::stop_local_proxy,
            commands::cleanup_before_update,
            commands::prepare_local_proxy_for_update,
            commands::restart_local_proxy,
            commands::list_request_logs,
            commands::clear_request_logs,
            commands::get_secret_migration_status,
            commands::run_secret_safety_scan,
            commands::list_station_keys,
            commands::create_station_key,
            commands::update_station_key,
            commands::save_station_key_with_defaults,
            commands::update_station_key_group_binding,
            commands::delete_station_key,
            commands::reorder_station_keys,
            commands::get_remote_key_capability,
            commands::list_remote_station_keys,
            commands::scan_remote_station_keys,
            commands::create_remote_station_key,
            commands::create_local_station_key_from_remote,
            commands::bind_remote_station_key,
            commands::unbind_remote_station_key,
            commands::list_key_pool_items,
            commands::reorder_key_pool,
            commands::get_station_key_capabilities,
            commands::update_station_key_capabilities,
            commands::list_model_aliases,
            commands::upsert_model_alias,
            commands::delete_model_alias,
            commands::list_station_key_health,
            commands::list_station_endpoint_health,
            commands::list_channel_monitors,
            commands::list_channel_monitor_summaries,
            commands::list_channel_status_summaries,
            commands::create_channel_monitor,
            commands::update_channel_monitor,
            commands::delete_channel_monitor,
            commands::list_channel_monitor_runs,
            commands::list_channel_monitor_templates,
            commands::create_channel_monitor_template,
            commands::update_channel_monitor_template,
            commands::duplicate_channel_monitor_template,
            commands::delete_channel_monitor_template,
            commands::run_channel_monitor_now,
            commands::get_station_key_health,
            commands::ping_station_endpoint,
            commands::test_station_key_connectivity,
            commands::simulate_route,
            commands::list_pricing_rules,
            commands::list_model_base_prices,
            commands::upsert_model_base_price,
            commands::reset_model_base_prices_to_builtins,
            commands::upsert_pricing_rule,
            commands::delete_pricing_rule,
            commands::resolve_station_key_pricing_context,
            commands::list_balance_snapshots,
            commands::list_balance_snapshots_for_station,
            commands::upsert_balance_snapshot,
            commands::list_station_group_bindings,
            commands::list_station_group_options,
            commands::upsert_station_group_binding,
            commands::list_group_rate_records,
            commands::list_collector_runs,
            commands::list_change_events,
            commands::clear_change_events,
            commands::list_change_events_for_station,
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
            commands::test_station_login_input,
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
