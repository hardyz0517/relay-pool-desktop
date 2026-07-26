mod app_composition;
mod application;
pub mod background_tasks;
mod commands;
mod ipc;
mod models;
mod observability;
pub mod outbound;
mod persistence;
mod runtime_composition;
mod services;

use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, Mutex,
};
use std::time::Duration;

pub use services::data_store::installation_lease::{InstallationLease, LeaseError};

use services::data_store::{
    inspect_startup,
    relocation::apply_trusted_relocation,
    types::{DataStoreStartupState, RecoveryReason, StartupDecision},
};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};

macro_rules! tauri_handler_from_registry {
    ($( $name:ident => $handler:path, )*) => {
        tauri::generate_handler![$($handler),*]
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrayBehavior {
    MinimizeToTray,
    CloseToTray,
    Disabled,
}

pub(crate) struct TrayBehaviorState(AtomicU8);

impl Default for TrayBehaviorState {
    fn default() -> Self {
        Self(AtomicU8::new(1))
    }
}

impl TrayBehaviorState {
    pub(crate) fn get(&self) -> TrayBehavior {
        match self.0.load(Ordering::Relaxed) {
            0 => TrayBehavior::MinimizeToTray,
            2 => TrayBehavior::Disabled,
            _ => TrayBehavior::CloseToTray,
        }
    }

    pub(crate) fn set(&self, behavior: TrayBehavior) {
        let value = match behavior {
            TrayBehavior::MinimizeToTray => 0,
            TrayBehavior::CloseToTray => 1,
            TrayBehavior::Disabled => 2,
        };
        self.0.store(value, Ordering::Relaxed);
    }
}

impl TrayBehavior {
    pub(crate) fn from_setting(value: &str) -> Self {
        match value {
            "minimize_to_tray" => Self::MinimizeToTray,
            "disabled" => Self::Disabled,
            _ => Self::CloseToTray,
        }
    }

    fn hides_on_close(self) -> bool {
        matches!(self, Self::CloseToTray)
    }

    fn hides_on_minimize(self) -> bool {
        matches!(self, Self::MinimizeToTray)
    }
}

fn current_tray_behavior<R: tauri::Runtime>(window: &tauri::Window<R>) -> TrayBehavior {
    window
        .try_state::<Arc<TrayBehaviorState>>()
        .map(|state| state.get())
        .unwrap_or(TrayBehavior::CloseToTray)
}

fn show_main_window<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
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
    Ready {
        runtime: Arc<persistence::runtime::PersistenceRuntime>,
        database_path: PathBuf,
        startup_state: DataStoreStartupState,
    },
    Recovery(DataStoreStartupState),
}

struct DataStoreRuntimeOwner {
    runtime: Option<Arc<persistence::runtime::PersistenceRuntime>>,
    installation_lease: Mutex<Option<InstallationLease>>,
}

impl DataStoreRuntimeOwner {
    fn new(
        runtime: Option<Arc<persistence::runtime::PersistenceRuntime>>,
        installation_lease: InstallationLease,
    ) -> Self {
        Self {
            runtime,
            installation_lease: Mutex::new(Some(installation_lease)),
        }
    }

    async fn shutdown(&self) -> Result<(), DataStoreShutdownError> {
        let runtime_result = match &self.runtime {
            Some(runtime) => runtime.close().await,
            None => Ok(()),
        };
        let lease = self
            .installation_lease
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take();
        let lease_result = match lease {
            Some(lease) => lease.release(),
            None => Ok(()),
        };

        match (runtime_result, lease_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(runtime), Ok(())) => Err(DataStoreShutdownError::Runtime(runtime)),
            (Ok(()), Err(lease)) => Err(DataStoreShutdownError::Lease(lease)),
            (Err(runtime), Err(lease)) => {
                Err(DataStoreShutdownError::RuntimeAndLease { runtime, lease })
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum DataStoreShutdownError {
    #[error("persistence runtime shutdown failed")]
    Runtime(#[source] persistence::runtime::RuntimeTransitionError),
    #[error("installation lease release failed")]
    Lease(#[source] LeaseError),
    #[error("persistence runtime shutdown and installation lease release failed")]
    RuntimeAndLease {
        #[source]
        runtime: persistence::runtime::RuntimeTransitionError,
        lease: LeaseError,
    },
}

fn prepare_data_store(
    default_data_dir: PathBuf,
    data_key: [u8; 32],
) -> Result<PreparedDataStore, String> {
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
    let persistence = match startup_state.decision.clone() {
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
            services::data_store::generation_upgrade::prepare_generation_two(
                &default_data_dir,
                &active_data_dir,
                Some(&db_path),
                data_key,
            )
        }
        StartupDecision::FirstRun { default_data_dir } => {
            services::data_store::generation_upgrade::prepare_generation_two(
                &default_data_dir,
                &default_data_dir,
                None,
                data_key,
            )
        }
        StartupDecision::NeedsRecovery { .. } | StartupDecision::Conflict { .. } => {
            return Ok(PreparedDataStore::Recovery(startup_state));
        }
    };

    match persistence {
        Ok((runtime, database_path)) => {
            let mut ready_state = inspect_startup(&startup_default_data_dir).map_err(|error| {
                format!("failed to verify data store startup after database open: {error}")
            })?;
            if matches!(ready_state.decision, StartupDecision::Ready { .. }) {
                Ok(PreparedDataStore::Ready {
                    runtime: Arc::new(runtime),
                    database_path,
                    startup_state: ready_state,
                })
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

fn shutdown_application(app: &tauri::AppHandle) {
    if let Some(runner) = app.try_state::<services::channel_monitors::ChannelMonitorRunnerState>() {
        runner.stop();
    }
    if let Some(runner) =
        app.try_state::<services::station_collectors::StationCollectorRunnerState>()
    {
        runner.stop();
    }
    if let Some(proxy) = app.try_state::<Arc<services::proxy::runtime::ProxyRuntimeState>>() {
        let drain = runtime_composition::drain_finalization(
            &persistence::upgrade_fault::NoUpgradeFaults,
            async {
                proxy
                    .prepare_for_update(Duration::from_secs(30))
                    .await
                    .map(|_| ())
                    .map_err(|_| ())
            },
        );
        if let Err(error) = tauri::async_runtime::block_on(drain) {
            eprintln!("application shutdown stopped before persistence close: {error}");
            return;
        }
    }
    if let Some(owner) = app.try_state::<DataStoreRuntimeOwner>() {
        if let Err(error) = tauri::async_runtime::block_on(owner.shutdown()) {
            eprintln!("data store shutdown failed: {error}");
        }
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app.manage(Arc::new(TrayBehaviorState::default()));
            setup_tray(app)?;
            let secret_manager = services::secrets::SecretManager::initialize()?;
            let app_config_dir = app.path().app_config_dir().map_err(|error| {
                format!("failed to resolve application config directory: {error}")
            })?;
            let installation_lease = InstallationLease::try_acquire(&app_config_dir)
                .map_err(|error| format!("failed to acquire installation lease: {error}"))?;
            let default_data_dir = app.path().app_data_dir().map_err(|error| {
                format!("failed to resolve application data directory: {error}")
            })?;
            let prepared_data_store =
                prepare_data_store(default_data_dir, *secret_manager.data_key())?;
            app.manage(secret_manager);
            let proxy_runtime = Arc::new(services::proxy::runtime::ProxyRuntimeState::default());
            let capture_session_store = services::capture::session::CaptureSessionStore::default();
            let runtime_owner = match prepared_data_store {
                PreparedDataStore::Ready {
                    runtime,
                    database_path,
                    startup_state,
                } => {
                    let data_key = *app.state::<services::secrets::SecretManager>().data_key();
                    let active_data_dir = database_path
                        .parent()
                        .ok_or_else(|| {
                            format!(
                                "generation 2 database has no parent directory: {}",
                                database_path.display()
                            )
                        })?
                        .to_path_buf();
                    let data_directory_port = Arc::new(
                        services::data_store::data_directory_port::FileDataDirectoryPort::new(
                            startup_state.default_data_dir().to_path_buf(),
                            active_data_dir.clone(),
                        ),
                    );
                    let app_services = app_composition::compose_app_services(
                        runtime.handle(),
                        data_key,
                        active_data_dir.display().to_string(),
                        None,
                        data_directory_port,
                    );
                    let settings_stations_command_facade =
                        app_composition::compose_settings_stations_command_facade(
                            &app_services,
                            Arc::clone(app.state::<Arc<TrayBehaviorState>>().inner()),
                        );
                    let key_pool_command_facade =
                        app_composition::compose_key_pool_command_facade(&app_services);
                    let remote_keys_command_facade =
                        app_composition::compose_remote_keys_command_facade(&app_services);
                    let routing_command_facade =
                        app_composition::compose_routing_command_facade(&app_services);
                    let request_logs_command_facade =
                        app_composition::compose_request_logs_command_facade(&app_services);
                    let channel_monitor_runner_port =
                        services::channel_monitors::v2_runner_port(&app_services);
                    let channel_monitoring_command_facade =
                        app_composition::compose_channel_monitoring_command_facade(
                            &app_services,
                            Arc::clone(&channel_monitor_runner_port),
                        );
                    let channel_status_command_facade =
                        app_composition::compose_channel_status_command_facade(&app_services);
                    let collector_metadata_command_facade =
                        app_composition::compose_collector_metadata_command_facade(&app_services);
                    let station_collection_command_facade =
                        app_composition::compose_station_collection_command_facade(
                            &app_services,
                            data_key,
                        );
                    let station_key_connectivity_command_facade =
                        app_composition::compose_station_key_connectivity_command_facade(
                            &app_services,
                        );
                    let capture_command_facade = app_composition::compose_capture_command_facade(
                        &app_services,
                        capture_session_store.clone(),
                    );
                    let pricing_command_facade =
                        app_composition::compose_pricing_command_facade(&app_services);
                    let change_events_command_facade =
                        app_composition::compose_change_events_command_facade(&app_services);
                    let credentials_command_facade =
                        app_composition::compose_credentials_command_facade(&app_services);
                    let data_directory_command_facade =
                        app_composition::compose_data_directory_command_facade(&app_services);
                    let local_proxy_command_facade =
                        app_composition::compose_local_proxy_command_facade(
                            &app_services,
                            Arc::clone(&proxy_runtime),
                            data_key,
                        );
                    tauri::async_runtime::block_on(app_services.settings.repair_legacy_settings())
                        .map_err(|error| {
                            format!("failed to repair legacy application settings: {error}")
                        })?;
                    tauri::async_runtime::block_on(app_services.settings.ensure_local_access_key())
                        .map_err(|error| {
                            format!("failed to initialize the local proxy access key: {error}")
                        })?;
                    tauri::async_runtime::block_on(
                        app_services.pricing.ensure_builtin_model_base_prices(),
                    )
                    .map_err(|error| {
                        format!("failed to initialize built-in model prices: {error}")
                    })?;
                    let settings = tauri::async_runtime::block_on(app_services.settings.load())
                        .map_err(|error| format!("failed to load application settings: {error}"))?;
                    app.state::<Arc<TrayBehaviorState>>()
                        .set(TrayBehavior::from_setting(&settings.tray_behavior));
                    let work_runtime = app_composition::compose_work_runtime(
                        app_composition::WorkRuntimeConfig::architecture_budget(),
                    )
                    .map_err(|error| format!("failed to compose work runtime: {error}"))?;
                    let supervisor_handle = work_runtime.supervisor.clone();
                    runtime_composition::register_work_runtime(
                        &persistence::upgrade_fault::NoUpgradeFaults,
                        app,
                        work_runtime,
                    )
                    .map_err(|error| format!("failed to register work runtime: {error}"))?;
                    let channel_monitor_runner =
                        services::channel_monitors::ChannelMonitorRunnerState::start_v2(
                            channel_monitor_runner_port,
                        );
                    let station_collector_runner =
                        services::station_collectors::StationCollectorRunnerState::start_v2(
                            supervisor_handle,
                            services::station_collectors::v2_runner_port(&app_services, data_key),
                        )
                        .map_err(|error| {
                            format!("failed to start station collector runner: {error}")
                        })?;
                    println!(
                        "Relay Pool Desktop database initialized at {}",
                        database_path.display()
                    );
                    let runtime_owner =
                        DataStoreRuntimeOwner::new(Some(Arc::clone(&runtime)), installation_lease);
                    runtime_composition::register_ready_services_with_command_facades(
                        &persistence::upgrade_fault::NoUpgradeFaults,
                        app,
                        runtime_composition::ReadyServiceBundleWithCommandFacades::new(
                            startup_state,
                            runtime,
                            app_services,
                            settings_stations_command_facade,
                            key_pool_command_facade,
                            remote_keys_command_facade,
                            routing_command_facade,
                            request_logs_command_facade,
                            channel_monitoring_command_facade,
                            channel_status_command_facade,
                            collector_metadata_command_facade,
                            station_collection_command_facade,
                            station_key_connectivity_command_facade,
                            capture_command_facade,
                            pricing_command_facade,
                            change_events_command_facade,
                            credentials_command_facade,
                            data_directory_command_facade,
                            local_proxy_command_facade,
                            channel_monitor_runner,
                            station_collector_runner,
                        ),
                    )
                    .map_err(|error| {
                        format!("failed to register ready runtime services: {error}")
                    })?;
                    runtime_owner
                }
                PreparedDataStore::Recovery(startup_state) => {
                    println!("Relay Pool Desktop started in data recovery mode");
                    app.manage(startup_state);
                    DataStoreRuntimeOwner::new(None, installation_lease)
                }
            };
            app.manage(runtime_owner);
            app.manage(commands::LocatedDataStoreCandidates::default());
            app.manage(capture_session_store);
            app.manage(proxy_runtime);
            services::proxy::startup_auto_start::schedule(app.handle().clone());
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
                WindowEvent::Resized(_)
                    if behavior.hides_on_minimize() && window.is_minimized().unwrap_or(false) =>
                {
                    let _ = window.hide();
                }
                _ => {}
            }
        })
        // commands::cleanup_before_update, commands::prepare_local_proxy_for_update,
        // commands::restart_local_proxy, commands::updater_network_config, and
        // commands::inspect_latest_update_manifest stay registered through the generated IPC registry.
        .invoke_handler(crate::ipc_command_registry!(tauri_handler_from_registry))
        .build(tauri::generate_context!())
        .expect("failed to build Relay Pool Desktop");
    app.run(|app, event| {
        if matches!(event, RunEvent::Exit) {
            shutdown_application(app);
        }
    });
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::{
        persistence::runtime::{PersistenceRuntime, RuntimeState},
        DataStoreRuntimeOwner, InstallationLease, LeaseError, TrayBehavior,
    };

    #[test]
    fn tray_behavior_maps_window_lifecycle_modes() {
        assert!(TrayBehavior::CloseToTray.hides_on_close());
        assert!(!TrayBehavior::CloseToTray.hides_on_minimize());

        assert!(!TrayBehavior::MinimizeToTray.hides_on_close());
        assert!(TrayBehavior::MinimizeToTray.hides_on_minimize());

        assert!(!TrayBehavior::Disabled.hides_on_close());
        assert!(!TrayBehavior::Disabled.hides_on_minimize());
    }

    #[tokio::test]
    async fn data_store_owner_releases_lease_only_after_runtime_drain() {
        let root = tempfile::tempdir().expect("temp directory");
        let config_dir = root.path().join("config");
        let database_path = root.path().join("runtime.sqlite3");
        let lease = InstallationLease::try_acquire(&config_dir).expect("acquire lease");
        let runtime = Arc::new(
            PersistenceRuntime::initialize_new(&database_path)
                .await
                .expect("initialize runtime"),
        );
        let read = runtime.begin_read().await.expect("begin read");
        let owner = Arc::new(DataStoreRuntimeOwner::new(
            Some(Arc::clone(&runtime)),
            lease,
        ));
        let closing_owner = Arc::clone(&owner);
        let closing = tokio::spawn(async move { closing_owner.shutdown().await });

        for _ in 0..100 {
            if runtime.state() == RuntimeState::Draining {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(runtime.state(), RuntimeState::Draining);
        assert!(matches!(
            InstallationLease::try_acquire(&config_dir),
            Err(LeaseError::AlreadyRunning)
        ));

        drop(read);
        closing
            .await
            .expect("shutdown task")
            .expect("shutdown owner");
        assert_eq!(runtime.state(), RuntimeState::Closed);
        InstallationLease::try_acquire(&config_dir)
            .expect("lease released after pool close")
            .release()
            .expect("release verification lease");
    }
}
