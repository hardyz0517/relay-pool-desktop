use serde_json::Value;
#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
use std::path::Path;
use tauri::State;

use crate::{
    application::{
        command_facades::SettingsStationsCommandFacade,
        runtime_diagnostics::RuntimeDiagnosticsService,
    },
    commands::error::{CommandError, CommandErrorCode},
    ipc::dto::runtime_context::RuntimeContextRegistry,
    ipc::dto::runtime_diagnostics::{
        RuntimeDiagnosticsPageDto, RuntimeDiagnosticsQueryDto, RuntimeSupportBundleResultDto,
    },
    observability::runtime::RuntimeLogService,
    services::support_bundle::SupportBundleService,
};

#[tauri::command]
pub async fn record_frontend_boundary_failure(
    runtime_log: State<'_, std::sync::Arc<RuntimeLogService>>,
    registry: State<'_, RuntimeContextRegistry>,
    input: Value,
    runtime_context: Option<Value>,
) -> Result<(), CommandError> {
    crate::observability::correlation::in_command_scope_with_runtime_context(
        "record_frontend_boundary_failure",
        &registry,
        runtime_context,
        async {
            crate::ipc::dto::EmptyInputDto::parse(input)?;
            runtime_log.record_descriptor(
                crate::commands::runtime_events::frontend_boundary_failed(),
                crate::observability::runtime::EventOutcome::Error,
                crate::observability::runtime::RuntimeDetail::Boundary {
                    action: crate::observability::runtime::event::BoundaryAction::Failed,
                },
            );
            Ok(())
        },
    )
    .await
}

#[tauri::command]
pub async fn read_runtime_diagnostics(
    settings: State<'_, SettingsStationsCommandFacade>,
    runtime_log: State<'_, std::sync::Arc<RuntimeLogService>>,
    registry: State<'_, RuntimeContextRegistry>,
    input: Value,
    runtime_context: Option<Value>,
) -> Result<RuntimeDiagnosticsPageDto, CommandError> {
    crate::observability::correlation::in_command_scope_with_runtime_context(
        "read_runtime_diagnostics",
        &registry,
        runtime_context,
        async {
            ensure_developer_mode(&settings).await?;
            let query = RuntimeDiagnosticsQueryDto::parse(input)?;
            RuntimeDiagnosticsService::new(std::sync::Arc::clone(&runtime_log))
                .read_page(query)
                .map_err(|_| invalid_diagnostics_input())
        },
    )
    .await
}

#[tauri::command]
pub async fn export_runtime_support_bundle(
    settings: State<'_, SettingsStationsCommandFacade>,
    runtime_log: State<'_, std::sync::Arc<RuntimeLogService>>,
    registry: State<'_, RuntimeContextRegistry>,
    input: Value,
    runtime_context: Option<Value>,
) -> Result<Option<RuntimeSupportBundleResultDto>, CommandError> {
    crate::observability::correlation::in_command_scope_with_runtime_context(
        "export_runtime_support_bundle",
        &registry,
        runtime_context,
        async {
            crate::ipc::dto::EmptyInputDto::parse(input)?;
            ensure_developer_mode(&settings).await?;
            let Some(path) = rfd::FileDialog::new()
                .set_file_name("relay-pool-support-bundle")
                .save_file()
            else {
                return Ok(None);
            };
            let report = SupportBundleService::export(&runtime_log, &path)
                .map_err(|_| CommandError::internal(None))?;
            Ok(Some(RuntimeSupportBundleResultDto {
                event_count: report.event_count,
                issue_count: report.issue_count,
            }))
        },
    )
    .await
}

async fn ensure_developer_mode(
    settings: &SettingsStationsCommandFacade,
) -> Result<(), CommandError> {
    let enabled = settings
        .get_settings()
        .await
        .map_err(super::public_command_application_error)?
        .developer_mode_enabled;
    if enabled {
        Ok(())
    } else {
        Err(CommandError::try_new(
            CommandErrorCode::PermissionDenied,
            "Developer diagnostics are disabled.",
            false,
            None,
            None,
        )
        .expect("developer diagnostics permission error is bounded"))
    }
}

#[cfg(all(feature = "runtime-logging-windows-smoke", debug_assertions))]
pub(crate) async fn run_runtime_logging_smoke_commands(
    settings: &SettingsStationsCommandFacade,
    runtime_log: &std::sync::Arc<RuntimeLogService>,
    registry: &RuntimeContextRegistry,
    destination: &Path,
) -> Result<
    (
        RuntimeDiagnosticsPageDto,
        crate::services::support_bundle::SupportBundleReport,
    ),
    CommandError,
