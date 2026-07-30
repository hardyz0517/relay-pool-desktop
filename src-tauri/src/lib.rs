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

pub use models::secrets::{canonical_secret_aad, SecretRecordSelector, VersionedEncryptedSecret};
pub use services::data_store::installation_lease::{InstallationLease, LeaseError};
pub use services::secrets::{
    rekey::{
        BufferedSecretRekeyWriter, SecretRekeyError, SecretRekeyErrorCode, SecretRekeyPolicy,
        SecretRekeyReport, SecretRekeyRowPolicy, SecretRekeyService, SecretRekeyWriter,
    },
    DeviceKeyId, DeviceKeyResolver, SecretKeyMaterial, CURRENT_SECRET_ENCRYPTION_VERSION,
};

use crate::background_tasks::{BlockingExecutor, ExitCoordinator, ExitReason};
use services::data_store::{
    inspect_startup,
    relocation::apply_trusted_relocation,
    types::{DataStoreStartupState, RecoveryReason, StartupDecision},
};
use services::portable_migration::recovery::{
    complete_portable_activation, recover_portable_activation_for_startup,
    PortableActivationManualReason, PortableActivationStartup,
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
                if let Some(coordinator) = app.try_state::<ExitCoordinator>() {
                    coordinator.request_exit(app.clone(), ExitReason::TrayQuit, 0);
                } else {
                    eprintln!("exit coordinator unavailable for tray quit request");
                }
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
    mut startup_state: DataStoreStartupState,
    device_keys: &services::secrets::DeviceKeyResolver,
) -> Result<PreparedDataStore, String> {
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
            services::data_store::generation_upgrade::prepare_generation_two_with_resolver(
                &default_data_dir,
                &active_data_dir,
                Some(&db_path),
                device_keys,
            )
        }
        StartupDecision::FirstRun { default_data_dir } => {
            services::data_store::generation_upgrade::prepare_generation_two_with_resolver(
                &default_data_dir,
                &default_data_dir,
                None,
                device_keys,
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

fn recovery_reason_for_device_key_error(
    error: services::secrets::device_key_store::DeviceKeyErrorKind,
) -> RecoveryReason {
    use services::secrets::device_key_store::DeviceKeyErrorKind;
    match error {
        DeviceKeyErrorKind::NotFound => RecoveryReason::SystemCredentialMissing,
        DeviceKeyErrorKind::Unavailable => RecoveryReason::SystemCredentialUnavailable,
        DeviceKeyErrorKind::PermissionDenied => RecoveryReason::SystemCredentialPermissionDenied,
        DeviceKeyErrorKind::Corrupt => RecoveryReason::SystemCredentialCorrupt,
        DeviceKeyErrorKind::Unsupported => RecoveryReason::SystemCredentialUnsupported,
        DeviceKeyErrorKind::Internal => RecoveryReason::SystemCredentialInternal,
    }
}

fn startup_has_recovery_evidence(
    default_data_dir: &Path,
    startup_state: &DataStoreStartupState,
) -> bool {
    !startup_state.candidates.is_empty()
        || default_data_dir
            .join(persistence::upgrade_recovery_executor::UPGRADE_JOURNAL_FILE)
            .exists()
        || default_data_dir
            .join("portable-migration-activation-journal.json")
            .exists()
}

fn portable_recovery_reason(reason: PortableActivationManualReason) -> RecoveryReason {
    match reason {
        PortableActivationManualReason::KeyUnavailable => {
            RecoveryReason::PortableMigrationKeyUnavailable
        }
        PortableActivationManualReason::JournalMalformed
        | PortableActivationManualReason::UnsupportedJournal
        | PortableActivationManualReason::PathRejected
        | PortableActivationManualReason::MissingArtifact
        | PortableActivationManualReason::IdentityMismatch
        | PortableActivationManualReason::ReplacementFailed
        | PortableActivationManualReason::NewActiveInvalid
        | PortableActivationManualReason::RollbackFailed => {
            RecoveryReason::PortableMigrationManualRecoveryRequired
        }
    }
}

fn portable_manual_startup_state(
    default_data_dir: &Path,
    reason: RecoveryReason,
) -> DataStoreStartupState {
    let mut startup_state = inspect_startup(default_data_dir).unwrap_or_else(|_| {
        DataStoreStartupState::new(
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::Unreadable,
            },
            Vec::new(),
            default_data_dir.to_path_buf(),
            None,
        )
    });
    startup_state.decision = StartupDecision::NeedsRecovery { reason };
    startup_state
}

struct StartupSecretMaterial {
    manager: Option<services::secrets::SecretManager>,
    first_run_key_id: Option<String>,
    startup_state: DataStoreStartupState,
}

fn initialize_secret_material_for_startup(
    blocking_executor: BlockingExecutor,
    app_config_dir: &Path,
    default_data_dir: &Path,
    mut startup_state: DataStoreStartupState,
) -> Result<StartupSecretMaterial, String> {
    match startup_state.decision.clone() {
        StartupDecision::FirstRun { .. }
            if !startup_has_recovery_evidence(default_data_dir, &startup_state) =>
        {
            let key_id = services::secrets::device_key_store::DeviceKeyStore::<
                services::secrets::keychain::SystemCredentialBackend,
            >::generate_key_id();
            let candidate_identity = format!("first-run:{}", default_data_dir.display());
            let planned = services::secrets::device_key_journal::DeviceKeyJournal::new(
                services::secrets::device_key_journal::DeviceKeyJournalPhase::Planned,
                key_id.clone(),
                candidate_identity,
            );
            services::secrets::device_key_journal::write_journal(app_config_dir, &planned)
                .map_err(|error| {
                    format!("failed to write device key bootstrap journal: {error}")
                })?;
            match tauri::async_runtime::block_on(
                services::secrets::SecretManager::create_pending_for_first_run(
                    blocking_executor,
                    key_id.clone(),
                ),
            ) {
                Ok(manager) => {
                    let key_created = planned.advance(
                        services::secrets::device_key_journal::DeviceKeyJournalPhase::KeyCreated,
                    );
                    services::secrets::device_key_journal::write_journal(
                        app_config_dir,
                        &key_created,
                    )
                    .map_err(|error| {
                        format!("failed to update device key bootstrap journal: {error}")
                    })?;
                    Ok(StartupSecretMaterial {
                        manager: Some(manager),
                        first_run_key_id: Some(key_id),
                        startup_state,
                    })
                }
                Err(error) => {
                    startup_state.decision = StartupDecision::NeedsRecovery {
                        reason: recovery_reason_for_device_key_error(error.kind()),
                    };
                    Ok(StartupSecretMaterial {
                        manager: None,
                        first_run_key_id: None,
                        startup_state,
                    })
                }
            }
        }
        StartupDecision::FirstRun { .. } => {
            startup_state.decision = StartupDecision::NeedsRecovery {
                reason: RecoveryReason::SystemCredentialMissing,
            };
            Ok(StartupSecretMaterial {
                manager: None,
                first_run_key_id: None,
                startup_state,
            })
        }
        StartupDecision::Ready { .. }
        | StartupDecision::NeedsRecovery { .. }
        | StartupDecision::Conflict { .. } => {
            match tauri::async_runtime::block_on(services::secrets::SecretManager::load_existing(
                blocking_executor,
            )) {
                Ok(manager) => Ok(StartupSecretMaterial {
                    manager: Some(manager),
                    first_run_key_id: None,
                    startup_state,
                }),
                Err(error) => {
                    startup_state.decision = StartupDecision::NeedsRecovery {
                        reason: recovery_reason_for_device_key_error(error.kind()),
                    };
                    Ok(StartupSecretMaterial {
                        manager: None,
                        first_run_key_id: None,
                        startup_state,
                    })
                }
            }
        }
    }
}

fn mark_first_run_database_validated(
    app_config_dir: &Path,
) -> Result<Option<services::secrets::device_key_journal::DeviceKeyJournal>, String> {
    let Some(journal) = services::secrets::device_key_journal::read_journal(app_config_dir)
        .map_err(|error| format!("failed to read device key bootstrap journal: {error}"))?
    else {
        return Ok(None);
    };
    let database_validated = journal
        .advance(services::secrets::device_key_journal::DeviceKeyJournalPhase::DatabaseValidated);
    services::secrets::device_key_journal::write_journal(app_config_dir, &database_validated)
        .map_err(|error| format!("failed to update device key bootstrap journal: {error}"))?;
    Ok(Some(database_validated))
}

fn mark_first_run_active_committed(
    app_config_dir: &Path,
    journal: &services::secrets::device_key_journal::DeviceKeyJournal,
) -> Result<(), String> {
    let active_committed = journal
        .advance(services::secrets::device_key_journal::DeviceKeyJournalPhase::ActiveCommitted);
    services::secrets::device_key_journal::write_journal(app_config_dir, &active_committed)
        .map_err(|error| format!("failed to update device key bootstrap journal: {error}"))?;
    let completed = active_committed
        .advance(services::secrets::device_key_journal::DeviceKeyJournalPhase::Completed);
    services::secrets::device_key_journal::write_journal(app_config_dir, &completed)
        .map_err(|error| format!("failed to complete device key bootstrap journal: {error}"))?;
    services::secrets::device_key_journal::remove_journal(app_config_dir)
        .map_err(|error| format!("failed to remove device key bootstrap journal: {error}"))
}

async fn drain_application_shutdown(app: tauri::AppHandle) {
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
        if let Err(error) = drain.await {
            eprintln!("application shutdown stopped before persistence close: {error}");
            return;
        }
    }
    if let Some(work_runtime) = app.try_state::<app_composition::ManagedWorkRuntime>() {
        if let Err(error) = work_runtime
            .supervisor
            .shutdown(Duration::from_secs(10))
            .await
        {
            eprintln!("task supervisor shutdown failed: {error}");
        }
        if let Err(error) = work_runtime
            .blocking
            .shutdown(Duration::from_secs(10))
            .await
        {
            eprintln!("blocking executor shutdown failed: {error}");
        }
    }
    if let Some(owner) = app.try_state::<DataStoreRuntimeOwner>() {
        if let Err(error) = owner.shutdown().await {
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
            app.manage(ExitCoordinator::new(Duration::from_secs(45)));
            app.manage(application::data_maintenance::DataMaintenanceCoordinator::new());
            setup_tray(app)?;
            let work_runtime = app_composition::compose_work_runtime(
                app_composition::WorkRuntimeConfig::architecture_budget(),
                tauri::async_runtime::handle().inner().clone(),
            )
            .map_err(|error| format!("failed to compose work runtime: {error}"))?;
            let provider_registry = Arc::new(
                app_composition::compose_provider_registry()
                    .map_err(|error| format!("failed to compose provider registry: {error}"))?,
            );
            let blocking_executor = work_runtime.blocking.clone();
            let app_config_dir = app.path().app_config_dir().map_err(|error| {
                format!("failed to resolve application config directory: {error}")
            })?;
            let installation_lease = InstallationLease::try_acquire(&app_config_dir)
                .map_err(|error| format!("failed to acquire installation lease: {error}"))?;
            let default_data_dir = app.path().app_data_dir().map_err(|error| {
                format!("failed to resolve application data directory: {error}")
            })?;
            app.manage(application::data_migration::PortableMigrationCommandFacade::new(
                app_config_dir.clone(),
                default_data_dir.clone(),
                work_runtime.operation.clone(),
            ));
            let portable_activation = tauri::async_runtime::block_on(
                recover_portable_activation_for_startup(
                    &app_config_dir,
                    &default_data_dir,
                    blocking_executor.clone(),
                ),
            );
            let mut portable_activation_completion: Option<(String, &'static str)> = None;
            let mut pending_device_key_commit_id: Option<String> = None;
            let (mut secret_manager, first_run_key_id, startup_state) = match portable_activation {
                Ok(PortableActivationStartup::NoJournal) => {
                    let startup_state = inspect_startup(&default_data_dir)?;
                    let secret_material = initialize_secret_material_for_startup(
                        blocking_executor.clone(),
                        &app_config_dir,
                        &default_data_dir,
                        startup_state,
                    )?;
                    (
                        secret_material.manager,
                        secret_material.first_run_key_id,
                        secret_material.startup_state,
                    )
                }
                Ok(PortableActivationStartup::Activated {
                    operation_id,
                    target_key_id,
                    ..
                }) => {
                    let startup_state = inspect_startup(&default_data_dir)?;
                    let manager = tauri::async_runtime::block_on(
                        services::secrets::SecretManager::load_by_key_id(
                            blocking_executor.clone(),
                            target_key_id.clone(),
                        ),
                    )
                    .map_err(|error| {
                        format!(
                            "failed to reload portable activation target key {:?}",
                            error.kind()
                        )
                    })?;
                    pending_device_key_commit_id = Some(target_key_id);
                    portable_activation_completion = Some((operation_id, "activated"));
                    (Some(manager), None, startup_state)
                }
                Ok(PortableActivationStartup::RolledBack { operation_id }) => {
                    let startup_state = inspect_startup(&default_data_dir)?;
                    let secret_material = initialize_secret_material_for_startup(
                        blocking_executor.clone(),
                        &app_config_dir,
                        &default_data_dir,
                        startup_state,
                    )?;
                    portable_activation_completion = Some((operation_id, "rolled_back"));
                    (
                        secret_material.manager,
                        secret_material.first_run_key_id,
                        secret_material.startup_state,
                    )
                }
                Ok(PortableActivationStartup::ManualRecoveryRequired { reason, .. }) => (
                    None,
                    None,
                    portable_manual_startup_state(
                        &default_data_dir,
                        portable_recovery_reason(reason),
                    ),
                ),
                Err(error) => {
                    eprintln!("Relay Pool Desktop portable activation requires recovery: {error}");
                    (
                        None,
                        None,
                        portable_manual_startup_state(
                            &default_data_dir,
                            RecoveryReason::PortableMigrationManualRecoveryRequired,
                        ),
                    )
                }
            };
            let prepared_data_store = match secret_manager.as_ref() {
                Some(secret_manager) => prepare_data_store(
                    default_data_dir,
                    startup_state,
                    &secret_manager.resolver(),
                )?,
                None => PreparedDataStore::Recovery(startup_state),
            };
            let proxy_runtime = Arc::new(services::proxy::runtime::ProxyRuntimeState::default());
            let capture_session_store = services::capture::session::CaptureSessionStore::default();
            let runtime_owner = match prepared_data_store {
                PreparedDataStore::Ready {
                    runtime,
                    database_path,
                    mut startup_state,
                } => {
                    let Some(secret_manager) = secret_manager.take() else {
                        return Err("ready data store requires device key material".into());
                    };
                    let device_key_activation_error = if first_run_key_id.is_some() {
                        let commit_result = (|| {
                            let validated_journal =
                                mark_first_run_database_validated(&app_config_dir)?;
                            tauri::async_runtime::block_on(
                                secret_manager.commit_active(blocking_executor.clone()),
                            )
                            .map_err(|error| {
                                format!("failed to commit active device key: {:?}", error.kind())
                            })?;
                            if let Some(journal) = validated_journal.as_ref() {
                                mark_first_run_active_committed(&app_config_dir, journal)?;
                            }
                            Ok::<(), String>(())
                        })();
                        commit_result.err()
                    } else if let Some(key_id) = pending_device_key_commit_id.take() {
                        tauri::async_runtime::block_on(
                            services::secrets::SecretManager::commit_key_id(
                                blocking_executor.clone(),
                                key_id,
                            ),
                        )
                        .map_err(|error| {
                            format!("failed to commit portable activation key: {:?}", error.kind())
                        })
                        .err()
                    } else {
                        None
                    };
                    if let Some(error) = device_key_activation_error {
                        eprintln!(
                            "Relay Pool Desktop first-run device key activation requires recovery: {error}"
                        );
                        if let Err(close_error) = tauri::async_runtime::block_on(runtime.close()) {
                            eprintln!(
                                "failed to close runtime after device key activation error: {close_error}"
                            );
                        }
                        startup_state.decision = StartupDecision::NeedsRecovery {
                            reason: RecoveryReason::SystemCredentialInternal,
                        };
                        app.manage(secret_manager);
                        app.manage(startup_state);
                        DataStoreRuntimeOwner::new(None, installation_lease)
                    } else {
                        let device_keys = secret_manager.resolver();
                        app.manage(secret_manager);
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
                        let outbound_client = work_runtime.outbound.clone();
                        let supervisor_handle = work_runtime.supervisor.clone();
                        let app_services = app_composition::compose_app_services(
                            runtime.handle(),
                            device_keys.clone(),
                            active_data_dir.display().to_string(),
                            None,
                            data_directory_port,
                            blocking_executor.clone(),
                        );
                        app.state::<application::data_migration::PortableMigrationCommandFacade>()
                            .configure_ready_services(
                                app.state::<application::data_maintenance::DataMaintenanceCoordinator>()
                                    .inner()
                                    .clone(),
                                database_path.clone(),
                                device_keys.clone(),
                                Arc::clone(&runtime),
                                Some(Arc::clone(&proxy_runtime)),
                            );
                        let settings_stations_command_facade =
                            app_composition::compose_settings_stations_command_facade(
                                &app_services,
                                Arc::clone(app.state::<Arc<TrayBehaviorState>>().inner()),
                            );
                        let key_pool_command_facade =
                            app_composition::compose_key_pool_command_facade(&app_services);
                        let remote_keys_command_facade =
                            app_composition::compose_remote_keys_command_facade(
                                &app_services,
                                blocking_executor.clone(),
                                outbound_client.clone(),
                                Arc::clone(&provider_registry),
                            );
                        let routing_command_facade =
                            app_composition::compose_routing_command_facade(
                                &app_services,
                                outbound_client.clone(),
                            );
                        let request_logs_command_facade =
                            app_composition::compose_request_logs_command_facade(&app_services);
                        let channel_monitor_runner_port =
                            services::channel_monitors::v2_runner_port(
                                &app_services,
                                outbound_client.clone(),
                            );
                        let channel_monitoring_command_facade =
                            app_composition::compose_channel_monitoring_command_facade(
                                &app_services,
                                Arc::clone(&channel_monitor_runner_port),
                            );
                        let channel_status_command_facade =
                            app_composition::compose_channel_status_command_facade(&app_services);
                        let collector_metadata_command_facade =
                            app_composition::compose_collector_metadata_command_facade(
                                &app_services,
                            );
                        let station_collection_command_facade =
                            app_composition::compose_station_collection_command_facade(
                                &app_services,
                                blocking_executor.clone(),
                                outbound_client.clone(),
                                Arc::clone(&provider_registry),
                            );
                        let station_key_connectivity_command_facade =
                            app_composition::compose_station_key_connectivity_command_facade(
                                &app_services,
                                outbound_client.clone(),
                            );
                        let capture_command_facade =
                            app_composition::compose_capture_command_facade(
                                &app_services,
                                capture_session_store.clone(),
                                outbound_client.clone(),
                                Arc::clone(&provider_registry),
                            );
                        let pricing_command_facade =
                            app_composition::compose_pricing_command_facade(&app_services);
                        let change_events_command_facade =
                            app_composition::compose_change_events_command_facade(&app_services);
                        let credentials_command_facade =
                            app_composition::compose_credentials_command_facade(&app_services);
                        let data_directory_command_facade =
                            app_composition::compose_data_directory_command_facade(
                                &app_services,
                                blocking_executor.clone(),
                            );
                        let local_proxy_command_facade =
                            app_composition::compose_local_proxy_command_facade(
                                &app_services,
                                Arc::clone(&proxy_runtime),
                                device_keys.clone(),
                            );
                        tauri::async_runtime::block_on(
                            app_services.settings.repair_legacy_settings(),
                        )
                        .map_err(|error| {
                            format!("failed to repair legacy application settings: {error}")
                        })?;
                        tauri::async_runtime::block_on(
                            app_services.settings.ensure_local_access_key(),
                        )
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
                            .map_err(|error| {
                                format!("failed to load application settings: {error}")
                            })?;
                        app.state::<Arc<TrayBehaviorState>>()
                            .set(TrayBehavior::from_setting(&settings.tray_behavior));
                        if let Some((operation_id, outcome)) =
                            portable_activation_completion.take()
                        {
                            complete_portable_activation(&app_config_dir, &operation_id, outcome)
                                .map_err(|error| {
                                    format!(
                                        "failed to complete portable activation receipt: {error}"
                                    )
                                })?;
                        }
                        runtime_composition::register_work_runtime(
                            &persistence::upgrade_fault::NoUpgradeFaults,
                            app,
                            work_runtime,
                        )
                        .map_err(|error| format!("failed to register work runtime: {error}"))?;
                        let channel_monitor_runner =
                            services::channel_monitors::ChannelMonitorRunnerState::start_v2(
                                supervisor_handle.clone(),
                                channel_monitor_runner_port,
                            )
                            .map_err(|error| {
                                format!("failed to start channel monitor runner: {error}")
                            })?;
                        let station_collector_runner =
                            services::station_collectors::StationCollectorRunnerState::start_v2(
                                supervisor_handle,
                                services::station_collectors::v2_runner_port(
                                    &app_services,
                                    blocking_executor,
                                    outbound_client,
                                    provider_registry,
                                ),
                            )
                            .map_err(|error| {
                                format!("failed to start station collector runner: {error}")
                            })?;
                        println!(
                            "Relay Pool Desktop database initialized at {}",
                            database_path.display()
                        );
                        let runtime_owner = DataStoreRuntimeOwner::new(
                            Some(Arc::clone(&runtime)),
                            installation_lease,
                        );
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
                }
                PreparedDataStore::Recovery(startup_state) => {
                    println!("Relay Pool Desktop started in data recovery mode");
                    if let Some(secret_manager) = secret_manager.take() {
                        app.manage(secret_manager);
                    }
                    app.manage(startup_state);
                    DataStoreRuntimeOwner::new(None, installation_lease)
                }
            };
            app.manage(runtime_owner);
            app.manage(commands::data_store_startup::LocatedDataStoreCandidates::default());
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
                    if let Some(coordinator) = window.try_state::<ExitCoordinator>() {
                        coordinator.request_exit(
                            window.app_handle().clone(),
                            ExitReason::MainWindowClose,
                            0,
                        );
                    } else {
                        eprintln!("exit coordinator unavailable for main window close request");
                    }
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
    app.run(|app, event| match event {
        RunEvent::ExitRequested { api, code, .. } => {
            if let Some(coordinator) = app.try_state::<ExitCoordinator>() {
                coordinator.handle_exit_requested(
                    app.clone(),
                    code,
                    &api,
                    drain_application_shutdown,
                );
            }
        }
        RunEvent::Exit => {}
        _ => {}
    });
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::Duration};

    use super::{
        persistence::runtime::{PersistenceRuntime, RuntimeState},
        portable_recovery_reason, DataStoreRuntimeOwner, InstallationLease, LeaseError,
        RecoveryReason, TrayBehavior,
    };
    use crate::services::portable_migration::recovery::PortableActivationManualReason;

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

    #[tokio::test]
    async fn recovery_data_store_owner_releases_lease_without_runtime() {
        let root = tempfile::tempdir().expect("temp directory");
        let config_dir = root.path().join("config");
        let lease = InstallationLease::try_acquire(&config_dir).expect("acquire lease");
        let owner = DataStoreRuntimeOwner::new(None, lease);

        owner.shutdown().await.expect("shutdown recovery owner");

        InstallationLease::try_acquire(&config_dir)
            .expect("recovery shutdown releases lease")
            .release()
            .expect("release verification lease");
    }

    #[tokio::test]
    async fn data_store_owner_shutdown_is_idempotent_after_lease_release() {
        let root = tempfile::tempdir().expect("temp directory");
        let config_dir = root.path().join("config");
        let lease = InstallationLease::try_acquire(&config_dir).expect("acquire lease");
        let owner = DataStoreRuntimeOwner::new(None, lease);

        owner.shutdown().await.expect("first shutdown");
        owner.shutdown().await.expect("second shutdown");

        InstallationLease::try_acquire(&config_dir)
            .expect("lease remains released")
            .release()
            .expect("release verification lease");
    }

    #[test]
    fn startup_activation_recovery_reason_preserves_key_unavailable_boundary() {
        assert_eq!(
            portable_recovery_reason(PortableActivationManualReason::KeyUnavailable),
            RecoveryReason::PortableMigrationKeyUnavailable
        );
        assert_eq!(
            portable_recovery_reason(PortableActivationManualReason::IdentityMismatch),
            RecoveryReason::PortableMigrationManualRecoveryRequired
        );
    }
}
