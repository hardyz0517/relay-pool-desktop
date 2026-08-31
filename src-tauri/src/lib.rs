#![recursion_limit = "512"]

mod app_composition;
pub(crate) mod app_runtime_events;
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
#[cfg(debug_assertions)]
pub mod test_support;

use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use std::{
    num::NonZeroUsize,
    path::{Path, PathBuf},
};

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
    startup_probe::probe_upgrade_state_with_journal,
    startup_upgrade_plan::{plan_upgrade, StartupUpgradePlan},
    types::{DataStoreStartupState, RecoveryReason, StartupDecision, StartupUpgradeStage},
};
use services::portable_migration::recovery::{
    complete_portable_activation, recover_portable_activation_for_startup,
    PortableActivationManualReason, PortableActivationStartup,
};
#[cfg(feature = "tray")]
use tauri::menu::{Menu, MenuItem};
#[cfg(feature = "tray")]
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, RunEvent, WindowEvent};

/// Resolve the two application-owned roots used during startup.
///
/// The smoke override is deliberately compiled only into a debug build with
/// an explicit feature. Production binaries always use Tauri's KnownFolder
/// resolver, even when an attacker or a stale test harness sets the override
/// environment variable.
fn resolve_application_directories<R: tauri::Runtime>(
    app: &tauri::App<R>,
) -> Result<(PathBuf, PathBuf), String> {
    #[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
    if let Some(root) = std::env::var_os("RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err("runtime logging smoke root must be absolute".to_string());
        }

        let config_dir = root.join("config");
        let data_dir = root.join("data");
        std::fs::create_dir_all(&config_dir)
            .map_err(|error| format!("failed to create smoke config directory: {error}"))?;
        std::fs::create_dir_all(&data_dir)
            .map_err(|error| format!("failed to create smoke data directory: {error}"))?;
        return Ok((config_dir, data_dir));
    }

    let app_config_dir = app
        .path()
        .app_config_dir()
        .map_err(|error| format!("failed to resolve application config directory: {error}"))?;
    let default_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("failed to resolve application data directory: {error}"))?;
    Ok((app_config_dir, default_data_dir))
}

#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
fn schedule_runtime_logging_smoke_exit<R: tauri::Runtime>(app: &tauri::App<R>) {
    if std::env::var("RELAY_POOL_RUNTIME_LOGGING_SMOKE_EXIT")
        .ok()
        .as_deref()
        != Some("1")
    {
        return;
    }

    let handle = app.handle().clone();
    tauri::async_runtime::spawn(async move {
        // Let the setup callback finish and the run loop deliver a clean
        // ExitRequested event so the normal drain/marker path is exercised.
        tokio::time::sleep(Duration::from_millis(750)).await;
        let _ = mark_runtime_logging_smoke_state("complete");
        handle.exit(0);
    });
}

#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
fn runtime_logging_smoke_state_path() -> Result<PathBuf, String> {
    let root = std::env::var_os("RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT")
        .ok_or_else(|| "runtime logging smoke root is not configured".to_string())?;
    let root = PathBuf::from(root);
    std::fs::create_dir_all(&root)
        .map_err(|error| format!("failed to create runtime logging smoke root: {error}"))?;
    Ok(root.join("runtime-logging-smoke-restart.state"))
}

#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
fn read_runtime_logging_smoke_boot() -> Result<u8, String> {
    let path = runtime_logging_smoke_state_path()?;
    let state = match std::fs::read_to_string(&path) {
        Ok(state) => state,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(1),
        Err(error) => {
            return Err(format!(
                "failed to read runtime logging smoke restart state: {error}"
            ))
        }
    };
    match state.trim() {
        "restart-requested" => Ok(2),
        "complete" => Err("runtime logging smoke restarted more than once".to_string()),
        other => Err(format!(
            "invalid runtime logging smoke restart state: {other}"
        )),
    }
}

#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
fn mark_runtime_logging_smoke_state(state: &str) -> Result<(), String> {
    let path = runtime_logging_smoke_state_path()?;
    std::fs::write(&path, format!("{state}\n"))
        .map_err(|error| format!("failed to write runtime logging smoke restart state: {error}"))
}