> {
    let page = crate::observability::correlation::in_command_scope_with_runtime_context(
        "read_runtime_diagnostics",
        registry,
        None,
        async {
            ensure_developer_mode(settings).await?;
            RuntimeDiagnosticsService::new(std::sync::Arc::clone(runtime_log))
                .read_page(RuntimeDiagnosticsQueryDto::default())
                .map_err(|_| invalid_diagnostics_input())
        },
    )
    .await?;
    let report = crate::observability::correlation::in_command_scope_with_runtime_context(
        "export_runtime_support_bundle",
        registry,
        None,
        async {
            ensure_developer_mode(settings).await?;
            SupportBundleService::export(runtime_log, destination)
                .map_err(|_| CommandError::internal(None))
        },
    )
    .await?;
    Ok((page, report))
}

fn invalid_diagnostics_input() -> CommandError {
    CommandError::try_new(
        CommandErrorCode::InvalidInput,
        "The runtime diagnostics input is invalid.",
        false,
        None,
        None,
    )
    .expect("runtime diagnostics input error is bounded")
}

#[cfg(all(test, feature = "tauri-test"))]
mod tests {
    use std::{num::NonZeroUsize, sync::Arc};

    use serde_json::Value;
    use tauri::{
        http::HeaderMap,
        ipc::{CallbackFn, InvokeBody},
        webview::InvokeRequest,
        WebviewUrl, WebviewWindowBuilder,
    };

    use super::*;
    use crate::{
        app_composition,
        background_tasks::{BlockingExecutor, BlockingExecutorConfig},
        ipc::dto::runtime_context::RuntimeContextRegistry,
        models::settings::UpdateSettingsInput,
        observability::runtime::{Component, EventLevel, EventOutcome, RuntimeDetail},
        persistence::runtime::PersistenceRuntime,
        services::{
            data_store::data_directory_port::FileDataDirectoryPort, secrets::DeviceKeyResolver,
            station_collection_coordinator::StationCollectionCoordinator,
        },
    };

    struct Fixture {
        _temporary_directory: tempfile::TempDir,
        _persistence: PersistenceRuntime,
        app: tauri::App<tauri::test::MockRuntime>,
        window: tauri::WebviewWindow<tauri::test::MockRuntime>,
        runtime_log: Arc<RuntimeLogService>,
        context_session_id: String,
    }

    async fn fixture(developer_mode_enabled: bool) -> Fixture {
        let temporary_directory = tempfile::tempdir().expect("test application directory");
        let database_path = temporary_directory.path().join("settings.sqlite3");
        let persistence = PersistenceRuntime::initialize_new(&database_path)
            .await
            .expect("test persistence runtime");
        let blocking = BlockingExecutor::new(BlockingExecutorConfig::architecture_budget());
        let data_directory = temporary_directory.path().join("data");
        let services = app_composition::compose_app_services(
            persistence.handle(),
            DeviceKeyResolver::for_test([17; 32]),
            data_directory.display().to_string(),
            None,
            Arc::new(FileDataDirectoryPort::new(
                data_directory.clone(),
                data_directory.clone(),
            )),
            blocking,
        );
        if developer_mode_enabled {
            services
                .settings
                .update(UpdateSettingsInput {
                    local_proxy_port: 8787,
                    routing_policy_name: "cost_stable_first".to_string(),
                    collector_proxy_mode: "direct".to_string(),
                    collector_proxy_url: None,
                    max_rate_multiplier: None,
                    routing_group_scope: None,
                    scheduler_config: None,
                    low_balance_threshold_cny: 15.0,
                    collector_interval_minutes: 30,
                    balance_interval_minutes: 5,
                    group_rate_interval_minutes: 20,
                    pricing_refresh_interval_minutes: 60,
                    collector_timeout_seconds: 15,
                    collector_max_concurrency: 3,
                    allow_depleted_fallback: false,
                    developer_mode_enabled: true,
                    tray_behavior: None,
                })
                .await
                .expect("enable developer mode in test settings");
        }

        let settings = app_composition::compose_settings_stations_command_facade(
            &services,
            Arc::new(crate::TrayBehaviorState::default()),
            StationCollectionCoordinator::new(NonZeroUsize::new(3).expect("non-zero")),
        );
        let runtime_log = Arc::new(RuntimeLogService::open(
            temporary_directory.path().join("runtime-logs"),
        ));
        let registry = RuntimeContextRegistry::new();
        let context_session_id = registry.context_session_id().to_owned();
        let app = tauri::test::mock_builder()
            .manage(settings)
            .manage(Arc::clone(&runtime_log))
            .manage(registry)
            .invoke_handler(tauri::generate_handler![
                read_runtime_diagnostics,
                export_runtime_support_bundle,
                record_frontend_boundary_failure,
            ])
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .expect("mock Tauri app");
        let window = WebviewWindowBuilder::new(&app, "main", WebviewUrl::App("index.html".into()))
            .build()
            .expect("mock webview");

        Fixture {
            _temporary_directory: temporary_directory,
            _persistence: persistence,
            app,
            window,
            runtime_log,
            context_session_id,
        }
    }

