mod commands;
mod models;
mod services;

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use services::data_store::{
    config::{
        create_installation_marker, installation_marker_exists, read_config, write_config,
        DataDirConfigV2,
    },
    inspect_startup,
    relocation::apply_trusted_relocation,
    types::{DataStoreStartupState, RecoveryReason, StartupDecision},
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri::WindowEvent;

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";

#[derive(Debug, Clone, Copy)]
struct WindowLifecyclePolicy;

impl WindowLifecyclePolicy {
    fn hides_on_close(self) -> bool {
        true
    }

    fn hides_on_minimize(self) -> bool {
        false
    }
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "显示主窗口", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "退出", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("Relay Pool Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let menu_id = event.id();
            if menu_id.as_ref() == "show" {
                show_main_window(app);
            }
            if menu_id.as_ref() == "quit" {
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| match event {
            TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            }
            | TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } => show_main_window(tray.app_handle()),
            _ => {}
        });

    if let Some(icon) = app.default_window_icon().cloned() {
        tray = tray.icon(icon);
    }

    tray.build(app)?;
    Ok(())
}

enum PreparedDataStore {
    Ready(services::database::AppDatabase, DataStoreStartupState),
    Recovery(DataStoreStartupState),
}

fn prepare_data_store(default_data_dir: PathBuf) -> Result<PreparedDataStore, String> {
    let mut startup_state = inspect_startup(&default_data_dir)?;
    if let Some(intent) = startup_state.relocation_intent.clone() {
        match apply_trusted_relocation(&default_data_dir, &intent) {
            Ok(_) => {
                startup_state = inspect_startup(&default_data_dir)?;
            }
            Err(error) => {
                eprintln!(
                    "Relay Pool Desktop data directory relocation requires recovery: {error}"
                );
                startup_state.decision = StartupDecision::NeedsRecovery {
                    reason: RecoveryReason::PendingRelocation,
                };
                return Ok(PreparedDataStore::Recovery(startup_state));
            }
        }
    }
    let startup_default_data_dir = startup_state.default_data_dir().to_path_buf();
    let database = match startup_state.decision.clone() {
        StartupDecision::Ready { candidate_id } => {
            let Some(candidate) = startup_state
                .candidates
                .iter()
                .find(|candidate| candidate.id == candidate_id)
            else {
                startup_state.decision = StartupDecision::NeedsRecovery {
                    reason: RecoveryReason::Missing,
                };
                return Ok(PreparedDataStore::Recovery(startup_state));
            };
            let db_path = PathBuf::from(&candidate.path);
            let Some(active_data_dir) = db_path.parent().map(Path::to_path_buf) else {
                startup_state.decision = StartupDecision::NeedsRecovery {
                    reason: RecoveryReason::Missing,
                };
                return Ok(PreparedDataStore::Recovery(startup_state));
            };
            services::database::AppDatabase::initialize_existing_at(
                default_data_dir.clone(),
                active_data_dir.clone(),
                None,
            )
            .and_then(|database| {
                commit_active_selection_after_success(&default_data_dir, &active_data_dir)?;
                Ok(database)
            })
        }
        StartupDecision::FirstRun { default_data_dir } => {
            services::database::AppDatabase::initialize_new_at(
                default_data_dir.clone(),
                default_data_dir.clone(),
            )
            .and_then(|database| {
                commit_active_selection_after_success(&default_data_dir, &default_data_dir)?;
                Ok(database)
            })
        }
        StartupDecision::NeedsRecovery { .. } | StartupDecision::Conflict { .. } => {
            return Ok(PreparedDataStore::Recovery(startup_state));
        }
    };

    match database {
        Ok(database) => {
            let mut ready_state = inspect_startup(&startup_default_data_dir).map_err(|error| {
                format!("failed to verify data store startup after database open: {error}")
            })?;
            if matches!(ready_state.decision, StartupDecision::Ready { .. }) {
                Ok(PreparedDataStore::Ready(database, ready_state))
            } else {
                ready_state.decision = StartupDecision::NeedsRecovery {
                    reason: RecoveryReason::OpenOrMigrationFailed,
                };
                Ok(PreparedDataStore::Recovery(ready_state))
            }
        }
        Err(error) => {
            eprintln!("Relay Pool Desktop database startup requires recovery: {error}");
            startup_state.decision = StartupDecision::NeedsRecovery {
                reason: RecoveryReason::OpenOrMigrationFailed,
            };
            Ok(PreparedDataStore::Recovery(startup_state))
        }
    }
}

fn commit_active_selection_after_success(
    default_data_dir: &Path,
    active_data_dir: &Path,
) -> Result<(), String> {
    let config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
    let config = read_config(&config_path)?;
    if config
        .as_ref()
        .and_then(|config| config.active_data_dir.as_deref())
        == Some(active_data_dir)
        && installation_marker_exists(default_data_dir)
    {
        return Ok(());
    }
    write_config(
        &config_path,
        &DataDirConfigV2 {
            version: 2,
            active_data_dir: Some(active_data_dir.to_path_buf()),
            pending_data_dir: None,
            source_data_dir: None,
            updated_at: data_store_updated_at(),
        },
    )?;
    create_installation_marker(default_data_dir)
}

fn data_store_updated_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            setup_tray(app)?;
            let secret_manager = services::secrets::SecretManager::initialize()?;
            let default_data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| format!("无法解析应用数据目录: {error}"))?;
            let prepared_data_store = prepare_data_store(default_data_dir)?;
            app.manage(secret_manager);
            match prepared_data_store {
                PreparedDataStore::Ready(database, startup_state) => {
                    let data_key = *app.state::<services::secrets::SecretManager>().data_key();
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
                    app.manage(startup_state);
                    app.manage(database);
                    app.manage(channel_monitor_runner);
                    app.manage(station_collector_runner);
                }
                PreparedDataStore::Recovery(startup_state) => {
                    println!("Relay Pool Desktop started in data recovery mode");
                    app.manage(startup_state);
                }
            }
            app.manage(services::capture::session::CaptureSessionStore::default());
            app.manage(services::proxy::runtime::ProxyRuntimeState::default());
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            let behavior = WindowLifecyclePolicy;
            match event {
                WindowEvent::CloseRequested { api, .. } if behavior.hides_on_close() => {
                    api.prevent_close();
                    let _ = window.hide();
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    window.app_handle().exit(0);
                }
                WindowEvent::Resized(_) if behavior.hides_on_minimize() => {
                    if window.is_minimized().unwrap_or(false) {
                        let _ = window.hide();
                    }
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_status,
            commands::get_data_store_startup_state,
            commands::refresh_data_store_candidates,
            commands::locate_data_store_candidate,
            commands::activate_data_store_candidate,
            commands::create_new_data_store,
            commands::open_data_store_backup_dir,
            commands::export_data_store_diagnostic,
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
            commands::updater_network_config,
            commands::inspect_latest_update_manifest,
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
            commands::load_channel_status_workspace,
            commands::load_pricing_comparison_workspace,
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
            commands::list_current_station_balance_snapshots,
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
            commands::mark_change_events_read,
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
            commands::finish_web_authorization_session,
            commands::clear_capture_session,
            commands::close_capture_session,
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Relay Pool Desktop");
}

#[cfg(test)]
mod tests {
    use super::WindowLifecyclePolicy;

    #[test]
    fn fixed_window_policy_hides_only_on_close() {
        let behavior = WindowLifecyclePolicy;
        assert!(behavior.hides_on_close());
        assert!(!behavior.hides_on_minimize());
    }
}
