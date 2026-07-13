mod commands;
mod models;
mod services;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::Manager;
use tauri::WindowEvent;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrayBehavior {
    MinimizeToTray,
    CloseToTray,
    Disabled,
}

impl TrayBehavior {
    fn from_setting(value: &str) -> Self {
        match value {
            "close-to-tray" => Self::CloseToTray,
            "disabled" => Self::Disabled,
            _ => Self::MinimizeToTray,
        }
    }

    fn hides_on_close(self) -> bool {
        matches!(self, Self::CloseToTray)
    }

    fn hides_on_minimize(self) -> bool {
        matches!(self, Self::MinimizeToTray)
    }
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn current_tray_behavior<R: tauri::Runtime, M: Manager<R>>(manager: &M) -> TrayBehavior {
    manager
        .app_handle()
        .try_state::<services::database::AppDatabase>()
        .and_then(|database| database.get_settings().ok())
        .map(|settings| TrayBehavior::from_setting(&settings.tray_behavior))
        .unwrap_or(TrayBehavior::MinimizeToTray)
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
        .on_window_event(|window, event| {
            if window.label() != "main" {
                return;
            }

            let behavior = current_tray_behavior(window);
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
    use super::TrayBehavior;

    #[test]
    fn tray_behavior_maps_persisted_values() {
        assert_eq!(
            TrayBehavior::from_setting("minimize-to-tray"),
            TrayBehavior::MinimizeToTray
        );
        assert_eq!(
            TrayBehavior::from_setting("close-to-tray"),
            TrayBehavior::CloseToTray
        );
        assert_eq!(
            TrayBehavior::from_setting("disabled"),
            TrayBehavior::Disabled
        );
        assert_eq!(
            TrayBehavior::from_setting("unexpected"),
            TrayBehavior::MinimizeToTray
        );
    }

    #[test]
    fn tray_behavior_keeps_close_and_minimize_modes_separate() {
        assert!(TrayBehavior::CloseToTray.hides_on_close());
        assert!(!TrayBehavior::CloseToTray.hides_on_minimize());

        assert!(TrayBehavior::MinimizeToTray.hides_on_minimize());
        assert!(!TrayBehavior::MinimizeToTray.hides_on_close());

        assert!(!TrayBehavior::Disabled.hides_on_close());
        assert!(!TrayBehavior::Disabled.hides_on_minimize());
    }
}