    fn invoke(fixture: &Fixture, command: &str, body: Value) -> Result<Value, Value> {
        let request = InvokeRequest {
            cmd: command.to_string(),
            callback: CallbackFn(0),
            error: CallbackFn(1),
            url: "http://tauri.localhost".parse().expect("mock URL"),
            body: InvokeBody::Json(body),
            headers: HeaderMap::new(),
            invoke_key: tauri::test::INVOKE_KEY.to_string(),
        };
        tauri::test::get_ipc_response(&fixture.window, request)
            .map(|response| response.deserialize::<Value>().expect("JSON response"))
    }

    #[test]
    fn read_command_uses_real_state_and_denies_non_developer_mode() {
        let fixture = tauri::async_runtime::block_on(fixture(false));
        let result = invoke(
            &fixture,
            "read_runtime_diagnostics",
            serde_json::json!({ "input": {}, "runtimeContext": null }),
        )
        .expect_err("diagnostics must be developer-only");

        assert_eq!(result["code"], "permission_denied");
        assert_eq!(result["message"], "Developer diagnostics are disabled.");
    }

    #[test]
    fn read_command_returns_allowlisted_event_through_tauri_state() {
        let fixture = tauri::async_runtime::block_on(fixture(true));
        fixture.runtime_log.record(
            "runtime.log_event.dropped",
            Component::Runtime,
            EventLevel::Warn,
            EventOutcome::Ok,
            RuntimeDetail::None,
        );
        fixture.runtime_log.flush();

        let result = invoke(
            &fixture,
            "read_runtime_diagnostics",
            serde_json::json!({ "input": {}, "runtimeContext": null }),
        )
        .expect("developer diagnostics command");
        assert_eq!(
            result["events"][0]["eventCode"],
            "runtime.log_event.dropped"
        );
        assert_eq!(result["events"][0]["detail"]["kind"], "none");
        assert_eq!(result["issueCount"], 0);
        assert_eq!(result["sinkDegraded"], false);
    }

    #[test]
    fn frontend_boundary_command_records_a_fixed_event_without_raw_error_text() {
        let fixture = tauri::async_runtime::block_on(fixture(true));
        invoke(
            &fixture,
            "record_frontend_boundary_failure",
            serde_json::json!({ "input": {}, "runtimeContext": null }),
        )
        .expect("frontend boundary command");
        fixture.runtime_log.flush();

        let result = invoke(
            &fixture,
            "read_runtime_diagnostics",
            serde_json::json!({ "input": {}, "runtimeContext": null }),
        )
        .expect("diagnostics command");
        assert_eq!(result["events"][0]["eventCode"], "frontend.boundary.failed");
        assert_eq!(result["events"][0]["detail"]["kind"], "boundary");
        let serialized = serde_json::to_string(&result).expect("result serialization");
        assert!(!serialized.contains("stack"));
        assert!(!serialized.contains("props"));
    }

    #[test]
    fn command_runtime_context_survives_into_jsonl_and_diagnostics_dto() {
        let fixture = tauri::async_runtime::block_on(fixture(true));
        let interaction_id = "int_0123456789abcdef0123456789abcdef";
        invoke(
            &fixture,
            "record_frontend_boundary_failure",
            serde_json::json!({
                "input": {},
                "runtimeContext": {
                    "contextSessionId": fixture.context_session_id,
                    "interactionId": interaction_id
                }
            }),
        )
        .expect("frontend boundary command with runtime context");
        fixture.runtime_log.flush();

        let result = invoke(
            &fixture,
            "read_runtime_diagnostics",
            serde_json::json!({ "input": {}, "runtimeContext": null }),
        )
        .expect("diagnostics command");
        let event = result["events"]
            .as_array()
            .and_then(|events| {
                events
                    .iter()
                    .find(|event| event["eventCode"] == "frontend.boundary.failed")
            })
            .expect("frontend event in diagnostics DTO");
        assert_eq!(event["interactionId"], interaction_id);
        let correlation_id = event["correlationId"]
            .as_str()
            .expect("command correlation in diagnostics DTO");
        assert_eq!(correlation_id.len(), 32);
        assert!(correlation_id.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn export_command_enforces_developer_gate_before_opening_save_dialog() {
        let fixture = tauri::async_runtime::block_on(fixture(false));
        let result = invoke(
            &fixture,
            "export_runtime_support_bundle",
            serde_json::json!({ "input": {}, "runtimeContext": null }),
        )
        .expect_err("support bundle must be developer-only");
        assert_eq!(result["code"], "permission_denied");
    }
}