#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
fn run_runtime_logging_smoke_probe<R: tauri::Runtime>(app: &tauri::App<R>) -> Result<bool, String> {
    let smoke_fault = std::env::var("RELAY_POOL_RUNTIME_LOGGING_SMOKE_FAULT")
        .ok()
        .filter(|value| value == "panic" || value == "marker-io");
    let settings = app
        .try_state::<application::command_facades::SettingsStationsCommandFacade>()
        .ok_or_else(|| "runtime logging smoke missing settings facade".to_string())?;
    let runtime_log = app
        .try_state::<Arc<observability::runtime::RuntimeLogService>>()
        .ok_or_else(|| "runtime logging smoke missing runtime log".to_string())?;
    let registry = app
        .try_state::<ipc::dto::runtime_context::RuntimeContextRegistry>()
        .ok_or_else(|| "runtime logging smoke missing runtime context registry".to_string())?;

    // A small smoke-only segment limit makes this process exercise rotation
    // and the same reader/export code paths without creating a large fixture.
    for _ in 0..96 {
        runtime_log.record_descriptor(
            app_runtime_events::bootstrap_started(),
            observability::runtime::EventOutcome::Ok,
            observability::runtime::RuntimeDetail::Phase {
                phase: observability::runtime::RuntimePhase::Startup,
            },
        );
        runtime_log.flush();
    }
    if smoke_fault.as_deref() == Some("panic") {
        // This branch is compiled only into the isolated debug smoke binary.
        // The panic payload is intentionally a canary: the installed crash
        // hook must never expose it on stderr or in the marker.
        panic!("runtime logging smoke panic canary: authorization=sk-smoke-secret");
    }
    let boot_index = read_runtime_logging_smoke_boot()?;
    let bundle_name = format!("runtime-support-bundle-{boot_index}");
    let destination = runtime_log
        .root()
        .parent()
        .ok_or_else(|| "runtime logging smoke runtime root has no parent".to_string())?
        .join(bundle_name);
    let (page, report) = tauri::async_runtime::block_on(
        commands::runtime_diagnostics::run_runtime_logging_smoke_commands(
            settings.inner(),
            runtime_log.inner(),
            registry.inner(),
            &destination,
        ),
    )
    .map_err(|error| format!("runtime logging smoke diagnostics command failed: {error:?}"))?;
    if page.events.is_empty() || report.event_count == 0 {
        return Err("runtime logging smoke diagnostics returned no events".to_string());
    }
    for file in [
        "manifest.json",
        "runtime-summary.json",
        "runtime-events.jsonl",
    ] {
        if !destination.join(file).is_file() {
            return Err(format!("runtime logging smoke bundle missing {file}"));
        }
    }
    if smoke_fault.as_deref() == Some("marker-io") {
        // This fault process proves the marker-unavailable fallback and then
        // takes the normal clean-exit path. It must not enter the smoke's
        // restart probe, whose debug reload intentionally remains resident.
        return Ok(false);
    }
    if boot_index == 1 {
        // The first boot must use the same lifecycle boundary as updater,
        // data-recovery, and tray restart requests. The state marker is
        // written only after the probe/export succeeds, so a failed first
        // boot cannot be mistaken for a valid restart.
        mark_runtime_logging_smoke_state("restart-requested")?;
        request_application_restart(app.handle());
        Ok(true)
    } else {
        Ok(false)
    }
}

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

/// Request a process restart through the application lifecycle boundary.
///
/// All restart initiators (tray, data recovery, and updater) must use this
/// helper so the request is observable before Tauri begins the normal
/// `ExitRequested` drain. The event payload deliberately contains no caller
/// supplied text; the command boundary is the source of correlation context.
pub(crate) fn request_application_restart<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if let Some(runtime_log) = app.try_state::<Arc<observability::runtime::RuntimeLogService>>() {
        runtime_log.record_descriptor(
            app_runtime_events::restart_requested(),
            observability::runtime::EventOutcome::Ok,
            observability::runtime::RuntimeDetail::Phase {
                phase: observability::runtime::RuntimePhase::Shutdown,
            },
        );
        runtime_log.flush();
    }
    app.request_restart();
}

#[cfg(feature = "tray")]
fn setup_tray(app: &tauri::App) -> tauri::Result<()> {
    let show_item = MenuItem::with_id(app, "show", "Show", true, None::<&str>)?;
    let restart_item = MenuItem::with_id(app, "restart", "Restart", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show_item, &restart_item, &quit_item])?;

    let mut tray = TrayIconBuilder::with_id("main-tray")
        .tooltip("Relay Pool Desktop")
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let menu_id = event.id();
            if menu_id.as_ref() == "show" {
                show_main_window(app);
            }
            if menu_id.as_ref() == "restart" {
                request_application_restart(app);
            }
            if menu_id.as_ref() == "quit" {
                if let Some(coordinator) = app.try_state::<ExitCoordinator>() {
                    coordinator.request_exit(app.clone(), ExitReason::TrayQuit, 0);
                } else {
                    observability::runtime::bootstrap::emit(
                        app_runtime_events::exit_coordinator_unavailable(),
                    );
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
                observability::runtime::bootstrap::emit(
                    persistence::runtime_events::relocation_recovery_required(),
                );
                startup_state.enter_recovery(RecoveryReason::PendingRelocation, None);
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
                startup_state.enter_recovery(RecoveryReason::Missing, None);
                return Ok(PreparedDataStore::Recovery(startup_state));
            };
            let db_path = PathBuf::from(&candidate.path);
            let Some(active_data_dir) = db_path.parent().map(Path::to_path_buf) else {
                startup_state.enter_recovery(RecoveryReason::Missing, None);
                return Ok(PreparedDataStore::Recovery(startup_state));
            };
            startup_state.set_startup_upgrade_stage(StartupUpgradeStage::Probe, None);
            let journal_path = default_data_dir
                .join(persistence::baseline_conversion_support::BASELINE_CONVERSION_JOURNAL_FILE);
            match probe_upgrade_state_with_journal(
                &db_path,
                Some(&journal_path),
                Some(device_keys.active_key_id().as_str()),
            ) {
                Ok(probe) => {
                    startup_state.set_startup_upgrade_stage(
                        StartupUpgradeStage::Probe,
                        Some(probe.compatibility_schema_version),
                    );
                    match plan_upgrade(&probe) {
                        StartupUpgradePlan::Execute(steps) => {
                            startup_state.set_startup_upgrade_stage(
                                StartupUpgradeStage::Migrate,
                                Some(probe.compatibility_schema_version),
                            );
                            services::data_store::generation_upgrade::prepare_generation_two_with_resolver(
                            &default_data_dir,
                            &active_data_dir,
                            Some(&db_path),
                            Some(&steps),
                            device_keys,
                        )
                        }
                        StartupUpgradePlan::NeedsRecovery(reason) => {
                            observability::runtime::bootstrap::emit(
                                persistence::runtime_events::startup_plan_recovery_required(),
                            );
                            startup_state.enter_recovery(
                                reason.recovery_reason(),
                                Some(probe.compatibility_schema_version),
                            );
                            return Ok(PreparedDataStore::Recovery(startup_state));
                        }
                    }
                }
                Err(error) => {
                    observability::runtime::bootstrap::emit(
                        persistence::runtime_events::startup_probe_recovery_required(),
                    );
                    startup_state.enter_recovery(error.recovery_reason(), None);
                    return Ok(PreparedDataStore::Recovery(startup_state));
                }
            }
        }
        StartupDecision::FirstRun { default_data_dir } => {
            startup_state.set_startup_upgrade_stage(StartupUpgradeStage::Migrate, None);
            services::data_store::generation_upgrade::prepare_generation_two_with_resolver(
                &default_data_dir,
                &default_data_dir,
                None,
                None,
                device_keys,
            )
        }
        StartupDecision::NeedsRecovery { reason } => {
            let current_schema_version = startup_state.startup_upgrade().current_schema_version;
            startup_state.enter_recovery(reason, current_schema_version);
            return Ok(PreparedDataStore::Recovery(startup_state));
        }
        StartupDecision::Conflict { .. } => {
            return Ok(PreparedDataStore::Recovery(startup_state));
        }
    };

    match persistence {
        Ok((runtime, database_path)) => {
            let probed_schema_version = startup_state.startup_upgrade().current_schema_version;
            startup_state
                .set_startup_upgrade_stage(StartupUpgradeStage::Validate, probed_schema_version);
            let mut ready_state = match inspect_startup(&startup_default_data_dir) {
                Ok(state) => state,
                Err(_) => {
                    startup_state.enter_recovery(
                        RecoveryReason::InconsistentSchemaMetadata,
                        probed_schema_version,
                    );
                    return Ok(PreparedDataStore::Recovery(startup_state));
                }
            };
            if matches!(ready_state.decision, StartupDecision::Ready { .. }) {
                ready_state.set_startup_upgrade_stage(
                    StartupUpgradeStage::Ready,
                    Some(persistence::current_schema_version()),
                );
                Ok(PreparedDataStore::Ready {
                    runtime: Arc::new(runtime),
                    database_path,
                    startup_state: ready_state,
                })
            } else {
                ready_state.set_startup_upgrade_stage(
                    StartupUpgradeStage::Validate,
                    probed_schema_version,
                );
                ready_state.enter_recovery(
                    RecoveryReason::InconsistentSchemaMetadata,
                    probed_schema_version,
                );
                Ok(PreparedDataStore::Recovery(ready_state))
            }
        }
        Err(error) => {
            observability::runtime::bootstrap::emit(
                persistence::runtime_events::startup_recovery_required(),
            );
            let reason = error.recovery_reason();
            let current_schema_version = startup_state.startup_upgrade().current_schema_version;
            startup_state.enter_recovery(reason, current_schema_version);
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

fn recovery_reason_for_existing_database_device_key_error(
    error: services::secrets::device_key_store::DeviceKeyErrorKind,
) -> RecoveryReason {
    use services::secrets::device_key_store::DeviceKeyErrorKind;
    match error {
        DeviceKeyErrorKind::NotFound => RecoveryReason::MissingKey,
        other => recovery_reason_for_device_key_error(other),
    }
}

fn startup_has_recovery_evidence(
    default_data_dir: &Path,
    startup_state: &DataStoreStartupState,
) -> bool {
    !startup_state.candidates.is_empty()
        || default_data_dir
            .join(persistence::baseline_conversion_support::BASELINE_CONVERSION_JOURNAL_FILE)
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
        PortableActivationManualReason::PathRejected
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
    startup_state.enter_recovery(reason, None);
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
    #[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
    if std::env::var_os("RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT").is_some() {
        return Ok(StartupSecretMaterial {
            manager: Some(services::secrets::SecretManager::for_runtime_logging_smoke()),
            // Keeping this `None` skips the production first-run commit path,
            // whose only purpose is to publish a key to Credential Manager.
            // The smoke key is intentionally process-local and non-persistent.
            first_run_key_id: None,
            startup_state,
        });
    }

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
                    startup_state
                        .enter_recovery(recovery_reason_for_device_key_error(error.kind()), None);
                    Ok(StartupSecretMaterial {
                        manager: None,
                        first_run_key_id: None,
                        startup_state,
                    })
                }
            }
        }
        StartupDecision::FirstRun { .. } => {
            startup_state.enter_recovery(RecoveryReason::SystemCredentialMissing, None);
            Ok(StartupSecretMaterial {
                manager: None,
                first_run_key_id: None,
                startup_state,
            })
        }
        StartupDecision::Ready { .. } => {
            match tauri::async_runtime::block_on(services::secrets::SecretManager::load_existing(
                blocking_executor,
            )) {
                Ok(manager) => Ok(StartupSecretMaterial {
                    manager: Some(manager),
                    first_run_key_id: None,
                    startup_state,
                }),
                Err(error) => {
                    startup_state.enter_recovery(
                        recovery_reason_for_existing_database_device_key_error(error.kind()),
                        None,
                    );
                    Ok(StartupSecretMaterial {
                        manager: None,
                        first_run_key_id: None,
                        startup_state,
                    })
                }
            }
        }
        StartupDecision::NeedsRecovery { .. } | StartupDecision::Conflict { .. } => {
            match tauri::async_runtime::block_on(services::secrets::SecretManager::load_existing(
                blocking_executor,
            )) {
                Ok(manager) => Ok(StartupSecretMaterial {
                    manager: Some(manager),
                    first_run_key_id: None,
                    startup_state,
                }),
                Err(error) => {
                    let reason = if startup_has_recovery_evidence(default_data_dir, &startup_state)
                    {
                        recovery_reason_for_existing_database_device_key_error(error.kind())
                    } else {
                        recovery_reason_for_device_key_error(error.kind())
                    };
                    startup_state.enter_recovery(reason, None);
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
    let lifecycle = app
        .try_state::<Arc<runtime_composition::RuntimeLogLifecycle>>()
        .map(|lifecycle| Arc::clone(&*lifecycle));
    if let Some(lifecycle) = lifecycle {
        lifecycle
            .shutdown(|| drain_application_components(&app))
            .await;
    } else {
        // The application drain must still run if setup only registered a
        // subset of state. There is no marker/logger owner to finalize in
        // this degraded setup path.
        let _ = drain_application_components(&app).await;
    }
}

async fn drain_application_components(app: &tauri::AppHandle) -> Result<(), ()> {
    if let Some(runner) = app.try_state::<services::monitoring::runner::MonitoringRunnerState>() {
        runner.stop();
    }
    if let Some(runner) =
        app.try_state::<services::station_collectors::StationCollectorRunnerState>()
    {
        runner.stop();
    }
    let mut proxy_drain_failed = false;
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
        if drain.await.is_err() {
            proxy_drain_failed = true;
        }
    }
    if let Some(work_runtime) = app.try_state::<app_composition::ManagedWorkRuntime>() {
        if work_runtime
            .supervisor
            .shutdown(Duration::from_secs(10))
            .await
            .is_err()
        {
            observability::runtime::bootstrap::emit(
                app_runtime_events::shutdown_supervisor_failed(),
            );
        }
        if work_runtime
            .blocking
            .shutdown(Duration::from_secs(10))
            .await
            .is_err()
        {
            observability::runtime::bootstrap::emit(app_runtime_events::shutdown_blocking_failed());
        }
    }
    if let Some(owner) = app.try_state::<DataStoreRuntimeOwner>() {
        if owner.shutdown().await.is_err() {
            observability::runtime::bootstrap::emit(
                app_runtime_events::shutdown_persistence_failed(),
            );
        }
    }
    if proxy_drain_failed {
        Err(())
    } else {
        Ok(())
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            show_main_window(app);
        }))
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            app.manage(Arc::new(TrayBehaviorState::default()));
            app.manage(ipc::dto::runtime_context::RuntimeContextRegistry::new());
            app.manage(ExitCoordinator::new(Duration::from_secs(45)));
            app.manage(application::data_maintenance::DataMaintenanceCoordinator::new());
            #[cfg(feature = "tray")]
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
            let (app_config_dir, default_data_dir) = resolve_application_directories(app)?;
            services::secrets::keychain::configure_for_app_identifier(&app.config().identifier)
                .map_err(|error| format!("failed to configure credential namespace: {error}"))?;
            let runtime_log_root = default_data_dir.join("runtime-logs");
            let runtime_lifecycle = Arc::new(runtime_composition::RuntimeLogLifecycle::open(
                &runtime_log_root,
            ));
            if let Some(marker) = runtime_lifecycle.marker() {
                let panic_marker = marker;
                std::panic::set_hook(Box::new(move |_| panic_marker.record_panic()));
            }
            let runtime_log = runtime_lifecycle.service();
            observability::runtime::bootstrap::install(Arc::clone(&runtime_log));
            runtime_lifecycle.record_startup();
            // Acquire the business installation lease only after the
            // independent runtime logger is ready. This makes a startup
            // contention/failure durable instead of leaving its fixed event
            // in the pre-install pending queue when setup returns early.
            let installation_lease = InstallationLease::try_acquire(&app_config_dir)
                .map_err(|error| {
                    runtime_log.flush();
                    format!("failed to acquire installation lease: {error}")
            })?;
            app.manage(Arc::clone(&runtime_lifecycle));
            app.manage(Arc::clone(&runtime_log));
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
                    observability::runtime::bootstrap::emit(services::portable_migration::runtime_events::recovery_required());
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
                        observability::runtime::bootstrap::emit(persistence::runtime_events::device_key_recovery_required());
                        if let Err(close_error) = tauri::async_runtime::block_on(runtime.close()) {
                            observability::runtime::bootstrap::emit(persistence::runtime_events::runtime_close_failed());
                        }
                        startup_state.enter_recovery(RecoveryReason::SystemCredentialInternal, None);
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
                        let alerting_updates: Arc<
                            dyn application::alerting::AlertingReadModelUpdatePublisher,
                        > = Arc::new(
                            services::alerting::TauriAlertingReadModelUpdatePublisher::new(
                                app.handle().clone(),
                            ),
                        );
                        let app_services = app_composition::compose_app_services_with_alerting_read_model_updates(
                            runtime.handle(),
                            device_keys.clone(),
                            active_data_dir.display().to_string(),
                            None,
                            data_directory_port,
                            blocking_executor.clone(),
                            Arc::clone(&alerting_updates),
                        );
                        let routing_policy_mutations =
                            app_composition::compose_routing_policy_mutation_coordinator(
                                &app_services,
                                Arc::clone(&proxy_runtime),
                            );
                        tauri::async_runtime::block_on(
                            application::model_mapping::initialize_from_persistence(
                                runtime.handle(),
                            ),
                        )
                        .map_err(|error| format!("failed to initialize model mapping: {error}"))?;
                        tauri::async_runtime::block_on(
                            application::routing::initialize_routing_policy_document_sync(
                                runtime.handle(),
                            ),
                        )
                        .map_err(|error| {
                            format!("failed to initialize routing policy document: {error}")
                        })?;
                        let settings = tauri::async_runtime::block_on(app_services.settings.load())
                            .map_err(|error| {
                                format!("failed to load application settings: {error}")
                            })?;
                        #[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
                        let settings = if std::env::var_os(
                            "RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT",
                        )
                        .is_some()
                        {
                            // Developer diagnostics are enabled only in the
                            // isolated smoke database. Production defaults
                            // and the ordinary command gate remain unchanged.
                            tauri::async_runtime::block_on(app_services.settings.update(
                                models::settings::UpdateSettingsInput {
                                    local_proxy_port: settings.local_proxy_port,
                                    collector_proxy_mode: settings.collector_proxy_mode.clone(),
                                    collector_proxy_url: settings.collector_proxy_url.clone(),
                                    low_balance_threshold_cny: settings.low_balance_threshold_cny,
                                    collector_interval_minutes: settings.collector_interval_minutes,
                                    balance_interval_minutes: settings.balance_interval_minutes,
                                    group_rate_interval_minutes: settings.group_rate_interval_minutes,
                                    published_status_interval_minutes: settings
                                        .published_status_interval_minutes,
                                    pricing_refresh_interval_minutes:
                                        settings.pricing_refresh_interval_minutes,
                                    collector_timeout_seconds: settings.collector_timeout_seconds,
                                    collector_max_concurrency: settings.collector_max_concurrency,
                                    developer_mode_enabled: true,
                                    show_decision_explanation: settings.show_decision_explanation,
                                    tray_behavior: Some(settings.tray_behavior.clone()),
                                },
                            ))
                            .map_err(|error| {
                                format!("failed to enable smoke developer diagnostics: {error}")
                            })?
                        } else {
                            settings
                        };
                        let station_collection_coordinator =
                            services::station_collection_coordinator::StationCollectionCoordinator::new(
                                NonZeroUsize::new(usize::from(settings.collector_max_concurrency))
                                    .expect("settings store validates collector concurrency as non-zero"),
                            );
                        let station_collection_feedback =
                            services::station_collection_feedback::StationCollectionFeedback::default();
                        app.manage(app_composition::compose_alerting_command_facade(
                            runtime.handle(),
                            Arc::clone(&alerting_updates),
                        ));
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
                                station_collection_coordinator.clone(),
                            );
                        let key_pool_command_facade =
                            app_composition::compose_key_pool_command_facade(&app_services);
                        let provider_draft_command_facade =
                            app_composition::compose_provider_draft_command_facade(
                                &app_services,
                                outbound_client.clone(),
                                Arc::clone(&provider_registry),
                            );
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
                                Arc::clone(&proxy_runtime),
                                Arc::clone(&routing_policy_mutations),
                            );
                        let request_logs_command_facade =
                            app_composition::compose_request_logs_command_facade(&app_services);
                        let dashboard_metrics_command_facade =
                            app_composition::compose_dashboard_metrics_command_facade(&app_services);
                        app.manage(dashboard_metrics_command_facade);
                        let monitoring_runner =
                            services::monitoring::runner::compose_monitoring_runner(&app_services);
                        let channel_monitoring_command_facade =
                            app_composition::compose_channel_monitoring_command_facade(
                                &app_services,
                                Arc::clone(&monitoring_runner),
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
                                station_collection_coordinator.clone(),
                                station_collection_feedback.clone(),
                            );
                        let station_key_connectivity_command_facade =
                            app_composition::compose_station_key_connectivity_command_facade(
                                &app_services,
                            );
                        let capture_command_facade =
                            app_composition::compose_capture_command_facade(
                                &app_services,
                                capture_session_store.clone(),
                                outbound_client.clone(),
                                Arc::clone(&provider_registry),
                                blocking_executor.clone(),
                            );
                        let pricing_command_facade =
                            app_composition::compose_pricing_command_facade(
                                &app_services,
                                outbound_client.clone(),
                            );
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
                            );
                        tauri::async_runtime::block_on(
                            app_services.pricing.ensure_builtin_model_base_prices(),
                        )
                        .map_err(|error| {
                            format!("failed to initialize built-in model prices: {error}")
                        })?;
                        tauri::async_runtime::block_on(
                            pricing_command_facade.reload_model_price_catalog(),
                        )
                        .map_err(|error| {
                            format!("failed to apply local model price overrides: {error}")
                        })?;
                        tauri::async_runtime::block_on(
                            app_services
                                .monitoring
                                .recover_startup_interrupted_monitoring_executions(),
                        )
                        .map_err(|error| {
                            format!("failed to recover interrupted monitor executions: {error}")
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
                        let model_price_sync_task =
                            services::model_price_sync::register_model_price_sync_task(
                                &supervisor_handle,
                                pricing_command_facade.model_price_sync_service(),
                            )
                            .map_err(|error| {
                                format!("failed to register model price synchronization: {error}")
                            })?;
                        supervisor_handle
                            .start(&model_price_sync_task)
                            .map_err(|error| {
                                format!("failed to start model price synchronization: {error}")
                            })?;
                        let installation_hash = active_data_dir
                            .display()
                            .to_string()
                            .bytes()
                            .fold(0xcbf29ce484222325_u64, |hash, byte| {
                                hash ^ u64::from(byte)
                            });
                        let maintenance_task =
                            services::monitoring::maintenance::register_monitoring_maintenance_task(
                                &supervisor_handle,
                                app_services.monitoring.clone(),
                                services::monitoring::maintenance::MonitoringMaintenanceConfig::default(
                                ),
                                installation_hash,
                            )
                            .map_err(|error| {
                                format!("failed to register monitoring maintenance: {error}")
                            })?;
                        supervisor_handle
                            .start(&maintenance_task)
                            .map_err(|error| {
                                format!("failed to start monitoring maintenance: {error}")
                            })?;
                        let alerting_runtime_task =
                            background_tasks::alerting_runner::register_alerting_runtime_task(
                                &supervisor_handle,
                                runtime.handle(),
                                std::sync::Arc::new(
                                    services::alerting::TauriDesktopNotificationAdapter::new(
                                        app.handle().clone(),
                                    ),
                                ),
                                Arc::clone(&alerting_updates),
                            )
                            .map_err(|error| {
                                format!("failed to register alerting runtime: {error}")
                            })?;
                        supervisor_handle
                            .start(&alerting_runtime_task)
                            .map_err(|error| {
                                format!("failed to start alerting runtime: {error}")
                            })?;
                        let routing_generation_cutover_task =
                            background_tasks::routing_generation_cutover_runner::register_routing_generation_cutover_task(
                                &supervisor_handle,
                                runtime.handle(),
                                Arc::clone(&routing_policy_mutations),
                            )
                            .map_err(|error| {
                                format!("failed to register routing generation builder: {error}")
                            })?;
                        supervisor_handle
                            .start(&routing_generation_cutover_task)
                            .map_err(|error| {
                                format!("failed to start routing generation builder: {error}")
                            })?;
                        let routing_projection_task =
                            background_tasks::routing_projection_runner::register_routing_projection_task(
                                &supervisor_handle,
                                runtime.handle(),
                            )
                            .map_err(|error| {
                                format!("failed to register routing projection runner: {error}")
                            })?;
                        supervisor_handle
                            .start(&routing_projection_task)
                            .map_err(|error| {
                                format!("failed to start routing projection runner: {error}")
                            })?;
                        let station_key_circuit_reaper_task =
                            background_tasks::station_key_circuit_reaper::register_station_key_circuit_reaper_task(
                                 &supervisor_handle,
                                 runtime.handle(),
                                 Arc::clone(&app_services.routing),
                                 Arc::clone(&app_services.request_finalization),
                             )
                            .map_err(|error| {
                                format!("failed to register station-key circuit reaper: {error}")
                            })?;
                        supervisor_handle
                            .start(&station_key_circuit_reaper_task)
                            .map_err(|error| {
                                format!("failed to start station-key circuit reaper: {error}")
                            })?;
                        let policy_document_task =
                            background_tasks::policy_document_runner::register_policy_document_task(
                                &supervisor_handle,
                                Arc::new({
                                    let runtime = runtime.handle();
                                    move || {
                                        Box::pin(
                                            application::model_mapping::reconcile_external_model_mapping_document(
                                                runtime.clone(),
                                            ),
                                        )
                                    }
                                }),
                                Arc::clone(&routing_policy_mutations),
                            )
                            .map_err(|error| {
                                format!("failed to register policy document reconciler: {error}")
                            })?;
                        supervisor_handle
                            .start(&policy_document_task)
                            .map_err(|error| {
                                format!("failed to start policy document reconciler: {error}")
                            })?;
                        let monitoring_runner_state =
                            services::monitoring::runner::MonitoringRunnerState::start(
                                supervisor_handle.clone(),
                                monitoring_runner,
                            )
                            .map_err(|error| {
                                format!("failed to start monitoring runner: {error}")
                            })?;
                        let station_collector_runner =
                            services::station_collectors::StationCollectorRunnerState::start_v2(
                                supervisor_handle,
                                services::station_collectors::v2_runner_port(
                                    &app_services,
                                    blocking_executor,
                                    outbound_client,
                                    provider_registry,
                                    Arc::new(remote_keys_command_facade.clone()),
                                ),
                                station_collection_coordinator,
                                station_collection_feedback,
                            )
                            .map_err(|error| {
                                format!("failed to start station collector runner: {error}")
                            })?;
                        runtime_log.record_descriptor(
                            persistence::runtime_events::database_initialized(),
                            observability::runtime::EventOutcome::Ok,
                            observability::runtime::RuntimeDetail::Phase {
                                phase: observability::runtime::RuntimePhase::Startup,
                            },
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
                                provider_draft_command_facade,
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
                                credentials_command_facade,
                                data_directory_command_facade,
                                local_proxy_command_facade,
                                monitoring_runner_state,
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
                    runtime_log.record_descriptor(
                        persistence::runtime_events::recovery_mode_started(),
                        observability::runtime::EventOutcome::Degraded,
                        observability::runtime::RuntimeDetail::Phase {
                            phase: observability::runtime::RuntimePhase::Recovery,
                        },
                    );
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
            #[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
            if std::env::var_os("RELAY_POOL_RUNTIME_LOGGING_SMOKE_ROOT").is_some() {
                let restart_requested = run_runtime_logging_smoke_probe(app)?;
                if !restart_requested {
                    schedule_runtime_logging_smoke_exit(app);
                }
            }
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
                        observability::runtime::bootstrap::emit(app_runtime_events::exit_coordinator_unavailable());
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
        portable_recovery_reason, recovery_reason_for_device_key_error,
        recovery_reason_for_existing_database_device_key_error, DataStoreRuntimeOwner,
        InstallationLease, LeaseError, RecoveryReason, TrayBehavior,
    };
    use crate::services::{
        portable_migration::recovery::PortableActivationManualReason,
        secrets::device_key_store::DeviceKeyErrorKind,
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

    #[test]
    fn startup_device_key_not_found_distinguishes_fresh_install_from_existing_database() {
        assert_eq!(
            recovery_reason_for_device_key_error(DeviceKeyErrorKind::NotFound),
            RecoveryReason::SystemCredentialMissing
        );
        assert_eq!(
            recovery_reason_for_existing_database_device_key_error(DeviceKeyErrorKind::NotFound),
            RecoveryReason::MissingKey
        );
    }
}
