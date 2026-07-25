use base64::{engine::general_purpose, Engine as _};
use serde::Serialize;
use serde_json::{json, Value};
use std::process::Command;
use std::time::{Duration, Instant};
use std::{
    collections::{BTreeMap, VecDeque},
    io::Read,
    path::PathBuf,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::{ipc::Channel, Manager, State};

pub(crate) mod data_recovery;
pub(crate) mod error;

use crate::{
    application::{
        app_services::AppServices,
        command_facades::{
            ChangeEventsCommandFacade, ChannelMonitoringCommandFacade, CredentialsCommandFacade,
            KeyPoolCommandFacade, PricingCommandFacade, RequestLogsCommandFacade,
            RoutingCommandFacade, SettingsStationsCommandFacade,
        },
        error::ApplicationError,
        pagination::PageLimit,
    },
    ipc::dto::{
        change_logs::{
            ChangeEventDto, ChangeEventIdInputDto, ChangeEventIdsInputDto, RequestLogDto,
            StationIdInputDto as ChangeLogStationIdInputDto, UpsertChangeEventInputDto,
        },
        channel_monitor_mutations::{
            ChannelMonitorMutationIdInputDto, CreateChannelMonitorInputDto,
            CreateChannelMonitorTemplateInputDto, UpdateChannelMonitorInputDto,
            UpdateChannelMonitorTemplateInputDto,
        },
        channel_monitor_operations::ChannelStatusWorkspaceDto,
        channel_monitor_reads::{
            ChannelMonitorDto, ChannelMonitorIdInputDto, ChannelMonitorRequestTemplateDto,
            ChannelMonitorRunDto, ChannelMonitorSummaryDto, ChannelMonitorSummaryInputDto,
            ChannelStatusSummaryDto,
        },
        collector_facts::{
            BalanceSnapshotDto, CollectorRunDto, CollectorSnapshotDto, CollectorStationIdInputDto,
            GroupRateRecordDto, StationGroupBindingDto, StationGroupOptionDto,
            UpsertBalanceSnapshotInputDto, UpsertStationGroupBindingInputDto,
        },
        pricing_mutations::{
            PricingRuleIdInputDto, UpsertModelBasePriceInputDto, UpsertPricingRuleInputDto,
        },
        pricing_reads::{
            ModelBasePriceDto, PricingComparisonWorkspaceDto, PricingContextInputDto,
            PricingRuleDto, ResolvedPricingContextDto,
        },
        proxy_workspace_reads::{LocalRoutingWorkspaceDto, ProxyStatusDto},
        routing_health_reads::{
            ModelAliasDto, RouteSimulationInputDto, RouteSimulationResultDto,
            RoutingStationKeyIdInputDto, StationEndpointHealthDto, StationKeyCapabilitiesDto,
            StationKeyHealthDto,
        },
        routing_mutations::{
            DeleteModelAliasInputDto, EndpointPingResultDto, ReorderLocalRoutingKeysInputDto,
            UpdateStationKeyCapabilitiesInputDto, UpsertModelAliasInputDto,
        },
        settings::{
            AppStatusDto, CcswitchImportResultDto, OpenExternalUrlInputDto, SettingsDto,
            UpdateLocalAccessKeyInputDto, UpdateSettingsInputDto,
        },
        station_collector_operations::{
            CaptureSessionStatusDto, CaptureStationIdInputDto, CapturedHttpEventInputDto,
            CollectorRunResultDto, StationCollectorTaskInputDto, StationCollectorTaskTypeDto,
            StationLoginTestInputDto, StationLoginTestResultDto,
        },
        station_keys::{
            BindRemoteStationKeyInputDto, CreateLocalStationKeyFromRemoteResultDto,
            CreateRemoteStationKeyInputDto, CreateRemoteStationKeyResultDto,
            CreateStationKeyInputDto, KeyPoolItemDto, RemoteKeyCapabilityDto,
            RemoteKeyScanResultDto, RemoteStationKeyDto, RemoteStationKeyInputDto,
            ReorderKeyPoolInputDto, ReorderStationKeysInputDto, SaveStationKeyWithDefaultsInputDto,
            SaveStationKeyWithDefaultsResultDto, StationCredentialsDto, StationIdInputDto,
            StationKeyConnectivityInputDto, StationKeyDto, StationKeyIdInputDto,
            UpdateStationCredentialsInputDto, UpdateStationKeyGroupBindingInputDto,
            UpdateStationKeyInputDto, UpdateStationSessionInputDto,
        },
        stations::{
            CreateStationInputDto, DeleteStationInputDto, ReorderStationsInputDto,
            UpdateStationInputDto,
        },
        updater_data_recovery::{
            ActivateDataStoreCandidateInputDto, ActivationResultDto, CreateNewDataStoreInputDto,
            DataStoreCandidateViewDto, DataStoreStartupViewDto, PublishedUpdateInspectionDto,
            PublishedUpdateInspectionInputDto, UpdaterNetworkConfigDto,
        },
        EmptyInputDto, StationDto,
    },
    ipc::runtime_contract::{current_runtime_contract, RuntimeContractInfo},
    models::{
        capture::CapturedHttpEventInput,
        collector::CollectorRunResult,
        credentials::PersistStationSessionInput,
        proxy::{ProxyStatus, UpstreamApiFormat},
        routing::StationKeyCapabilities,
        station_keys::KeyPoolItem,
        stations::EndpointPingResult,
        AppStatus,
    },
    observability::correlation,
    services::{
        capture, collectors,
        data_store::{
            backup::backup_selected_database,
            config::{
                create_installation_marker, write_config_v3, DataDirConfigV3, DatabaseGeneration,
            },
            diagnostic::build_diagnostic_report,
            inspect::inspect_candidate,
            inspect_startup,
            types::{
                ActivationResult, CandidateHealth, CandidateRole, DataStoreCandidate,
                DataStoreStartupState,
            },
        },
        endpoint_ping::ping_station_endpoint as probe_station_endpoint,
        proxy::{redact_error_message, runtime::ProxyRuntimeState, should_fallback},
        remote_keys,
        secrets::{validation::validate_database_secrets, SecretManager},
        station_endpoints::{build_api_url, url_belongs_to_base},
        time::now_millis_for_services,
        updater,
    },
};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";
const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
const DATABASE_FILE_V2: &str = "relay-pool-desktop-v2.sqlite3";

const STATION_KEY_CONNECTIVITY_MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(5);
const STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT: Duration = Duration::from_secs(8);
const STATION_KEY_CONNECTIVITY_CANDIDATE_LIMIT: usize = 2;
const STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT: usize = 64 * 1024;
const DEFAULT_STATION_KEY_CONNECTIVITY_MODEL: &str = "gpt-4.1-mini";

const LOCATED_CANDIDATE_LIMIT: usize = 32;

#[derive(Default)]
pub(crate) struct LocatedDataStoreCandidates(Mutex<LocatedDataStoreCandidatesInner>);

#[derive(Default)]
struct LocatedDataStoreCandidatesInner {
    paths: BTreeMap<String, PathBuf>,
    insertion_order: VecDeque<String>,
}

impl LocatedDataStoreCandidates {
    fn record(&self, candidate: &DataStoreCandidate) {
        let mut inner = self
            .0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if inner.paths.contains_key(&candidate.id) {
            inner
                .insertion_order
                .retain(|candidate_id| candidate_id != &candidate.id);
        }
        inner
            .paths
            .insert(candidate.id.clone(), PathBuf::from(&candidate.path));
        inner.insertion_order.push_back(candidate.id.clone());
        while inner.paths.len() > LOCATED_CANDIDATE_LIMIT {
            if let Some(expired) = inner.insertion_order.pop_front() {
                inner.paths.remove(&expired);
            }
        }
    }

    fn path(&self, candidate_id: &str) -> Option<PathBuf> {
        self.0
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .paths
            .get(candidate_id)
            .cloned()
    }
}

#[cfg(test)]
mod located_candidate_tests {
    use super::{LocatedDataStoreCandidates, LOCATED_CANDIDATE_LIMIT};
    use crate::services::data_store::types::{CandidateHealth, CandidateRole, DataStoreCandidate};

    #[test]
    fn only_backend_inspected_candidate_ids_enter_the_registry() {
        let registry = LocatedDataStoreCandidates::default();
        let candidate = DataStoreCandidate {
            id: "inspected-id".to_string(),
            role: CandidateRole::Located,
            path: "relay-pool-desktop-v2.sqlite3".to_string(),
            health: CandidateHealth::Healthy,
            schema_compatible: true,
            size_bytes: None,
            modified_at: None,
            counts: std::collections::BTreeMap::new(),
        };

        registry.record(&candidate);

        assert_eq!(
            registry.path("inspected-id"),
            Some(std::path::PathBuf::from(&candidate.path))
        );
        assert_eq!(registry.path("user-supplied-id"), None);
    }

    #[test]
    fn located_candidate_registry_is_bounded() {
        let registry = LocatedDataStoreCandidates::default();
        for index in 0..=LOCATED_CANDIDATE_LIMIT {
            registry.record(&DataStoreCandidate {
                id: format!("candidate-{index:02}"),
                role: CandidateRole::Located,
                path: format!("candidate-{index:02}.sqlite3"),
                health: CandidateHealth::Healthy,
                schema_compatible: true,
                size_bytes: None,
                modified_at: None,
                counts: std::collections::BTreeMap::new(),
            });
        }

        assert_eq!(registry.path("candidate-00"), None);
        assert_eq!(
            registry.path(&format!("candidate-{LOCATED_CANDIDATE_LIMIT:02}")),
            Some(std::path::PathBuf::from(format!(
                "candidate-{LOCATED_CANDIDATE_LIMIT:02}.sqlite3"
            )))
        );
    }
}

#[tauri::command]
pub async fn app_status(input: Value) -> Result<AppStatusDto, error::CommandError> {
    correlation::in_command_scope("app_status", async {
        EmptyInputDto::parse(input)?;
        Ok(AppStatus::default())
    })
    .await
}

/// Returns only the immutable build/IPC identity needed before normal app queries.
#[tauri::command]
pub async fn get_runtime_contract_info(
    input: Value,
) -> Result<RuntimeContractInfo, error::CommandError> {
    correlation::in_command_scope("get_runtime_contract_info", async {
        EmptyInputDto::parse(input)?;
        Ok(current_runtime_contract())
    })
    .await
}

#[tauri::command]
pub async fn get_data_store_startup_state(
    state: State<'_, DataStoreStartupState>,
    input: Value,
) -> Result<DataStoreStartupViewDto, error::CommandError> {
    correlation::in_command_scope("get_data_store_startup_state", async {
        EmptyInputDto::parse(input)?;
        Ok(data_recovery::startup_view(&state))
    })
    .await
}

#[tauri::command]
pub async fn refresh_data_store_candidates(
    state: State<'_, DataStoreStartupState>,
    input: Value,
) -> Result<DataStoreStartupViewDto, error::CommandError> {
    correlation::in_command_scope("refresh_data_store_candidates", async {
        EmptyInputDto::parse(input)?;
        Ok(inspect_startup(state.default_data_dir())
            .map(|state| data_recovery::startup_view(&state))?)
    })
    .await
}

#[tauri::command]
pub async fn locate_data_store_candidate(
    located: State<'_, LocatedDataStoreCandidates>,
    input: Value,
) -> Result<Option<DataStoreCandidateViewDto>, error::CommandError> {
    correlation::in_command_scope("locate_data_store_candidate", async {
        EmptyInputDto::parse(input)?;
        let Some(path) = rfd::FileDialog::new()
            .add_filter("Relay Pool SQLite", &["sqlite3"])
            .pick_file()
        else {
            return Ok(None);
        };
        if !is_supported_database_file(&path) {
            return Err(format!(
                "selected database must be named {DATABASE_FILE} or {DATABASE_FILE_V2}"
            )
            .into());
        }
        let candidate = inspect_candidate(&path, CandidateRole::Located)?.candidate;
        located.record(&candidate);
        Ok(Some(data_recovery::candidate_view(&candidate)))
    })
    .await
}

#[tauri::command]
pub async fn activate_data_store_candidate(
    state: State<'_, DataStoreStartupState>,
    located: State<'_, LocatedDataStoreCandidates>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<ActivationResultDto, error::CommandError> {
    correlation::in_command_scope("activate_data_store_candidate", async {
        let input = ActivateDataStoreCandidateInputDto::parse(input)?;
        let candidate_path = state
            .candidates
            .iter()
            .find(|candidate| candidate.id == input.candidate_id)
            .map(|candidate| PathBuf::from(&candidate.path))
            .or_else(|| located.path(&input.candidate_id))
            .ok_or_else(|| {
                "selected data store candidate is not part of inspected evidence".to_string()
            })?;
        let canonical_path = candidate_path
            .canonicalize()
            .map_err(|error| format!("failed to resolve selected database path: {error}"))?;
        if !is_supported_database_file(&canonical_path) {
            return Err(format!(
                "selected database must be named {DATABASE_FILE} or {DATABASE_FILE_V2}"
            )
            .into());
        }

        if crate::services::data_store::generation_upgrade::commit_explicit_generation_two_recovery(
            state.default_data_dir(),
            &canonical_path,
            *secrets.data_key(),
        )? {
            return Ok(ActivationResult {
                restart_required: true,
            });
        }

        let inspected = inspect_candidate(&canonical_path, CandidateRole::Located)?;
        if inspected.candidate.health != CandidateHealth::Healthy
            || !inspected.contains_relay_pool_schema
            || !inspected.candidate.schema_compatible
        {
            return Err("selected database is not a healthy Relay Pool database"
                .to_string()
                .into());
        }
        validate_database_secrets(&canonical_path, secrets.data_key())?;
        backup_selected_database(&canonical_path, state.default_data_dir())?;

        let active_data_dir = canonical_path
            .parent()
            .ok_or_else(|| "selected database path has no parent directory".to_string())?;
        let database_generation = if canonical_path.file_name().and_then(|name| name.to_str())
            == Some(DATABASE_FILE_V2)
        {
            DatabaseGeneration::Two
        } else {
            DatabaseGeneration::One
        };
        write_config_v3(
            &state.default_data_dir().join(DATA_DIR_CONFIG_FILE),
            &DataDirConfigV3 {
                version: 3,
                active_data_dir: Some(active_data_dir.to_path_buf()),
                pending_data_dir: None,
                source_data_dir: None,
                database_generation,
                updated_at: data_store_updated_at(),
            },
        )?;
        create_installation_marker(state.default_data_dir())?;

        Ok(ActivationResult {
            restart_required: true,
        })
    })
    .await
}

#[tauri::command]
pub async fn create_new_data_store(
    state: State<'_, DataStoreStartupState>,
    input: Value,
) -> Result<ActivationResultDto, error::CommandError> {
    correlation::in_command_scope("create_new_data_store", async {
        let input = CreateNewDataStoreInputDto::parse(input)?;
        if !input.confirmed {
            return Err("creating a new data store requires confirmation"
                .to_string()
                .into());
        }
        let Some(data_dir) = rfd::FileDialog::new().pick_folder() else {
            return Err("no data directory selected".to_string().into());
        };
        let db_path = data_dir.join(DatabaseGeneration::Two.database_file());
        if db_path.exists() {
            return Err(format!("target database already exists: {}", db_path.display()).into());
        }
        crate::services::data_store::generation_upgrade::initialize_empty_generation_two(&db_path)
            .await?;
        write_config_v3(
            &state.default_data_dir().join(DATA_DIR_CONFIG_FILE),
            &DataDirConfigV3 {
                version: 3,
                active_data_dir: Some(data_dir.clone()),
                pending_data_dir: None,
                source_data_dir: None,
                database_generation: DatabaseGeneration::Two,
                updated_at: data_store_updated_at(),
            },
        )?;
        create_installation_marker(state.default_data_dir())?;
        Ok(ActivationResult {
            restart_required: true,
        })
    })
    .await
}

#[tauri::command]
pub async fn open_data_store_backup_dir(
    state: State<'_, DataStoreStartupState>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("open_data_store_backup_dir", async {
        EmptyInputDto::parse(input)?;
        let backups = state.default_data_dir().join("backups");
        std::fs::create_dir_all(&backups).map_err(|error| {
            format!(
                "failed to create backup directory {}: {error}",
                backups.display()
            )
        })?;
        Ok(open_path_with_system(&backups)?)
    })
    .await
}

#[tauri::command]
pub async fn export_data_store_diagnostic(
    state: State<'_, DataStoreStartupState>,
    input: Value,
) -> Result<Option<String>, error::CommandError> {
    correlation::in_command_scope("export_data_store_diagnostic", async {
        EmptyInputDto::parse(input)?;
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("relay-pool-data-store-diagnostic.json")
            .save_file()
        else {
            return Ok(None);
        };
        let report = build_diagnostic_report(state.default_data_dir(), &state)?;
        let bytes = serde_json::to_vec_pretty(&report)
            .map_err(|error| format!("failed to serialize data-store diagnostic: {error}"))?;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|error| {
                format!(
                    "failed to create diagnostic directory {}: {error}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(&path, bytes)
            .map_err(|error| format!("failed to write diagnostic {}: {error}", path.display()))?;
        Ok(Some(path.display().to_string()))
    })
    .await
}

fn data_store_updated_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[tauri::command]
pub async fn list_stations(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<Vec<StationDto>, error::CommandError> {
    correlation::in_command_scope("list_stations", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_stations()
            .await
            .map(|stations| stations.into_iter().map(StationDto::from).collect())
            .map_err(public_command_application_error)
    })
    .await
}

fn is_supported_database_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == DATABASE_FILE || name == DATABASE_FILE_V2)
}

#[tauri::command]
pub async fn create_station(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<StationDto, error::CommandError> {
    correlation::in_command_scope("create_station", async {
        let input = CreateStationInputDto::parse(input)?.into_domain()?;
        facade
            .create_station(input)
            .await
            .map(StationDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<StationDto, error::CommandError> {
    correlation::in_command_scope("update_station", async {
        let input = UpdateStationInputDto::parse(input)?.into_domain()?;
        facade
            .update_station(input)
            .await
            .map(StationDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_station(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_station", async {
        let input = DeleteStationInputDto::parse(input)?;
        facade
            .delete_station(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reorder_stations(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<Vec<StationDto>, error::CommandError> {
    correlation::in_command_scope("reorder_stations", async {
        let input = ReorderStationsInputDto::parse(input)?;
        facade
            .reorder_stations(input.station_ids)
            .await
            .map(|stations| stations.into_iter().map(StationDto::from).collect())
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_settings(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("get_settings", async {
        EmptyInputDto::parse(input)?;
        facade
            .get_settings()
            .await
            .map(SettingsDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_local_access_key(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<String, error::CommandError> {
    correlation::in_command_scope("get_local_access_key", async {
        EmptyInputDto::parse(input)?;
        facade
            .get_local_access_key()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_local_access_key(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("update_local_access_key", async {
        let input = UpdateLocalAccessKeyInputDto::parse(input)?;
        facade
            .update_local_access_key(input.value)
            .await
            .map(SettingsDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn import_relay_pool_to_ccswitch(
    secrets: State<'_, SecretManager>,
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<CcswitchImportResultDto, error::CommandError> {
    correlation::in_command_scope("import_relay_pool_to_ccswitch", async {
        EmptyInputDto::parse(input)?;
        let settings = services
            .settings
            .load()
            .await
            .map_err(command_application_error)?;
        let local_access_key = services
            .settings
            .ensure_local_access_key()
            .await
            .map_err(command_application_error)?;
        let proxy_status = proxy
            .start(crate::services::proxy::startup::config_from_v2_services(
                services.inner(),
                *secrets.data_key(),
                local_access_key.clone(),
                settings.local_proxy_port,
            ))
            .await?;
        let (result, deeplink) = prepare_ccswitch_import(&local_access_key, &proxy_status);

        open_url_with_system(&deeplink)?;

        Ok(result)
    })
    .await
}

fn prepare_ccswitch_import(
    local_access_key: &str,
    status: &ProxyStatus,
) -> (CcswitchImportResultDto, String) {
    let endpoint = format!("http://{}:{}/v1", status.bind_addr, status.port);
    let homepage = format!("http://{}:{}", status.bind_addr, status.port);
    let provider_name = "Relay Pool Desktop".to_string();
    let deeplink = build_ccswitch_provider_deeplink(
        "codex",
        &provider_name,
        &homepage,
        &endpoint,
        local_access_key,
    );
    (
        CcswitchImportResultDto {
            app: "codex".to_string(),
            provider_name,
            endpoint,
        },
        deeplink,
    )
}

#[tauri::command]
pub async fn open_external_url(input: Value) -> Result<(), error::CommandError> {
    correlation::in_command_scope("open_external_url", async {
        let input = OpenExternalUrlInputDto::parse(input)?;
        let url = validate_external_http_url(&input.url)?;
        Ok(open_url_with_system(url)?)
    })
    .await
}

#[tauri::command]
pub async fn updater_network_config(
    input: Value,
) -> Result<UpdaterNetworkConfigDto, error::CommandError> {
    correlation::in_command_scope("updater_network_config", async {
        EmptyInputDto::parse(input)?;
        Ok(updater::network_config())
    })
    .await
}

#[tauri::command]
pub async fn inspect_latest_update_manifest(
    input: Value,
) -> Result<PublishedUpdateInspectionDto, error::CommandError> {
    correlation::in_command_scope("inspect_latest_update_manifest", async {
        let input = PublishedUpdateInspectionInputDto::parse(input)?;
        Ok(tauri::async_runtime::spawn_blocking(move || {
            updater::inspect_latest_update_manifest(&input.current_version)
        })
        .await
        .map_err(|error| format!("Updater manifest task failed: {error}"))??)
    })
    .await
}

#[tauri::command]
pub async fn update_settings(
    facade: State<'_, SettingsStationsCommandFacade>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("update_settings", async {
        let input = UpdateSettingsInputDto::parse(input)?.into_domain()?;
        let settings = facade
            .update_settings(input)
            .await
            .map_err(public_command_application_error)?;
        Ok(SettingsDto::from(settings))
    })
    .await
}

fn command_application_error(error: ApplicationError) -> error::CommandError {
    error::command_application_error(error)
}

fn public_command_application_error(error: ApplicationError) -> error::CommandError {
    command_application_error(error)
}

fn public_remote_key_error(error: remote_keys::RemoteKeyOperationError) -> error::CommandError {
    match error {
        remote_keys::RemoteKeyOperationError::Application(error) => {
            public_command_application_error(error)
        }
        remote_keys::RemoteKeyOperationError::Unsupported => {
            error::CommandError::from_driver(error::DriverFailure::Unsupported)
        }
        remote_keys::RemoteKeyOperationError::ExternalUnavailable => {
            error::CommandError::from_driver(error::DriverFailure::ExternalUnavailable {
                provider: None,
                upstream_status: None,
            })
        }
        remote_keys::RemoteKeyOperationError::Conflict => {
            public_command_application_error(ApplicationError::StaleRevision)
        }
        remote_keys::RemoteKeyOperationError::Internal => error::CommandError::internal(None),
    }
}

#[tauri::command]
pub async fn choose_data_dir(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("choose_data_dir", async {
        EmptyInputDto::parse(input)?;
        let selected =
            tauri::async_runtime::spawn_blocking(|| rfd::FileDialog::new().pick_folder())
                .await
                .map_err(|_| error::CommandError::internal(None))?;
        let Some(data_dir) = selected else {
            return services
                .settings
                .load()
                .await
                .map(SettingsDto::from)
                .map_err(public_command_application_error);
        };
        services
            .data_directory
            .select_pending(data_dir)
            .await
            .map(SettingsDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reset_data_dir(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<SettingsDto, error::CommandError> {
    correlation::in_command_scope("reset_data_dir", async {
        EmptyInputDto::parse(input)?;
        services
            .data_directory
            .reset_to_default()
            .await
            .map(SettingsDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_proxy_status(
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("get_proxy_status", async {
        EmptyInputDto::parse(input)?;
        let settings = services
            .settings
            .load()
            .await
            .map_err(public_command_application_error)?;
        Ok(proxy.status(settings.local_proxy_port))
    })
    .await
}

#[tauri::command]
pub async fn load_local_routing_workspace(
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<LocalRoutingWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("load_local_routing_workspace", async {
        EmptyInputDto::parse(input)?;
        load_local_routing_workspace_v2(services.inner(), proxy.inner()).await
    })
    .await
}

#[tauri::command]
pub async fn reorder_local_routing_keys(
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<LocalRoutingWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("reorder_local_routing_keys", async {
        let input = ReorderLocalRoutingKeysInputDto::parse(input)?;
        services
            .routing
            .reorder_local_routing_keys(input.station_key_ids)
            .await
            .map_err(public_command_application_error)?;
        load_local_routing_workspace_v2(services.inner(), proxy.inner()).await
    })
    .await
}

async fn load_local_routing_workspace_v2(
    services: &AppServices,
    proxy: &ProxyRuntimeState,
) -> Result<crate::services::proxy::routing_types::LocalRoutingWorkspace, error::CommandError> {
    let settings = services
        .settings
        .load()
        .await
        .map_err(command_application_error)?;
    let request_logs = services
        .request_logs
        .list_recent(PageLimit::new(500).expect("bounded limit"))
        .await
        .map_err(command_application_error)?;
    let proxy_status = proxy.status(settings.local_proxy_port);
    services
        .routing
        .load_local_routing_workspace(settings, request_logs, proxy_status)
        .await
        .map_err(command_application_error)
}

#[tauri::command]
pub async fn start_local_proxy(
    secrets: State<'_, SecretManager>,
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("start_local_proxy", async {
        EmptyInputDto::parse(input)?;
        let settings = services
            .settings
            .load()
            .await
            .map_err(public_command_application_error)?;
        let local_key = services
            .settings
            .ensure_local_access_key()
            .await
            .map_err(public_command_application_error)?;
        let status = proxy
            .start(crate::services::proxy::startup::config_from_v2_services(
                services.inner(),
                *secrets.data_key(),
                local_key,
                settings.local_proxy_port,
            ))
            .await?;
        if let Err(error) = services
            .settings
            .set_local_proxy_start_on_launch(true)
            .await
        {
            let _ = proxy.stop(status.port).await;
            return Err(public_command_application_error(error));
        }
        Ok(status)
    })
    .await
}

#[tauri::command]
pub async fn stop_local_proxy(
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("stop_local_proxy", async {
        EmptyInputDto::parse(input)?;
        let settings = services
            .settings
            .load()
            .await
            .map_err(public_command_application_error)?;
        let status = proxy.stop(settings.local_proxy_port).await?;
        services
            .settings
            .set_local_proxy_start_on_launch(false)
            .await
            .map_err(public_command_application_error)?;
        Ok(status)
    })
    .await
}

#[tauri::command]
pub async fn cleanup_before_update(
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("cleanup_before_update", async {
        EmptyInputDto::parse(input)?;
        let settings = services
            .settings
            .load()
            .await
            .map_err(command_application_error)?;
        Ok(proxy
            .cleanup_before_update(settings.local_proxy_port)
            .await?)
    })
    .await
}

#[tauri::command]
pub async fn prepare_local_proxy_for_update(
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("prepare_local_proxy_for_update", async {
        EmptyInputDto::parse(input)?;
        Ok(proxy.prepare_for_update(Duration::from_secs(30)).await?)
    })
    .await
}

#[tauri::command]
pub async fn restart_local_proxy(
    secrets: State<'_, SecretManager>,
    services: State<'_, AppServices>,
    proxy: State<'_, ProxyRuntimeState>,
    input: Value,
) -> Result<ProxyStatusDto, error::CommandError> {
    correlation::in_command_scope("restart_local_proxy", async {
        EmptyInputDto::parse(input)?;
        let settings = services
            .settings
            .load()
            .await
            .map_err(public_command_application_error)?;
        let local_key = services
            .settings
            .ensure_local_access_key()
            .await
            .map_err(public_command_application_error)?;
        let status = proxy
            .restart(crate::services::proxy::startup::config_from_v2_services(
                services.inner(),
                *secrets.data_key(),
                local_key,
                settings.local_proxy_port,
            ))
            .await?;
        if let Err(error) = services
            .settings
            .set_local_proxy_start_on_launch(true)
            .await
        {
            let _ = proxy.stop(status.port).await;
            return Err(public_command_application_error(error));
        }
        Ok(status)
    })
    .await
}

#[tauri::command]
pub async fn list_request_logs(
    facade: State<'_, RequestLogsCommandFacade>,
    input: Value,
) -> Result<Vec<RequestLogDto>, error::CommandError> {
    correlation::in_command_scope("list_request_logs", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_request_logs(PageLimit::new(500).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_request_logs(
    facade: State<'_, RequestLogsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("clear_request_logs", async {
        EmptyInputDto::parse(input)?;
        facade
            .clear_request_logs()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_keys(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<StationKeyDto>, error::CommandError> {
    correlation::in_command_scope("list_station_keys", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .list_station_keys(input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn create_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<StationKeyDto, error::CommandError> {
    correlation::in_command_scope("create_station_key", async {
        let input = CreateStationKeyInputDto::parse(input)?;
        facade
            .create_station_key(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<StationKeyDto, error::CommandError> {
    correlation::in_command_scope("update_station_key", async {
        let input = UpdateStationKeyInputDto::parse(input)?;
        facade
            .update_station_key(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn save_station_key_with_defaults(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<SaveStationKeyWithDefaultsResultDto, error::CommandError> {
    correlation::in_command_scope("save_station_key_with_defaults", async {
        let input = SaveStationKeyWithDefaultsInputDto::parse(input)?;
        facade
            .save_station_key_with_defaults(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_key_group_binding(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<StationKeyDto, error::CommandError> {
    correlation::in_command_scope("update_station_key_group_binding", async {
        let input = UpdateStationKeyGroupBindingInputDto::parse(input)?;
        facade
            .update_station_key_group_binding(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_station_key", async {
        let input = StationKeyIdInputDto::parse(input)?;
        facade
            .delete_station_key(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reorder_station_keys(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<StationKeyDto>, error::CommandError> {
    correlation::in_command_scope("reorder_station_keys", async {
        let input = ReorderStationKeysInputDto::parse(input)?;
        facade
            .reorder_station_keys(input.station_id, input.key_ids)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_remote_key_capability(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<RemoteKeyCapabilityDto, error::CommandError> {
    correlation::in_command_scope("get_remote_key_capability", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .get_remote_key_capability(input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_remote_station_keys(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<RemoteStationKeyDto>, error::CommandError> {
    correlation::in_command_scope("list_remote_station_keys", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .list_remote_station_keys(input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn scan_remote_station_keys(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<RemoteKeyScanResultDto, error::CommandError> {
    correlation::in_command_scope("scan_remote_station_keys", async {
        let input = StationIdInputDto::parse(input)?;
        let source = collectors::V2CollectorSourceAdapter::new(
            services.collectors.clone(),
            services.credentials.clone(),
            services.settings.clone(),
        );
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            remote_keys::prepare_remote_key_scan_v2(&source, input.station_id)
        })
        .await
        .map_err(|_| error::CommandError::internal(None))?
        .map_err(public_remote_key_error)?;
        remote_keys::finish_remote_key_scan_v2(services.credentials.as_ref(), prepared)
            .await
            .map_err(public_remote_key_error)
    })
    .await
}

#[tauri::command]
pub async fn create_remote_station_key(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<CreateRemoteStationKeyResultDto, error::CommandError> {
    correlation::in_command_scope("create_remote_station_key", async {
        let input = CreateRemoteStationKeyInputDto::parse(input)?;
        let source = collectors::V2CollectorSourceAdapter::new(
            services.collectors.clone(),
            services.credentials.clone(),
            services.settings.clone(),
        );
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            remote_keys::prepare_remote_key_creation_v2(&source, input)
        })
        .await
        .map_err(|_| error::CommandError::internal(None))?
        .map_err(public_remote_key_error)?;
        remote_keys::finish_remote_key_creation_v2(services.credentials.as_ref(), prepared)
            .await
            .map_err(public_remote_key_error)
    })
    .await
}

#[tauri::command]
pub async fn create_local_station_key_from_remote(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<CreateLocalStationKeyFromRemoteResultDto, error::CommandError> {
    correlation::in_command_scope("create_local_station_key_from_remote", async {
        let input = RemoteStationKeyInputDto::parse(input)?;
        let source = collectors::V2CollectorSourceAdapter::new(
            services.collectors.clone(),
            services.credentials.clone(),
            services.settings.clone(),
        );
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            remote_keys::prepare_local_key_from_remote_v2(
                &source,
                input.station_id,
                input.remote_key_id,
            )
        })
        .await
        .map_err(|_| error::CommandError::internal(None))?
        .map_err(public_remote_key_error)?;
        remote_keys::finish_local_key_from_remote_v2(services.credentials.as_ref(), prepared)
            .await
            .map_err(public_remote_key_error)
    })
    .await
}

#[tauri::command]
pub async fn bind_remote_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<RemoteStationKeyDto>, error::CommandError> {
    correlation::in_command_scope("bind_remote_station_key", async {
        let input = BindRemoteStationKeyInputDto::parse(input)?;
        facade
            .bind_remote_station_key(input.remote_key_id, input.station_key_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn unbind_remote_station_key(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<RemoteStationKeyDto>, error::CommandError> {
    correlation::in_command_scope("unbind_remote_station_key", async {
        let input = RemoteStationKeyInputDto::parse(input)?;
        facade
            .unbind_remote_station_key(input.remote_key_id, input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_key_pool_items(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<KeyPoolItemDto>, error::CommandError> {
    correlation::in_command_scope("list_key_pool_items", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_key_pool_items()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reorder_key_pool(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<Vec<KeyPoolItemDto>, error::CommandError> {
    correlation::in_command_scope("reorder_key_pool", async {
        let input = ReorderKeyPoolInputDto::parse(input)?;
        facade
            .reorder_key_pool(input.key_ids)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_station_key_capabilities(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<StationKeyCapabilitiesDto, error::CommandError> {
    correlation::in_command_scope("get_station_key_capabilities", async {
        let input = RoutingStationKeyIdInputDto::parse(input)?;
        facade
            .get_station_key_capabilities(input.station_key_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_key_capabilities(
    facade: State<'_, KeyPoolCommandFacade>,
    input: Value,
) -> Result<StationKeyCapabilitiesDto, error::CommandError> {
    correlation::in_command_scope("update_station_key_capabilities", async {
        let input = UpdateStationKeyCapabilitiesInputDto::parse(input)?.into_domain();
        facade
            .update_station_key_capabilities(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_model_aliases(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<Vec<ModelAliasDto>, error::CommandError> {
    correlation::in_command_scope("list_model_aliases", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_model_aliases()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_model_alias(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<ModelAliasDto, error::CommandError> {
    correlation::in_command_scope("upsert_model_alias", async {
        let input = UpsertModelAliasInputDto::parse(input)?.into_domain();
        facade
            .upsert_model_alias(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_model_alias(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_model_alias", async {
        let input = DeleteModelAliasInputDto::parse(input)?;
        facade
            .delete_model_alias(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_key_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<Vec<StationKeyHealthDto>, error::CommandError> {
    correlation::in_command_scope("list_station_key_health", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_station_key_health()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_endpoint_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<Vec<StationEndpointHealthDto>, error::CommandError> {
    correlation::in_command_scope("list_station_endpoint_health", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_station_endpoint_health()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitors(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitors", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_channel_monitors(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_summaries(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorSummaryDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_summaries", async {
        let input = ChannelMonitorSummaryInputDto::parse(input)?;
        facade
            .list_channel_monitor_summaries(input.run_since.as_deref(), input.run_limit)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_status_summaries(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<ChannelStatusSummaryDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_status_summaries", async {
        EmptyInputDto::parse(input)?;
        services
            .channel_status
            .load(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_channel_status_workspace(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<ChannelStatusWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("load_channel_status_workspace", async {
        EmptyInputDto::parse(input)?;
        services
            .channel_status
            .load_workspace(PageLimit::new(200).expect("bounded limit"))
            .await
            .map(ChannelStatusWorkspaceDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn load_pricing_comparison_workspace(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<PricingComparisonWorkspaceDto, error::CommandError> {
    correlation::in_command_scope("load_pricing_comparison_workspace", async {
        EmptyInputDto::parse(input)?;
        services
            .pricing_comparison
            .load(PageLimit::new(500).expect("bounded limit"))
            .await
            .map(PricingComparisonWorkspaceDto::from)
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn create_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorDto, error::CommandError> {
    correlation::in_command_scope("create_channel_monitor", async {
        let input = CreateChannelMonitorInputDto::parse(input)?.into_domain();
        facade
            .create_channel_monitor(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorDto, error::CommandError> {
    correlation::in_command_scope("update_channel_monitor", async {
        let input = UpdateChannelMonitorInputDto::parse(input)?.into_domain();
        facade
            .update_channel_monitor(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_channel_monitor(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_channel_monitor", async {
        let input = ChannelMonitorMutationIdInputDto::parse(input)?;
        facade
            .delete_channel_monitor(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_runs(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorRunDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_runs", async {
        let input = ChannelMonitorIdInputDto::parse(input)?;
        facade
            .list_channel_monitor_runs(
                &input.monitor_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_channel_monitor_templates(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<Vec<ChannelMonitorRequestTemplateDto>, error::CommandError> {
    correlation::in_command_scope("list_channel_monitor_templates", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_channel_monitor_templates(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn create_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope("create_channel_monitor_template", async {
        let input = CreateChannelMonitorTemplateInputDto::parse(input)?.into_domain();
        facade
            .create_channel_monitor_template(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope("update_channel_monitor_template", async {
        let input = UpdateChannelMonitorTemplateInputDto::parse(input)?.into_domain();
        facade
            .update_channel_monitor_template(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn duplicate_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<ChannelMonitorRequestTemplateDto, error::CommandError> {
    correlation::in_command_scope("duplicate_channel_monitor_template", async {
        let input = ChannelMonitorMutationIdInputDto::parse(input)?;
        facade
            .duplicate_channel_monitor_template(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_channel_monitor_template(
    facade: State<'_, ChannelMonitoringCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_channel_monitor_template", async {
        let input = ChannelMonitorMutationIdInputDto::parse(input)?;
        facade
            .delete_channel_monitor_template(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn run_channel_monitor_now(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<ChannelMonitorRunDto>, error::CommandError> {
    correlation::in_command_scope("run_channel_monitor_now", async {
        let input = ChannelMonitorIdInputDto::parse(input)?;
        crate::services::channel_monitors::v2_runner_port(services.inner())
            .run_monitor(input.monitor_id)
            .await
            .map_err(public_channel_monitor_run_error)
    })
    .await
}

fn public_channel_monitor_run_error(_: String) -> error::CommandError {
    error::CommandError::from_work(error::WorkFailure::ResultUnknown)
}

#[tauri::command]
pub async fn get_station_key_health(
    facade: State<'_, RoutingCommandFacade>,
    input: Value,
) -> Result<StationKeyHealthDto, error::CommandError> {
    correlation::in_command_scope("get_station_key_health", async {
        let input = RoutingStationKeyIdInputDto::parse(input)?;
        facade
            .get_station_key_health(input.station_key_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn ping_station_endpoint(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<EndpointPingResultDto, error::CommandError> {
    correlation::in_command_scope("ping_station_endpoint", async {
        let input = StationIdInputDto::parse(input)?;
        let target = services
            .routing
            .station_endpoint_probe_target(&input.station_id)
            .await
            .map_err(public_command_application_error)?;
        let checked_at = now_millis_for_services().to_string();
        let api_base_url = target.api_base_url.clone();
        let probe = tauri::async_runtime::spawn_blocking(move || {
            probe_station_endpoint(&api_base_url, Duration::from_secs(5))
        })
        .await
        .map_err(|_| error::CommandError::from_work(error::WorkFailure::ResultUnknown))?;
        let health = services
            .routing
            .record_station_endpoint_health(
                target.station_id,
                target.endpoint_revision,
                probe.status,
                probe.latency_ms,
                checked_at.clone(),
                probe.error_summary,
            )
            .await
            .map_err(|_| error::CommandError::from_work(error::WorkFailure::ResultUnknown))?;
        EndpointPingResultDto::try_from(EndpointPingResult {
            station_id: health.station_id,
            ok: probe.ok,
            status: health.status,
            latency_ms: health.latency_ms,
            checked_at: health.checked_at.unwrap_or(checked_at),
            error_summary: health.error_summary,
        })
        .map_err(|_| error::CommandError::from_work(error::WorkFailure::ResultUnknown))
    })
    .await
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyConnectivityTestResult {
    station_key_id: String,
    ok: bool,
    status_code: u16,
    duration_ms: i64,
    model: String,
    message: String,
    response_mode: StationKeyConnectivityResponseMode,
    stream_fallback_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StationKeyConnectivityProbeKind {
    Responses,
    ChatCompletions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StationKeyConnectivityRequestMode {
    Stream,
    NonStream,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StationKeyConnectivityResponseMode {
    Stream,
    NonStreamFallback,
}

const STATION_KEY_CONNECTIVITY_EVENT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StationKeyConnectivityTestEvent {
    schema_version: u32,
    run_id: String,
    sequence: u64,
    terminal: bool,
    cancel_capability: StationKeyConnectivityCancelCapability,
    event: StationKeyConnectivityTestEventPayload,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StationKeyConnectivityCancelCapability {
    DetachOnly,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum StationKeyConnectivityTestEventPayload {
    AttemptStarted { model: String, protocol: String },
    Delta { text: String },
    Fallback { reason: String },
    Completed { ok: bool },
    Failed { message: String },
}

struct StationKeyConnectivityProgress {
    run_id: String,
    sequence: u64,
    progress: Channel<StationKeyConnectivityTestEvent>,
    terminal_sent: bool,
}

impl StationKeyConnectivityProgress {
    fn new(progress: Channel<StationKeyConnectivityTestEvent>) -> Self {
        Self {
            run_id: uuid::Uuid::now_v7().to_string(),
            sequence: 0,
            progress,
            terminal_sent: false,
        }
    }

    fn emit(&mut self, event: StationKeyConnectivityTestEventPayload, terminal: bool) {
        if self.terminal_sent {
            return;
        }
        let envelope = station_key_connectivity_event_envelope(
            self.run_id.clone(),
            self.sequence,
            terminal,
            event,
        );
        self.sequence = self.sequence.saturating_add(1);
        if terminal {
            self.terminal_sent = true;
        }
        let _ = self.progress.send(envelope);
    }

    fn emit_terminal(&mut self, result: &StationKeyConnectivityProbeResult) {
        if result.ok {
            self.emit(
                StationKeyConnectivityTestEventPayload::Completed { ok: true },
                true,
            );
        } else {
            self.emit(
                StationKeyConnectivityTestEventPayload::Failed {
                    message: result.message.clone(),
                },
                true,
            );
        }
    }
}

fn station_key_connectivity_event_envelope(
    run_id: String,
    sequence: u64,
    terminal: bool,
    event: StationKeyConnectivityTestEventPayload,
) -> StationKeyConnectivityTestEvent {
    StationKeyConnectivityTestEvent {
        schema_version: STATION_KEY_CONNECTIVITY_EVENT_SCHEMA_VERSION,
        run_id,
        sequence,
        terminal,
        cancel_capability: StationKeyConnectivityCancelCapability::DetachOnly,
        event,
    }
}

#[derive(Debug, Clone)]
struct StationKeyConnectivityProbeResult {
    ok: bool,
    status_code: u16,
    duration_ms: i64,
    message: String,
    response_mode: StationKeyConnectivityResponseMode,
    stream_fallback_reason: Option<String>,
}

impl StationKeyConnectivityProbeResult {
    fn success(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: true,
            status_code,
            duration_ms,
            message,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    fn failure(status_code: u16, duration_ms: i64, message: String) -> Self {
        Self {
            ok: false,
            status_code,
            duration_ms,
            message,
            response_mode: StationKeyConnectivityResponseMode::Stream,
            stream_fallback_reason: None,
        }
    }

    fn with_response_mode(mut self, response_mode: StationKeyConnectivityResponseMode) -> Self {
        self.response_mode = response_mode;
        self
    }

    fn with_stream_fallback_reason(mut self, reason: Option<String>) -> Self {
        self.stream_fallback_reason = reason;
        self
    }
}

#[tauri::command]
pub async fn test_station_key_connectivity(
    services: State<'_, AppServices>,
    input: Value,
    progress: Channel<StationKeyConnectivityTestEvent>,
) -> Result<StationKeyConnectivityTestResult, error::CommandError> {
    correlation::in_command_scope("test_station_key_connectivity", async {
        let input = StationKeyConnectivityInputDto::parse(input)?;
        let station_key_id = input.station_key_id;
        let model = input.model;
        let key = services
            .credentials
            .list_key_pool_items()
            .await
            .map_err(command_application_error)?
            .into_iter()
            .find(|item| item.id == station_key_id)
            .ok_or_else(|| "Station Key does not exist".to_string())?;
        if !key.api_key_present {
            return Err("Station Key does not have a saved API key"
                .to_string()
                .into());
        }
        let secret = services
            .credentials
            .resolve_station_key_secret(station_key_id.clone())
            .await
            .map_err(command_application_error)?;
        let api_key = String::from_utf8(secret.as_bytes().to_vec())
            .map(zeroize::Zeroizing::new)
            .map_err(|_| "Station Key API key is not valid UTF-8".to_string())?;
        let capabilities = services
            .credentials
            .get_station_key_capabilities(station_key_id.clone())
            .await
            .map_err(command_application_error)?;
        let station_id = key.station_id.clone();
        let endpoint_revision = key.station_endpoint_revision;
        let result = tauri::async_runtime::spawn_blocking(move || {
            test_station_key_connectivity_prepared_blocking(
                key,
                api_key,
                capabilities,
                model,
                progress,
            )
        })
        .await
        .map_err(|error| format!("测试密钥连通性任务失败: {error}"))??;
        services
            .routing
            .record_station_key_connectivity(
                station_key_id,
                station_id,
                endpoint_revision,
                result.ok,
                result.duration_ms,
                result.message.clone(),
            )
            .await
            .map_err(command_application_error)?;
        Ok(result)
    })
    .await
}

#[tauri::command]
pub async fn simulate_route(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<RouteSimulationResultDto, error::CommandError> {
    correlation::in_command_scope("simulate_route", async {
        let input = RouteSimulationInputDto::parse(input)?.into_domain();
        services
            .routing
            .simulate_route(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_pricing_rules(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<PricingRuleDto>, error::CommandError> {
    correlation::in_command_scope("list_pricing_rules", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_pricing_rules(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_model_base_prices(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<ModelBasePriceDto>, error::CommandError> {
    correlation::in_command_scope("list_model_base_prices", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_model_base_prices(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_model_base_price(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<ModelBasePriceDto, error::CommandError> {
    correlation::in_command_scope("upsert_model_base_price", async {
        let input = UpsertModelBasePriceInputDto::parse(input)?.into_domain();
        facade
            .upsert_model_base_price(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn reset_model_base_prices_to_builtins(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<ModelBasePriceDto>, error::CommandError> {
    correlation::in_command_scope("reset_model_base_prices_to_builtins", async {
        EmptyInputDto::parse(input)?;
        facade
            .reset_model_base_prices_to_builtins(PageLimit::new(500).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_pricing_rule(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<PricingRuleDto, error::CommandError> {
    correlation::in_command_scope("upsert_pricing_rule", async {
        let input = UpsertPricingRuleInputDto::parse(input)?.into_domain();
        facade
            .upsert_pricing_rule(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn delete_pricing_rule(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("delete_pricing_rule", async {
        let input = PricingRuleIdInputDto::parse(input)?;
        facade
            .delete_pricing_rule(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn resolve_station_key_pricing_context(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<ResolvedPricingContextDto, error::CommandError> {
    correlation::in_command_scope("resolve_station_key_pricing_context", async {
        let (station_key_id, requested_model, request_kind) =
            PricingContextInputDto::parse(input)?.into_parts();
        facade
            .resolve_station_key_pricing_context(&station_key_id, &requested_model, request_kind)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_balance_snapshots(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_balance_snapshots", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_balance_snapshots(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_current_station_balance_snapshots(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_current_station_balance_snapshots", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_balance_snapshots(PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_balance_snapshots_for_station(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<BalanceSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_balance_snapshots_for_station", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        services
            .routing
            .list_balance_snapshots_for_station(&input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_balance_snapshot(
    facade: State<'_, PricingCommandFacade>,
    input: Value,
) -> Result<BalanceSnapshotDto, error::CommandError> {
    correlation::in_command_scope("upsert_balance_snapshot", async {
        let input = UpsertBalanceSnapshotInputDto::parse(input)?.into_domain();
        facade
            .upsert_balance_snapshot(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_group_bindings(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<StationGroupBindingDto>, error::CommandError> {
    correlation::in_command_scope("list_station_group_bindings", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        services
            .collectors
            .list_station_group_bindings(&input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_station_group_options(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<StationGroupOptionDto>, error::CommandError> {
    correlation::in_command_scope("list_station_group_options", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        services
            .collectors
            .list_station_group_options(
                &input.station_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_station_group_binding(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<StationGroupBindingDto, error::CommandError> {
    correlation::in_command_scope("upsert_station_group_binding", async {
        let input = UpsertStationGroupBindingInputDto::parse(input)?.into_domain();
        services
            .collectors
            .upsert_station_group_binding(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_group_rate_records(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<GroupRateRecordDto>, error::CommandError> {
    correlation::in_command_scope("list_group_rate_records", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        services
            .collectors
            .list_group_rate_records(
                &input.station_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_collector_runs(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<CollectorRunDto>, error::CommandError> {
    correlation::in_command_scope("list_collector_runs", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        services
            .collectors
            .list_collector_runs(
                &input.station_id,
                PageLimit::new(500).expect("bounded limit"),
            )
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_change_events(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<Vec<ChangeEventDto>, error::CommandError> {
    correlation::in_command_scope("list_change_events", async {
        EmptyInputDto::parse(input)?;
        facade
            .list_change_events(None, PageLimit::new(200).expect("bounded limit"))
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_change_events(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<(), error::CommandError> {
    correlation::in_command_scope("clear_change_events", async {
        EmptyInputDto::parse(input)?;
        facade
            .clear_change_events()
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn list_change_events_for_station(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<Vec<ChangeEventDto>, error::CommandError> {
    correlation::in_command_scope("list_change_events_for_station", async {
        let input = ChangeLogStationIdInputDto::parse(input)?;
        facade
            .list_change_events(
                Some(&input.station_id),
                PageLimit::new(200).expect("bounded limit"),
            )
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn upsert_change_event(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("upsert_change_event", async {
        let input = UpsertChangeEventInputDto::parse(input)?.into_domain();
        facade
            .upsert_change_event(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn mark_change_event_read(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("mark_change_event_read", async {
        let input = ChangeEventIdInputDto::parse(input)?;
        facade
            .mark_change_event_read(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn mark_change_events_read(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<Vec<ChangeEventDto>, error::CommandError> {
    correlation::in_command_scope("mark_change_events_read", async {
        let input = ChangeEventIdsInputDto::parse(input)?;
        facade
            .mark_change_events_read(input.ids)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn dismiss_change_event(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("dismiss_change_event", async {
        let input = ChangeEventIdInputDto::parse(input)?;
        facade
            .dismiss_change_event(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn resolve_change_event(
    facade: State<'_, ChangeEventsCommandFacade>,
    input: Value,
) -> Result<ChangeEventDto, error::CommandError> {
    correlation::in_command_scope("resolve_change_event", async {
        let input = ChangeEventIdInputDto::parse(input)?;
        facade
            .resolve_change_event(input.id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_station_credentials(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("get_station_credentials", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .get_station_credentials(input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_credentials(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("update_station_credentials", async {
        let input = UpdateStationCredentialsInputDto::parse(input)?;
        facade
            .update_station_credentials(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn update_station_session(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("update_station_session", async {
        let input = UpdateStationSessionInputDto::parse(input)?;
        facade
            .update_station_session(input)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn clear_station_credentials(
    facade: State<'_, CredentialsCommandFacade>,
    input: Value,
) -> Result<StationCredentialsDto, error::CommandError> {
    correlation::in_command_scope("clear_station_credentials", async {
        let input = StationIdInputDto::parse(input)?;
        facade
            .clear_station_credentials(input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn detect_sub2api_station(
    services: State<'_, AppServices>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("detect_sub2api_station", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        run_station_collection_v2(
            services.inner(),
            *secrets.data_key(),
            input.station_id,
            collectors::adapters::CollectorTask::Detect,
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn collect_sub2api_station(
    services: State<'_, AppServices>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("collect_sub2api_station", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        run_station_collection_v2(
            services.inner(),
            *secrets.data_key(),
            input.station_id,
            collectors::adapters::CollectorTask::Full,
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn detect_station_info(
    services: State<'_, AppServices>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("detect_station_info", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        run_station_collection_v2(
            services.inner(),
            *secrets.data_key(),
            input.station_id,
            collectors::adapters::CollectorTask::Detect,
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn collect_station_info(
    services: State<'_, AppServices>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("collect_station_info", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        run_station_collection_v2(
            services.inner(),
            *secrets.data_key(),
            input.station_id,
            collectors::adapters::CollectorTask::Full,
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn collect_station_task(
    services: State<'_, AppServices>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("collect_station_task", async {
        let input = StationCollectorTaskInputDto::parse(input)?;
        let task = match input.task_type {
            StationCollectorTaskTypeDto::Detect => collectors::adapters::CollectorTask::Detect,
            StationCollectorTaskTypeDto::Balance => collectors::adapters::CollectorTask::Balance,
            StationCollectorTaskTypeDto::Groups => collectors::adapters::CollectorTask::Groups,
            StationCollectorTaskTypeDto::Models => collectors::adapters::CollectorTask::Models,
            StationCollectorTaskTypeDto::Full => collectors::adapters::CollectorTask::Full,
        };
        run_station_collection_v2(
            services.inner(),
            *secrets.data_key(),
            input.station_id,
            task,
        )
        .await
    })
    .await
}

#[tauri::command]
pub async fn test_station_login(
    services: State<'_, AppServices>,
    secrets: State<'_, SecretManager>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("test_station_login", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        let data_key = *secrets.data_key();
        let source = collectors::V2CollectorSourceAdapter::new(
            services.collectors.clone(),
            services.credentials.clone(),
            services.settings.clone(),
        );
        let prepared = tauri::async_runtime::spawn_blocking(move || {
            collectors::prepare_station_login_test_v2(&source, &data_key, input.station_id)
        })
        .await
        .map_err(|_| error::CommandError::internal(None))?
        .map_err(public_command_application_error)?;
        apply_prepared_collection_v2(services.inner(), prepared).await
    })
    .await
}

#[tauri::command]
pub async fn test_station_login_input(
    input: Value,
) -> Result<StationLoginTestResultDto, error::CommandError> {
    correlation::in_command_scope("test_station_login_input", async {
        let input = StationLoginTestInputDto::parse(input)?.into_domain();
        tauri::async_runtime::spawn_blocking(move || collectors::test_station_login_input(input))
            .await
            .map_err(|_| error::CommandError::internal(None))?
            .map_err(public_station_login_probe_error)
    })
    .await
}

fn public_station_login_probe_error(_: String) -> error::CommandError {
    error::CommandError::from_driver(error::DriverFailure::ExternalUnavailable {
        provider: None,
        upstream_status: None,
    })
}

async fn run_station_collection_v2(
    services: &AppServices,
    data_key: [u8; 32],
    station_id: String,
    task: collectors::adapters::CollectorTask,
) -> Result<CollectorRunResultDto, error::CommandError> {
    let source = collectors::V2CollectorSourceAdapter::new(
        services.collectors.clone(),
        services.credentials.clone(),
        services.settings.clone(),
    );
    let prepared = tauri::async_runtime::spawn_blocking(move || {
        collectors::prepare_station_collection_v2(&source, &data_key, station_id, task)
    })
    .await
    .map_err(|_| error::CommandError::internal(None))?
    .map_err(public_command_application_error)?;
    apply_prepared_collection_v2(services, prepared).await
}

async fn apply_prepared_collection_v2(
    services: &AppServices,
    prepared: collectors::PreparedStationCollection,
) -> Result<CollectorRunResultDto, error::CommandError> {
    let apply = collectors::apply::V2CollectorApplyAdapter::new((*services.collectors).clone());
    collectors::apply_prepared_station_collection_v2(&services.collectors, &apply, prepared)
        .await
        .map_err(command_application_error)
}

#[tauri::command]
pub async fn list_collector_snapshots(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Vec<CollectorSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("list_collector_snapshots", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        let limit = PageLimit::new(100).map_err(public_command_application_error)?;
        services
            .collectors
            .list_station_snapshots(&input.station_id, limit)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn get_latest_collector_snapshot(
    services: State<'_, AppServices>,
    input: Value,
) -> Result<Option<CollectorSnapshotDto>, error::CommandError> {
    correlation::in_command_scope("get_latest_collector_snapshot", async {
        let input = CollectorStationIdInputDto::parse(input)?;
        services
            .collectors
            .latest_station_snapshot(&input.station_id)
            .await
            .map_err(public_command_application_error)
    })
    .await
}

#[tauri::command]
pub async fn start_capture_session(
    app: tauri::AppHandle,
    services: State<'_, AppServices>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("start_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let station_id = input.station_id;
        let station = services
            .stations
            .station_for_capture(&station_id)
            .await
            .map_err(command_application_error)?;
        let credentials = services
            .credentials
            .get_station_credentials(station_id.clone())
            .await
            .map_err(command_application_error)?;
        let login_password_secret = if credentials.password_present {
            services
                .credentials
                .get_station_login_password(station_id.clone())
                .await
                .map_err(command_application_error)?
        } else {
            None
        };
        let login_password = login_password_secret
            .as_ref()
            .map(|secret| {
                std::str::from_utf8(secret.as_bytes())
                    .map_err(|_| "stored station login password is not valid UTF-8".to_string())
            })
            .transpose()?;
        let label = capture_window_label(&station_id);
        let endpoint_revision = station.endpoint_revision;
        let script = capture_script(
            &station_id,
            &label,
            credentials.login_username.as_deref(),
            login_password,
        );
        let app_handle = app.clone();
        let label_for_start = label.clone();
        tauri::async_runtime::spawn_blocking(move || {
            if let Some(window) = app_handle.get_webview_window(&label_for_start) {
                window
                    .set_focus()
                    .map_err(|error| format!("聚焦捕获窗口失败: {error}"))?;
            } else {
                tauri::WebviewWindowBuilder::new(
                    &app_handle,
                    label_for_start.clone(),
                    tauri::WebviewUrl::External(
                        "about:blank"
                            .parse()
                            .map_err(|error| format!("捕获窗口初始化失败: {error}"))?,
                    ),
                )
                .title(format!("网页登录 / 捕获 - {}", station.name))
                .inner_size(1100.0, 760.0)
                .initialization_script(&script)
                .build()
                .map_err(|error| format!("打开网页登录窗口失败: {error}"))?;
                if let Some(window) = app_handle.get_webview_window(&label_for_start) {
                    let target_url = station.website_url.clone();
                    let target = target_url
                        .parse()
                        .map_err(|error| format!("Base URL 无法作为网页登录地址打开: {error}"))?;
                    let navigator = window.clone();
                    window
                        .run_on_main_thread(move || {
                            let _ = navigator.navigate(target);
                        })
                        .map_err(|error| format!("安排捕获窗口导航失败: {error}"))?;
                }
            }
            Ok::<(), error::CommandError>(())
        })
        .await
        .map_err(|error| format!("打开网页登录窗口失败: {error}"))??;
        Ok(sessions.start(station_id, label, endpoint_revision)?)
    })
    .await
}

#[tauri::command]
pub async fn get_capture_session_status(
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("get_capture_session_status", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        Ok(sessions.status(&input.station_id)?)
    })
    .await
}

#[tauri::command]
pub async fn record_capture_event(
    services: State<'_, AppServices>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("record_capture_event", async {
        let input = CapturedHttpEventInputDto::parse(input)?.into_domain();
        let station = services
            .stations
            .station_for_capture(&input.station_id)
            .await
            .map_err(command_application_error)?;
        if !capture_request_belongs_to_station(
            &station.website_url,
            &station.api_base_url,
            &input.request_url,
        ) {
            return Err("捕获事件不属于当前站点 Base URL，已拒绝。"
                .to_string()
                .into());
        }
        let web_authorization_user_id = web_authorization_candidate_user_id_from_input(&input);
        let captured_credentials = capture::extract_session_credentials(&input);
        let station_id = input.station_id.clone();
        let event = capture::sanitize_event(input);
        let receipt = sessions.push_event(&station_id, event, web_authorization_user_id)?;
        if let Some(session) = captured_credentials {
            services
                .credentials
                .persist_station_session_if_revision(session, receipt.endpoint_revision)
                .await
                .map_err(capture_endpoint_revision_error)?;
        }
        Ok(receipt.status)
    })
    .await
}

#[tauri::command]
pub async fn clear_capture_session(
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("clear_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        Ok(sessions.clear(&input.station_id)?)
    })
    .await
}

#[tauri::command]
pub async fn close_capture_session(
    app: tauri::AppHandle,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CaptureSessionStatusDto, error::CommandError> {
    correlation::in_command_scope("close_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let label = capture_window_label(&input.station_id);
        if let Some(window) = app.get_webview_window(&label) {
            window
                .close()
                .map_err(|error| format!("关闭网页登录窗口失败: {error}"))?;
        }
        Ok(sessions.clear(&input.station_id)?)
    })
    .await
}

#[tauri::command]
pub async fn finish_capture_session(
    services: State<'_, AppServices>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("finish_capture_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let station_id = input.station_id;
        let commit = sessions.begin_finish(&station_id)?;
        let result =
            finish_capture_session_with_events(services.inner(), &station_id, &commit, None).await;
        match result {
            Ok(result) => {
                sessions.complete_commit(&station_id, &commit)?;
                Ok(result)
            }
            Err(error) => Err(abort_capture_commit(
                sessions.inner(),
                &station_id,
                &commit,
                error,
            )),
        }
    })
    .await
}

async fn finish_capture_session_with_events(
    services: &AppServices,
    station_id: &str,
    commit: &capture::session::CaptureCommit,
    web_authorization_summary: Option<Value>,
) -> Result<CollectorRunResult, error::CommandError> {
    let events = &commit.events;
    let (mut summary, normalized, raw) = capture::summarize_events(events);
    if let Some(web_authorization_summary) = web_authorization_summary {
        if let Some(summary) = summary.as_object_mut() {
            summary.insert("webAuthorization".to_string(), web_authorization_summary);
        }
    }
    let status = normalized
        .get("status")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("partial")
        .to_string();
    let error_message = if events.is_empty() {
        Some("未捕获到后台接口响应，请确认已在网页登录窗口完成登录并打开后台页面。".to_string())
    } else {
        None
    };
    services
        .collectors
        .record_capture_snapshot(crate::application::collectors::CaptureSnapshotRequest {
            station_id: station_id.to_string(),
            endpoint_revision: commit.endpoint_revision,
            status,
            summary_json: summary,
            normalized_json: normalized,
            raw_json_redacted: Some(raw),
            error_message,
            event_count: events.len() as i64,
        })
        .await
        .map_err(capture_endpoint_revision_error)
}

#[tauri::command]
pub async fn finish_web_authorization_session(
    app: tauri::AppHandle,
    services: State<'_, AppServices>,
    sessions: State<'_, capture::session::CaptureSessionStore>,
    input: Value,
) -> Result<CollectorRunResultDto, error::CommandError> {
    correlation::in_command_scope("finish_web_authorization_session", async {
        let input = CaptureStationIdInputDto::parse(input)?;
        let station_id = input.station_id;
        let station = services
            .stations
            .station_for_capture(&station_id)
            .await
            .map_err(command_application_error)?;
        let candidate = sessions
            .web_authorization_candidate(&station_id)?
            .ok_or_else(|| {
                "网页登录授权尚未捕获到用户身份，请在授权窗口完成登录后重试。".to_string()
            })?;
        let cookie_header =
            read_capture_window_cookie_header(app, &station_id, &station.website_url).await?;
        let verified = capture::web_authorization::verify_newapi_cookie_session(
            &station.website_url,
            &cookie_header,
            &candidate.user_id,
            Duration::from_secs(20),
        )?;
        let commit = sessions.begin_web_authorization_commit(&station_id, &candidate)?;
        let persist_result = services
            .credentials
            .persist_station_session_if_revision(
                PersistStationSessionInput {
                    station_id: station_id.clone(),
                    access_token: None,
                    refresh_token: None,
                    cookie: Some(verified.cookie_header),
                    newapi_user_id: Some(verified.newapi_user_id),
                    token_expires_at: None,
                    session_expires_at: None,
                    session_source: verified.session_source,
                },
                commit.endpoint_revision,
            )
            .await
            .map_err(capture_endpoint_revision_error);
        if let Err(error) = persist_result {
            return Err(abort_capture_commit(
                sessions.inner(),
                &station_id,
                &commit,
                error,
            ));
        }

        let result = finish_capture_session_with_events(
            services.inner(),
            &station_id,
            &commit,
            Some(capture::web_authorization_summary(
                "success",
                Some("web_authorization"),
                true,
            )),
        )
        .await;
        match result {
            Ok(result) => {
                sessions.complete_commit(&station_id, &commit)?;
                Ok(result)
            }
            Err(error) => Err(abort_capture_commit(
                sessions.inner(),
                &station_id,
                &commit,
                error,
            )),
        }
    })
    .await
}

fn abort_capture_commit(
    sessions: &capture::session::CaptureSessionStore,
    station_id: &str,
    commit: &capture::session::CaptureCommit,
    persistence_error: error::CommandError,
) -> error::CommandError {
    match sessions.abort_commit(station_id, commit) {
        Ok(()) => persistence_error,
        Err(_) => error::CommandError::internal(None),
    }
}

fn capture_window_label(station_id: &str) -> String {
    format!(
        "capture-{}",
        station_id.replace(|character: char| !character.is_ascii_alphanumeric(), "-")
    )
}

async fn read_capture_window_cookie_header(
    app: tauri::AppHandle,
    station_id: &str,
    station_website_url: &str,
) -> Result<String, error::CommandError> {
    let label = capture_window_label(station_id);
    let window = app
        .get_webview_window(&label)
        .ok_or_else(|| "网页登录授权窗口不存在，请重新打开授权窗口。".to_string())?;
    let target = tauri::Url::parse(station_website_url)
        .map_err(|error| format!("站点管理地址无法用于读取 Cookie: {error}"))?;

    let cookies = tauri::async_runtime::spawn_blocking(move || window.cookies_for_url(target))
        .await
        .map_err(|error| format!("读取网页登录授权 Cookie 任务失败: {error}"))?
        .map_err(|error| format!("读取网页登录授权 Cookie 失败: {error}"))?;

    let pairs = cookies
        .into_iter()
        .map(|cookie| (cookie.name().to_string(), cookie.value().to_string()))
        .collect::<Vec<_>>();
    Ok(
        capture::web_authorization::build_cookie_header_from_pairs(&pairs).ok_or_else(|| {
            "网页登录授权未捕获到可用 Cookie，请确认已在授权窗口完成登录。".to_string()
        })?,
    )
}

fn capture_request_belongs_to_station(
    station_website_url: &str,
    station_api_base_url: &str,
    request_url: &str,
) -> bool {
    [station_website_url, station_api_base_url]
        .into_iter()
        .any(|base_url| url_belongs_to_base(request_url, base_url))
}

fn capture_endpoint_revision_error(error: ApplicationError) -> error::CommandError {
    if matches!(error, ApplicationError::StaleRevision) {
        command_application_error(ApplicationError::StaleRevision)
    } else {
        command_application_error(error)
    }
}

fn web_authorization_candidate_user_id_from_input(
    input: &CapturedHttpEventInput,
) -> Option<String> {
    let fallback_path;
    let request_path = if let Some(path) = input.request_path.as_deref() {
        path
    } else {
        fallback_path = path_from_request_url(&input.request_url);
        &fallback_path
    };
    if !capture::web_authorization::is_newapi_completion_candidate(
        request_path,
        input.status,
        input.response_json.as_ref(),
    ) {
        return None;
    }
    input
        .response_json
        .as_ref()
        .and_then(capture::web_authorization::extract_verified_user_id)
}

fn path_from_request_url(url: &str) -> String {
    let without_scheme = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let path = without_scheme
        .find('/')
        .map(|index| &without_scheme[index..])
        .unwrap_or("/");
    path.split(['?', '#']).next().unwrap_or("/").to_string()
}

fn capture_script(
    station_id: &str,
    window_label: &str,
    login_username: Option<&str>,
    login_password: Option<&str>,
) -> String {
    let login_username_json =
        serde_json::to_string(&login_username).unwrap_or_else(|_| "null".to_string());
    let login_password_json =
        serde_json::to_string(&login_password).unwrap_or_else(|_| "null".to_string());
    format!(
        r#"
(() => {{
  if (window.__relayPoolCaptureInstalled) return;
  window.__relayPoolCaptureInstalled = true;
  const stationId = {station_id:?};
  const sourceWindowId = {window_label:?};
  const loginUsername = {login_username_json};
  const loginPassword = {login_password_json};
  const limit = 4000;
  const invoke = (window.__TAURI_INTERNALS__ && window.__TAURI_INTERNALS__.invoke)
    ? window.__TAURI_INTERNALS__.invoke
    : null;
  const pathFromUrl = (url) => {{
    try {{ return new URL(url, window.location.href).pathname || "/"; }}
    catch (_) {{ return "/"; }}
  }};
  const contentTypeOf = (headers) => {{
    try {{ return headers && headers.get ? (headers.get("content-type") || "") : ""; }}
    catch (_) {{ return ""; }}
  }};
  const tryFinishWebAuthorization = (status) => {{
    if (!invoke || !status || !status.webAuthorizationCandidate) return;
    if (window.__relayPoolAuthorizationFinishInFlight) return;
    window.__relayPoolAuthorizationFinishInFlight = true;
    invoke("finish_web_authorization_session", {{ stationId }})
      .catch(() => undefined)
      .finally(() => {{
        window.__relayPoolAuthorizationFinishInFlight = false;
      }});
  }};
  const send = (input) => {{
    if (!invoke) return;
    invoke("record_capture_event", {{ input }})
      .then(tryFinishWebAuthorization)
      .catch(() => undefined);
  }};
  const buildBase = (url, method, startedAt) => ({{
    stationId,
    sourceWindowId,
    pageUrl: window.location.href,
    requestUrl: String(new URL(url, window.location.href)),
    requestPath: pathFromUrl(url),
    method,
    startedAt,
  }});
  const setNativeValue = (element, value) => {{
    if (!element || value == null || element.value === value) return false;
    const prototype = Object.getPrototypeOf(element);
    const descriptor = prototype ? Object.getOwnPropertyDescriptor(prototype, "value") : null;
    if (descriptor && descriptor.set) descriptor.set.call(element, value);
    else element.value = value;
    element.dispatchEvent(new Event("input", {{ bubbles: true }}));
    element.dispatchEvent(new Event("change", {{ bubbles: true }}));
    return true;
  }};
  const candidateInput = (selectors) => {{
    for (const selector of selectors) {{
      const found = document.querySelector(selector);
      if (found && !found.disabled && !found.readOnly) return found;
    }}
    return null;
  }};
  const fillLoginForm = () => {{
    try {{
      setNativeValue(candidateInput([
        "input[type='email']",
        "input[name='email']",
        "input[name='username']",
        "input[name='user']",
        "input[autocomplete='username']",
        "input[placeholder*='邮箱']",
        "input[placeholder*='账号']",
        "input[placeholder*='email' i]",
      ]), loginUsername);
      setNativeValue(candidateInput([
        "input[type='password']",
        "input[name='password']",
        "input[autocomplete='current-password']",
        "input[placeholder*='密码']",
        "input[placeholder*='password' i]",
      ]), loginPassword);
      for (const checkbox of Array.from(document.querySelectorAll("input[type='checkbox']"))) {{
        const label = checkbox.closest("label") || (checkbox.id ? document.querySelector(`label[for="${{checkbox.id}}"]`) : null);
        const text = `${{checkbox.name || ""}} ${{checkbox.id || ""}} ${{label ? label.textContent || "" : ""}}`.toLowerCase();
        if (text.includes("agreement") || text.includes("attestation") || text.includes("region") || text.includes("大陆") || text.includes("中华人民共和国") || text.includes("独立陈述")) {{
          if (!checkbox.checked) {{
            checkbox.checked = true;
            checkbox.dispatchEvent(new Event("input", {{ bubbles: true }}));
            checkbox.dispatchEvent(new Event("change", {{ bubbles: true }}));
          }}
        }}
      }}
    }} catch (_) {{}}
  }};
  fillLoginForm();
  const fillTimer = window.setInterval(fillLoginForm, 800);
  window.setTimeout(() => window.clearInterval(fillTimer), 15000);
  try {{
    new MutationObserver(fillLoginForm).observe(document.documentElement, {{ childList: true, subtree: true }});
  }} catch (_) {{}}
  const originalFetch = window.fetch;
  window.fetch = async function(input, init) {{
    const url = typeof input === "string" ? input : (input && input.url) || String(input);
    const method = (init && init.method) || (input && input.method) || "GET";
    const startedAt = new Date().toISOString();
    const started = performance.now();
    try {{
      const response = await originalFetch.apply(this, arguments);
      const clone = response.clone();
      const contentType = contentTypeOf(response.headers);
      const base = buildBase(url, method, startedAt);
      if (contentType.includes("json")) {{
        clone.json().then((json) => send({{
          ...base,
          status: response.status,
          contentType,
          finishedAt: new Date().toISOString(),
          durationMs: Math.round(performance.now() - started),
          responseKind: "json",
          responseJson: json,
          responseSize: JSON.stringify(json).length,
        }})).catch(() => undefined);
      }} else {{
        clone.text().then((text) => send({{
          ...base,
          status: response.status,
          contentType,
          finishedAt: new Date().toISOString(),
          durationMs: Math.round(performance.now() - started),
          responseKind: contentType.includes("html") ? "html" : "text",
          responseText: text.slice(0, limit),
          responseSize: text.length,
        }})).catch(() => undefined);
      }}
      return response;
    }} catch (error) {{
      send({{
        ...buildBase(url, method, startedAt),
        finishedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - started),
        responseKind: "error",
        errorMessage: error && error.message ? error.message : String(error),
      }});
      throw error;
    }}
  }};
  const originalOpen = XMLHttpRequest.prototype.open;
  const originalSend = XMLHttpRequest.prototype.send;
  XMLHttpRequest.prototype.open = function(method, url) {{
    this.__relayPoolCapture = {{ method: method || "GET", url: String(url), startedAt: new Date().toISOString(), started: performance.now() }};
    return originalOpen.apply(this, arguments);
  }};
  XMLHttpRequest.prototype.send = function() {{
    this.addEventListener("loadend", function() {{
      const meta = this.__relayPoolCapture;
      if (!meta) return;
      const contentType = this.getResponseHeader("content-type") || "";
      let responseText = "";
      try {{ responseText = typeof this.responseText === "string" ? this.responseText : ""; }} catch (_) {{}}
      let responseJson = null;
      if (contentType.includes("json") && responseText) {{
        try {{ responseJson = JSON.parse(responseText); }} catch (_) {{}}
      }}
      send({{
        ...buildBase(meta.url, meta.method, meta.startedAt),
        status: this.status,
        contentType,
        finishedAt: new Date().toISOString(),
        durationMs: Math.round(performance.now() - meta.started),
        responseKind: responseJson ? "json" : (contentType.includes("html") ? "html" : "text"),
        responseJson,
        responseText: responseJson ? null : responseText.slice(0, limit),
        responseSize: responseText.length,
      }});
    }});
    return originalSend.apply(this, arguments);
  }};
}})();
"#
    )
}

fn build_ccswitch_provider_deeplink(
    app: &str,
    provider_name: &str,
    homepage: &str,
    endpoint: &str,
    api_key: &str,
) -> String {
    let usage_script = general_purpose::STANDARD.encode(build_ccswitch_usage_script());
    let mut entries = vec![
        ("resource", "provider".to_string()),
        ("app", app.to_string()),
        ("name", provider_name.to_string()),
        ("homepage", homepage.to_string()),
        ("endpoint", endpoint.to_string()),
        ("apiKey", api_key.to_string()),
        ("configFormat", "json".to_string()),
        ("usageEnabled", "true".to_string()),
        ("usageScript", usage_script),
        ("usageAutoInterval", "30".to_string()),
        ("enabled", "true".to_string()),
    ];
    if app == "codex" {
        entries.insert(2, ("model", "gpt-5.4".to_string()));
    }

    let query = entries
        .into_iter()
        .map(|(key, value)| format!("{}={}", encode_query_param(key), encode_query_param(&value)))
        .collect::<Vec<_>>()
        .join("&");

    format!("ccswitch://v1/import?{query}")
}

fn build_ccswitch_usage_script() -> &'static str {
    r#"({
    request: {
      url: "{{baseUrl}}/usage",
      method: "GET",
      headers: { "Authorization": "Bearer {{apiKey}}" }
    },
    extractor: function(response) {
      const remaining = response?.remaining ?? response?.quota?.remaining ?? response?.balance;
      const unit = response?.unit ?? response?.quota?.unit ?? "USD";
      return {
        isValid: response?.is_active ?? response?.isValid ?? true,
        remaining,
        unit
      };
    }
  })"#
}

fn encode_query_param(value: &str) -> String {
    let mut output = String::new();
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                output.push(byte as char);
            }
            b' ' => output.push('+'),
            _ => output.push_str(&format!("%{byte:02X}")),
        }
    }
    output
}

struct SystemUrlLauncher {
    program: &'static str,
    args: Vec<String>,
}

#[cfg(target_os = "windows")]
fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    SystemUrlLauncher {
        program: "rundll32.exe",
        args: vec!["url.dll,FileProtocolHandler".to_string(), url.to_string()],
    }
}

#[cfg(target_os = "macos")]
fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    SystemUrlLauncher {
        program: "open",
        args: vec![url.to_string()],
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn system_url_launcher(url: &str) -> SystemUrlLauncher {
    SystemUrlLauncher {
        program: "xdg-open",
        args: vec![url.to_string()],
    }
}

fn open_url_with_system(url: &str) -> Result<(), String> {
    let launcher = system_url_launcher(url);
    let result = Command::new(launcher.program).args(launcher.args).spawn();

    result
        .map(|_| ())
        .map_err(|error| format!("无法打开外部链接: {error}"))
}

fn open_path_with_system(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer.exe").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(path).spawn();

    result
        .map(|_| ())
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
}

fn validate_external_http_url(url: &str) -> Result<&str, String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("外部链接为空，无法打开。".to_string());
    }
    if trimmed.chars().any(char::is_control) {
        return Err("外部链接包含无效字符，无法打开。".to_string());
    }
    let lower = trimmed.to_ascii_lowercase();
    if !lower.starts_with("http://") && !lower.starts_with("https://") {
        return Err("只支持打开 HTTP 或 HTTPS 链接。".to_string());
    }
    Ok(trimmed)
}

fn test_station_key_connectivity_prepared_blocking(
    key: KeyPoolItem,
    api_key: zeroize::Zeroizing<String>,
    capabilities: StationKeyCapabilities,
    model: String,
    progress: Channel<StationKeyConnectivityTestEvent>,
) -> Result<StationKeyConnectivityTestResult, String> {
    let mut progress = StationKeyConnectivityProgress::new(progress);
    let upstream_api_format = match key.station_upstream_api_format.as_str() {
        "openai_chat_completions" => UpstreamApiFormat::OpenAiChatCompletions,
        "openai_responses" => UpstreamApiFormat::OpenAiResponses,
        "custom_openai_compatible" => UpstreamApiFormat::CustomOpenAiCompatible,
        _ => UpstreamApiFormat::Auto,
    };
    let discovered_models =
        discover_station_key_connectivity_models(&key.station_api_base_url, api_key.as_str())
            .unwrap_or_default();
    let requested_model = model.trim().to_string();
    let candidates = station_key_connectivity_model_candidates(
        Some(&capabilities),
        Some(requested_model.as_str()),
        &discovered_models,
    );
    let (model, result) = run_station_key_connectivity_model_attempts(&candidates, |candidate| {
        run_station_key_connectivity_single_model_probe(
            &upstream_api_format,
            Some(&capabilities),
            |kind| {
                send_station_key_connectivity_probe(
                    &key.station_api_base_url,
                    api_key.as_str(),
                    candidate,
                    kind,
                    &mut progress,
                )
            },
        )
    });
    progress.emit_terminal(&result);
    Ok(StationKeyConnectivityTestResult {
        station_key_id: key.id,
        ok: result.ok,
        status_code: result.status_code,
        duration_ms: result.duration_ms,
        model,
        message: result.message,
        response_mode: result.response_mode,
        stream_fallback_reason: result.stream_fallback_reason,
    })
}

fn build_station_key_connectivity_probe_url(
    base_url: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Result<String, String> {
    let path = match kind {
        StationKeyConnectivityProbeKind::Responses => "/v1/responses",
        StationKeyConnectivityProbeKind::ChatCompletions => "/v1/chat/completions",
    };
    build_api_url(base_url, path)
}

fn build_station_key_connectivity_probe_body(
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    mode: StationKeyConnectivityRequestMode,
) -> Value {
    match kind {
        StationKeyConnectivityProbeKind::Responses => json!({
            "model": model,
            "input": "hi",
            "store": false,
            "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
            "max_output_tokens": 32,
        }),
        StationKeyConnectivityProbeKind::ChatCompletions => json!({
            "model": model,
            "messages": [{
                "role": "user",
                "content": "hi",
            }],
            "stream": matches!(mode, StationKeyConnectivityRequestMode::Stream),
            "max_tokens": 32,
        }),
    }
}

fn station_key_connectivity_protocol_label(kind: StationKeyConnectivityProbeKind) -> String {
    match kind {
        StationKeyConnectivityProbeKind::Responses => "responses".to_string(),
        StationKeyConnectivityProbeKind::ChatCompletions => "chat_completions".to_string(),
    }
}

fn emit_station_key_connectivity_event(
    progress: &mut StationKeyConnectivityProgress,
    event: StationKeyConnectivityTestEventPayload,
) {
    progress.emit(event, false);
}

fn redact_connectivity_error(message: &str) -> String {
    redact_error_message(&truncate_connectivity_reply(message.trim()))
}

struct StationKeyConnectivitySseDecoder {
    kind: StationKeyConnectivityProbeKind,
    pending: Vec<u8>,
    message: String,
    terminal_seen: bool,
}

impl StationKeyConnectivitySseDecoder {
    fn new(kind: StationKeyConnectivityProbeKind) -> Self {
        Self {
            kind,
            pending: Vec::new(),
            message: String::new(),
            terminal_seen: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) -> Result<Vec<String>, String> {
        self.pending.extend_from_slice(chunk);
        if self.pending.len() > STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT {
            return Err("SSE pending buffer too large".to_string());
        }

        let mut deltas = Vec::new();
        while let Some((boundary, separator_len)) = find_sse_event_boundary(&self.pending) {
            let event_bytes = self.pending[..boundary].to_vec();
            self.pending.drain(..boundary + separator_len);
            let event_text = std::str::from_utf8(&event_bytes)
                .map_err(|_| "SSE event contained invalid UTF-8".to_string())?;
            deltas.extend(self.consume_event(event_text)?);
        }
        Ok(deltas)
    }

    fn finish(self) -> Result<String, String> {
        if !self.pending.is_empty() {
            return Err("SSE stream ended with incomplete event".to_string());
        }
        if !self.terminal_seen {
            return Err("SSE stream ended without terminal signal".to_string());
        }
        Ok(redact_error_message(&truncate_connectivity_reply(
            &self.message,
        )))
    }

    fn consume_event(&mut self, event_text: &str) -> Result<Vec<String>, String> {
        let mut data_lines = Vec::new();
        for raw_line in event_text.lines() {
            let line = raw_line.trim_end_matches('\r');
            if line.is_empty() || line.starts_with(':') {
                continue;
            }
            if let Some(data) = line.strip_prefix("data:") {
                data_lines.push(data.strip_prefix(' ').unwrap_or(data));
            }
        }
        if data_lines.is_empty() {
            return Ok(Vec::new());
        }
        let data = data_lines.join("\n");
        if data.trim() == "[DONE]" {
            self.terminal_seen = true;
            return Ok(Vec::new());
        }

        let value = serde_json::from_str::<Value>(&data)
            .map_err(|error| format!("Malformed SSE JSON: {error}"))?;
        let delta = match self.kind {
            StationKeyConnectivityProbeKind::Responses => self.consume_responses_event(&value),
            StationKeyConnectivityProbeKind::ChatCompletions => self.consume_chat_event(&value),
        };
        Ok(delta.into_iter().collect())
    }

    fn consume_responses_event(&mut self, value: &Value) -> Option<String> {
        match value.get("type").and_then(Value::as_str) {
            Some("response.output_text.delta") => {
                let delta = value.get("delta").and_then(Value::as_str)?;
                self.message.push_str(delta);
                Some(delta.to_string())
            }
            Some("response.completed") => {
                self.terminal_seen = true;
                None
            }
            _ => None,
        }
    }

    fn consume_chat_event(&mut self, value: &Value) -> Option<String> {
        let delta = value
            .get("choices")
            .and_then(Value::as_array)
            .and_then(|choices| choices.first())
            .and_then(|choice| choice.get("delta"))
            .and_then(|delta| delta.get("content"))
            .and_then(Value::as_str)?;
        self.message.push_str(delta);
        Some(delta.to_string())
    }
}

fn find_sse_event_boundary(bytes: &[u8]) -> Option<(usize, usize)> {
    for index in 0..bytes.len() {
        if bytes[index] == b'\n' && bytes.get(index + 1) == Some(&b'\n') {
            return Some((index, 2));
        }
        if bytes[index] == b'\r'
            && bytes.get(index + 1) == Some(&b'\n')
            && bytes.get(index + 2) == Some(&b'\r')
            && bytes.get(index + 3) == Some(&b'\n')
        {
            return Some((index, 4));
        }
    }
    None
}

fn should_try_station_key_connectivity_chat_fallback(
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
    status_code: u16,
) -> bool {
    if !matches!(
        upstream_api_format,
        UpstreamApiFormat::Auto | UpstreamApiFormat::CustomOpenAiCompatible
    ) {
        return false;
    }
    if capabilities
        .map(|capabilities| !capabilities.supports_chat_completions)
        .unwrap_or(false)
    {
        return false;
    }
    matches!(status_code, 404 | 405 | 501) || should_fallback(status_code)
}

fn station_key_connectivity_model_candidates(
    capabilities: Option<&StationKeyCapabilities>,
    configured_model: Option<&str>,
    discovered_models: &[String],
) -> Vec<String> {
    let mut candidates = Vec::new();
    let blocked_models = capabilities
        .map(|capabilities| {
            capabilities
                .model_blocklist
                .iter()
                .map(|model| normalize_connectivity_model(model))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    push_station_key_connectivity_model_candidate(
        &mut candidates,
        configured_model,
        &blocked_models,
    );
    if let Some(capabilities) = capabilities {
        let explicit_models = if capabilities.model_allowlist.is_empty() {
            capabilities.preferred_models.as_slice()
        } else {
            capabilities.model_allowlist.as_slice()
        };
        let mut explicit_models = explicit_models.to_vec();
        explicit_models.sort_by_key(|model| connectivity_model_priority(model));
        for model in &explicit_models {
            push_station_key_connectivity_model_candidate(
                &mut candidates,
                Some(model.as_str()),
                &blocked_models,
            );
        }
    }
    let mut discovered_models = discovered_models.iter().enumerate().collect::<Vec<_>>();
    discovered_models.sort_by_key(|(index, model)| (connectivity_model_priority(model), *index));
    for (_, model) in discovered_models {
        push_station_key_connectivity_model_candidate(
            &mut candidates,
            Some(model.as_str()),
            &blocked_models,
        );
    }
    if candidates.is_empty() {
        candidates.push(DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string());
    }
    candidates.truncate(STATION_KEY_CONNECTIVITY_CANDIDATE_LIMIT);
    candidates
}

fn push_station_key_connectivity_model_candidate(
    candidates: &mut Vec<String>,
    model: Option<&str>,
    blocked_models: &[String],
) {
    let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return;
    };
    let normalized = normalize_connectivity_model(model);
    if blocked_models.iter().any(|blocked| blocked == &normalized) {
        return;
    }
    if !candidates
        .iter()
        .any(|candidate| normalize_connectivity_model(candidate) == normalized)
    {
        candidates.push(model.to_string());
    }
}

fn connectivity_model_priority(model: &str) -> i32 {
    let normalized = normalize_connectivity_model(model);
    if normalized.contains("nano") {
        return 0;
    }
    if normalized.contains("mini") {
        return 1;
    }
    if normalized.contains("lite") {
        return 2;
    }
    if normalized.contains("flash") {
        return 3;
    }
    if normalized.contains("haiku") {
        return 4;
    }
    if normalized.contains("turbo") {
        return 5;
    }
    if normalized == "deepseek-chat" || normalized.ends_with("-chat") {
        return 6;
    }
    20
}

fn normalize_connectivity_model(model: &str) -> String {
    model.trim().to_ascii_lowercase()
}

fn run_station_key_connectivity_model_attempts<F>(
    candidates: &[String],
    mut probe: F,
) -> (String, StationKeyConnectivityProbeResult)
where
    F: FnMut(&str) -> StationKeyConnectivityProbeResult,
{
    let fallback_candidates;
    let candidates = if candidates.is_empty() {
        fallback_candidates = vec![DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string()];
        fallback_candidates.as_slice()
    } else {
        candidates
    };
    let mut last = None;
    for model in candidates {
        let result = probe(model);
        if result.ok {
            return (model.clone(), result);
        }
        last = Some((model.clone(), result));
    }
    last.unwrap_or_else(|| {
        (
            DEFAULT_STATION_KEY_CONNECTIVITY_MODEL.to_string(),
            StationKeyConnectivityProbeResult::failure(0, 0, "未执行连通性探测".to_string()),
        )
    })
}

fn run_station_key_connectivity_stream_first_probe<F, E>(
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    mut send_attempt: F,
    mut emit_event: E,
) -> StationKeyConnectivityProbeResult
where
    F: FnMut(StationKeyConnectivityRequestMode) -> StationKeyConnectivityProbeResult,
    E: FnMut(StationKeyConnectivityTestEventPayload),
{
    emit_event(StationKeyConnectivityTestEventPayload::AttemptStarted {
        model: model.to_string(),
        protocol: station_key_connectivity_protocol_label(kind),
    });

    let stream_result = send_attempt(StationKeyConnectivityRequestMode::Stream);
    if stream_result.ok {
        return stream_result.with_response_mode(StationKeyConnectivityResponseMode::Stream);
    }

    let fallback_reason = redact_connectivity_error(&stream_result.message);
    emit_event(StationKeyConnectivityTestEventPayload::Fallback {
        reason: fallback_reason.clone(),
    });
    let fallback_result = send_attempt(StationKeyConnectivityRequestMode::NonStream);
    let duration_ms = stream_result
        .duration_ms
        .saturating_add(fallback_result.duration_ms);

    if fallback_result.ok {
        return StationKeyConnectivityProbeResult::success(
            fallback_result.status_code,
            duration_ms,
            fallback_result.message,
        )
        .with_response_mode(StationKeyConnectivityResponseMode::NonStreamFallback)
        .with_stream_fallback_reason(Some(fallback_reason));
    }

    StationKeyConnectivityProbeResult::failure(
        fallback_result.status_code,
        duration_ms,
        format!(
            "Stream: {}; Non-stream fallback: {}",
            stream_result.message, fallback_result.message
        ),
    )
    .with_response_mode(StationKeyConnectivityResponseMode::NonStreamFallback)
    .with_stream_fallback_reason(Some(fallback_reason))
}

fn run_station_key_connectivity_single_model_probe<F>(
    upstream_api_format: &UpstreamApiFormat,
    capabilities: Option<&StationKeyCapabilities>,
    mut send_probe: F,
) -> StationKeyConnectivityProbeResult
where
    F: FnMut(StationKeyConnectivityProbeKind) -> StationKeyConnectivityProbeResult,
{
    let response_result = send_probe(StationKeyConnectivityProbeKind::Responses);
    if response_result.ok {
        return response_result;
    }
    if !should_try_station_key_connectivity_chat_fallback(
        upstream_api_format,
        capabilities,
        response_result.status_code,
    ) {
        return response_result;
    }

    let chat_result = send_probe(StationKeyConnectivityProbeKind::ChatCompletions);
    let duration_ms = response_result
        .duration_ms
        .saturating_add(chat_result.duration_ms);
    if chat_result.ok {
        let mut chat_result = chat_result;
        chat_result.duration_ms = duration_ms;
        return chat_result;
    }

    StationKeyConnectivityProbeResult::failure(
        chat_result.status_code,
        duration_ms,
        format!(
            "Responses: {}; Chat Completions: {}",
            response_result.message, chat_result.message
        ),
    )
}

fn send_station_key_connectivity_probe(
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    progress: &mut StationKeyConnectivityProgress,
) -> StationKeyConnectivityProbeResult {
    let progress = std::cell::RefCell::new(progress);
    run_station_key_connectivity_stream_first_probe(
        model,
        kind,
        |mode| match mode {
            StationKeyConnectivityRequestMode::Stream => {
                let mut progress = progress.borrow_mut();
                send_station_key_connectivity_stream_probe_attempt(
                    base_url,
                    api_key,
                    model,
                    kind,
                    &mut **progress,
                )
            }
            StationKeyConnectivityRequestMode::NonStream => {
                send_station_key_connectivity_non_stream_probe_attempt(
                    base_url, api_key, model, kind,
                )
            }
        },
        |event| {
            let mut progress = progress.borrow_mut();
            emit_station_key_connectivity_event(&mut **progress, event);
        },
    )
}

fn send_station_key_connectivity_non_stream_probe_attempt(
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
) -> StationKeyConnectivityProbeResult {
    let url = match build_station_key_connectivity_probe_url(base_url, kind) {
        Ok(url) => url,
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                0,
                redact_error_message(&format!("API Base URL 无效: {error}")),
            );
        }
    };
    let body = build_station_key_connectivity_probe_body(
        model,
        kind,
        StationKeyConnectivityRequestMode::NonStream,
    );
    let started = Instant::now();
    let response_result = ureq::post(&url)
        .timeout(STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "application/json")
        .send_json(body);
    let (status_code, response_text) = match response_result {
        Ok(response) => response_text_pair(response),
        Err(ureq::Error::Status(_, response)) => response_text_pair(response),
        Err(error) => {
            let duration_ms = elapsed_ms(started);
            return StationKeyConnectivityProbeResult::failure(
                0,
                duration_ms,
                redact_error_message(&format!("{error}")),
            );
        }
    };
    let duration_ms = elapsed_ms(started);
    if (200..300).contains(&status_code) {
        let message =
            extract_station_key_connectivity_reply(&response_text, kind).unwrap_or_else(|| {
                match kind {
                    StationKeyConnectivityProbeKind::Responses => "Responses 连通正常".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions => {
                        "Chat Completions 连通正常".to_string()
                    }
                }
            });
        return StationKeyConnectivityProbeResult::success(status_code, duration_ms, message);
    }
    StationKeyConnectivityProbeResult::failure(
        status_code,
        duration_ms,
        response_error_message(&response_text, status_code),
    )
}

fn send_station_key_connectivity_stream_probe_attempt(
    base_url: &str,
    api_key: &str,
    model: &str,
    kind: StationKeyConnectivityProbeKind,
    progress: &mut StationKeyConnectivityProgress,
) -> StationKeyConnectivityProbeResult {
    let url = match build_station_key_connectivity_probe_url(base_url, kind) {
        Ok(url) => url,
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                0,
                redact_error_message(&format!("API Base URL 无效: {error}")),
            );
        }
    };
    let body = build_station_key_connectivity_probe_body(
        model,
        kind,
        StationKeyConnectivityRequestMode::Stream,
    );
    let started = Instant::now();
    let response_result = ureq::post(&url)
        .timeout(STATION_KEY_CONNECTIVITY_PROBE_TIMEOUT)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Content-Type", "application/json")
        .set("Accept", "text/event-stream")
        .send_json(body);

    let response = match response_result {
        Ok(response) => response,
        Err(ureq::Error::Status(_, response)) => {
            let (status_code, response_text) = response_text_pair(response);
            return StationKeyConnectivityProbeResult::failure(
                status_code,
                elapsed_ms(started),
                response_error_message(&response_text, status_code),
            );
        }
        Err(error) => {
            return StationKeyConnectivityProbeResult::failure(
                0,
                elapsed_ms(started),
                redact_error_message(&format!("{error}")),
            );
        }
    };

    let status_code = response.status();
    if !(200..300).contains(&status_code) {
        let (status_code, response_text) = response_text_pair(response);
        return StationKeyConnectivityProbeResult::failure(
            status_code,
            elapsed_ms(started),
            response_error_message(&response_text, status_code),
        );
    }

    let content_type = response
        .header("content-type")
        .unwrap_or("")
        .to_ascii_lowercase();
    if !content_type.contains("text/event-stream") {
        let (_status_code, _response_text) = response_text_pair(response);
        return StationKeyConnectivityProbeResult::failure(
            status_code,
            elapsed_ms(started),
            redact_connectivity_error(&format!(
                "Expected text/event-stream response, got {}",
                if content_type.is_empty() {
                    "missing content-type"
                } else {
                    content_type.as_str()
                }
            )),
        );
    }

    let mut reader = response.into_reader();
    let mut decoder = StationKeyConnectivitySseDecoder::new(kind);
    let mut buffer = [0_u8; 2048];
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(count) => count,
            Err(error) => {
                return StationKeyConnectivityProbeResult::failure(
                    status_code,
                    elapsed_ms(started),
                    redact_connectivity_error(&format!("Failed to read SSE stream: {error}")),
                );
            }
        };
        let deltas = match decoder.push(&buffer[..count]) {
            Ok(deltas) => deltas,
            Err(error) => {
                return StationKeyConnectivityProbeResult::failure(
                    status_code,
                    elapsed_ms(started),
                    redact_connectivity_error(&error),
                );
            }
        };
        for delta in deltas {
            emit_station_key_connectivity_event(
                progress,
                StationKeyConnectivityTestEventPayload::Delta { text: delta },
            );
        }
    }

    match decoder.finish() {
        Ok(message) if !message.trim().is_empty() => {
            StationKeyConnectivityProbeResult::success(status_code, elapsed_ms(started), message)
        }
        Ok(_) => StationKeyConnectivityProbeResult::success(
            status_code,
            elapsed_ms(started),
            match kind {
                StationKeyConnectivityProbeKind::Responses => {
                    "Responses streaming connected".to_string()
                }
                StationKeyConnectivityProbeKind::ChatCompletions => {
                    "Chat Completions streaming connected".to_string()
                }
            },
        ),
        Err(error) => StationKeyConnectivityProbeResult::failure(
            status_code,
            elapsed_ms(started),
            redact_connectivity_error(&error),
        ),
    }
}

fn discover_station_key_connectivity_models(base_url: &str, api_key: &str) -> Option<Vec<String>> {
    let url = build_api_url(base_url, "/v1/models").ok()?;
    let response = ureq::get(&url)
        .timeout(STATION_KEY_CONNECTIVITY_MODEL_DISCOVERY_TIMEOUT)
        .set("Authorization", &format!("Bearer {api_key}"))
        .set("Accept", "application/json")
        .call()
        .ok()?;
    if !(200..300).contains(&response.status()) {
        return None;
    }
    let body = response.into_string().ok()?;
    let value = serde_json::from_str::<Value>(&body).ok()?;
    let models = model_ids_from_models_response(&value);
    if models.is_empty() {
        None
    } else {
        Some(models)
    }
}

fn model_ids_from_models_response(value: &Value) -> Vec<String> {
    value
        .get("data")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|model| model.get("id").and_then(Value::as_str))
        .filter(|model| !model.trim().is_empty())
        .map(|model| model.trim().to_string())
        .collect()
}

fn response_text_pair(response: ureq::Response) -> (u16, String) {
    let status = response.status();
    let text = response.into_string().unwrap_or_default();
    (status, text)
}

fn elapsed_ms(started: Instant) -> i64 {
    started.elapsed().as_millis().min(i64::MAX as u128) as i64
}

fn response_error_message(response_text: &str, status_code: u16) -> String {
    let parsed = serde_json::from_str::<Value>(response_text).ok();
    let message = parsed
        .as_ref()
        .and_then(|value| value.pointer("/error/message"))
        .and_then(Value::as_str)
        .or_else(|| {
            parsed
                .as_ref()
                .and_then(|value| value.get("message"))
                .and_then(Value::as_str)
        })
        .unwrap_or(response_text)
        .trim();
    let fallback = if message.is_empty() {
        format!("Responses 返回 HTTP {status_code}")
    } else {
        message.to_string()
    };
    redact_error_message(&fallback)
}

fn extract_station_key_connectivity_reply(
    response_text: &str,
    kind: StationKeyConnectivityProbeKind,
) -> Option<String> {
    let parsed = serde_json::from_str::<Value>(response_text).ok()?;
    let reply = match kind {
        StationKeyConnectivityProbeKind::Responses => extract_responses_reply_text(&parsed),
        StationKeyConnectivityProbeKind::ChatCompletions => extract_chat_reply_text(&parsed),
    }?;
    let trimmed = reply.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(redact_error_message(&truncate_connectivity_reply(trimmed)))
    }
}

fn extract_responses_reply_text(value: &Value) -> Option<String> {
    value
        .get("output_text")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            value
                .get("output")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|item| {
                    item.get("content")
                        .and_then(Value::as_array)
                        .into_iter()
                        .flatten()
                        .find_map(|content| {
                            content
                                .get("text")
                                .and_then(Value::as_str)
                                .map(ToString::to_string)
                        })
                })
        })
}

fn extract_chat_reply_text(value: &Value) -> Option<String> {
    let message = value.pointer("/choices/0/message")?;
    message
        .get("content")
        .and_then(Value::as_str)
        .map(ToString::to_string)
        .or_else(|| {
            message
                .get("content")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .find_map(|content| {
                    content
                        .get("text")
                        .and_then(Value::as_str)
                        .map(ToString::to_string)
                })
        })
}

fn truncate_connectivity_reply(value: &str) -> String {
    const MAX_REPLY_CHARS: usize = 240;
    let mut chars = value.chars();
    let truncated = chars.by_ref().take(MAX_REPLY_CHARS).collect::<String>();
    if chars.next().is_some() {
        format!("{truncated}...")
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channel_monitor_runner_errors_are_result_unknown_and_redacted() {
        let error = public_channel_monitor_run_error(
            "provider failed with api_key=sk-secret at C:/private/data.db".into(),
        );

        assert_eq!(error.code, error::CommandErrorCode::Conflict);
        assert_eq!(
            error.message,
            "The operation outcome could not be confirmed."
        );
        assert!(!error.retryable);
        assert!(!error.message.contains("sk-secret"));
        assert!(!error.message.contains("data.db"));
    }

    #[test]
    fn remote_key_failures_keep_public_machine_classification() {
        let unsupported =
            public_remote_key_error(remote_keys::RemoteKeyOperationError::Unsupported);
        assert_eq!(unsupported.code, error::CommandErrorCode::Unsupported);
        assert!(!unsupported.retryable);

        let external =
            public_remote_key_error(remote_keys::RemoteKeyOperationError::ExternalUnavailable);
        assert_eq!(external.code, error::CommandErrorCode::ExternalUnavailable);
        assert!(external.retryable);

        let conflict = public_remote_key_error(remote_keys::RemoteKeyOperationError::Conflict);
        assert_eq!(conflict.code, error::CommandErrorCode::Conflict);
        assert!(!conflict.retryable);

        let not_found = public_remote_key_error(remote_keys::RemoteKeyOperationError::Application(
            ApplicationError::NotFound,
        ));
        assert_eq!(not_found.code, error::CommandErrorCode::NotFound);
    }

    #[test]
    fn capture_request_belongs_to_management_base_when_station_url_uses_v1() {
        assert!(capture_request_belongs_to_station(
            "https://relay.example.com",
            "https://relay.example.com/v1",
            "https://relay.example.com/api/v1/auth/login"
        ));
    }

    #[test]
    fn capture_request_rejects_other_station_origins() {
        assert!(!capture_request_belongs_to_station(
            "https://relay.example.com",
            "https://relay.example.com/v1",
            "https://other.example.com/api/v1/auth/login"
        ));
    }

    #[test]
    fn capture_accepts_configured_origins_and_rejects_lookalikes() {
        assert!(capture_request_belongs_to_station(
            "https://console.example:443",
            "https://api.example/v1",
            "https://console.example/api/user/self",
        ));
        assert!(capture_request_belongs_to_station(
            "https://console.example",
            "https://api.example/v1",
            "https://api.example/v1/models",
        ));
        assert!(!capture_request_belongs_to_station(
            "https://console.example",
            "https://api.example/v1",
            "https://console.example.evil.test/api/user/self",
        ));
    }

    #[test]
    fn captured_newapi_self_event_marks_web_authorization_candidate() {
        let input = CapturedHttpEventInput {
            station_id: "station-1".to_string(),
            source_window_id: "capture-station-1".to_string(),
            page_url: "https://relay.example/console".to_string(),
            request_url: "https://relay.example/api/user/self".to_string(),
            request_path: Some("/api/user/self".to_string()),
            method: "GET".to_string(),
            status: Some(200),
            content_type: Some("application/json".to_string()),
            started_at: None,
            finished_at: None,
            duration_ms: None,
            response_kind: Some("json".to_string()),
            response_size: None,
            response_json: Some(json!({ "success": true, "data": { "id": 42 } })),
            response_text: None,
            error_message: None,
        };

        assert_eq!(
            web_authorization_candidate_user_id_from_input(&input).as_deref(),
            Some("42")
        );
    }

    #[test]
    fn capture_script_invokes_web_authorization_finish_after_candidate() {
        let script = capture_script("station-1", "capture-station-1", None, None);

        assert!(script.contains("finish_web_authorization_session"));
        assert!(script.contains("webAuthorizationCandidate"));
        assert!(script.contains("__relayPoolAuthorizationFinishInFlight"));
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn ccswitch_protocol_urls_use_windows_file_protocol_handler() {
        let launcher = system_url_launcher("ccswitch://v1/import?resource=provider");

        assert_eq!(launcher.program, "rundll32.exe");
        assert_eq!(
            launcher.args,
            vec![
                "url.dll,FileProtocolHandler",
                "ccswitch://v1/import?resource=provider"
            ]
        );
    }

    #[test]
    fn ccswitch_deeplink_matches_sub2api_codex_import_shape() {
        let deeplink = build_ccswitch_provider_deeplink(
            "codex",
            "Relay Pool Desktop",
            "http://127.0.0.1:8787",
            "http://127.0.0.1:8787/v1",
            "sk test",
        );

        assert!(deeplink.starts_with("ccswitch://v1/import?"));
        assert!(deeplink.contains("resource=provider"));
        assert!(deeplink.contains("app=codex"));
        assert!(deeplink.contains("model=gpt-5.4"));
        assert!(deeplink.contains("name=Relay+Pool+Desktop"));
        assert!(deeplink.contains("homepage=http%3A%2F%2F127.0.0.1%3A8787"));
        assert!(deeplink.contains("endpoint=http%3A%2F%2F127.0.0.1%3A8787%2Fv1"));
        assert!(deeplink.contains("apiKey=sk+test"));
        assert!(deeplink.contains("configFormat=json"));
        assert!(deeplink.contains("usageEnabled=true"));
        assert!(deeplink.contains("usageAutoInterval=30"));
        assert!(deeplink.contains("usageScript="));
    }

    #[test]
    fn ccswitch_import_uses_v2_local_access_key_before_building_deeplink() {
        let status = ProxyStatus {
            running: true,
            lifecycle: crate::models::proxy::ProxyLifecycle::Running,
            bind_addr: "127.0.0.1".to_string(),
            port: 8787,
            started_at: None,
            last_error: None,
            active_requests: 0,
            request_count: 0,
        };

        let local_access_key = "sk-v2-test";
        let (_, deeplink) = prepare_ccswitch_import(local_access_key, &status);

        assert!(deeplink.contains(&format!("apiKey={}", encode_query_param(local_access_key))));
    }

    #[test]
    fn external_url_validation_accepts_http_urls() {
        assert_eq!(
            validate_external_http_url(" https://api.example.test/v1 "),
            Ok("https://api.example.test/v1")
        );
        assert_eq!(
            validate_external_http_url("HTTP://api.example.test"),
            Ok("HTTP://api.example.test")
        );
    }

    #[test]
    fn external_url_validation_rejects_non_http_urls() {
        let error = validate_external_http_url("ccswitch://v1/import?resource=provider")
            .expect_err("custom schemes should not be accepted by the station URL opener");

        assert!(error.contains("HTTP"));
    }

    #[test]
    fn station_key_connectivity_probe_uses_low_token_responses_request() {
        let body = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            StationKeyConnectivityRequestMode::NonStream,
        );

        assert_eq!(body["model"], "gpt-test");
        assert_eq!(body["input"], "hi");
        assert_eq!(body["store"], false);
        assert_eq!(body["max_output_tokens"], 32);
    }

    #[test]
    fn station_key_connectivity_stream_bodies_request_streaming() {
        let responses = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            StationKeyConnectivityRequestMode::Stream,
        );
        let chat = build_station_key_connectivity_probe_body(
            "gpt-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
            StationKeyConnectivityRequestMode::Stream,
        );

        assert_eq!(responses["model"], "gpt-test");
        assert_eq!(responses["input"], "hi");
        assert_eq!(responses["stream"], true);
        assert_eq!(chat["model"], "gpt-test");
        assert_eq!(chat["messages"][0]["content"], "hi");
        assert_eq!(chat["stream"], true);
    }

    #[test]
    fn station_key_connectivity_responses_sse_decodes_split_deltas() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        assert!(decoder
            .push(br#"data: {"type":"response.output_text.delta","delta":"Hel"#)
            .unwrap()
            .is_empty());
        assert_eq!(decoder.push(br#"lo"}"#).unwrap(), Vec::<String>::new());
        assert_eq!(
            decoder
                .push(b"\n\ndata: {\"type\":\"response.output_text.delta\",\"delta\":\"!\"}\n\ndata: {\"type\":\"response.completed\"}\n\n")
                .unwrap(),
            vec!["Hello".to_string(), "!".to_string()]
        );
        assert_eq!(decoder.finish().unwrap(), "Hello!");
    }

    #[test]
    fn station_key_connectivity_responses_sse_accepts_done_sentinel() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let deltas = decoder
            .push(
                b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"Hi\"}\n\ndata: [DONE]\n\n",
            )
            .unwrap();

        assert_eq!(deltas, vec!["Hi".to_string()]);
        assert_eq!(decoder.finish().unwrap(), "Hi");
    }

    #[test]
    fn station_key_connectivity_chat_sse_decodes_crlf_comments_and_done() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::ChatCompletions);

        let deltas = decoder
            .push(
                b": keep-alive\r\n\r\ndata: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\r\n\r\ndata: [DONE]\r\n\r\n",
            )
            .unwrap();

        assert_eq!(deltas, vec!["Hi".to_string()]);
        assert_eq!(decoder.finish().unwrap(), "Hi");
    }

    #[test]
    fn station_key_connectivity_sse_rejects_malformed_json() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let error = decoder
            .push(b"data: {not-json}\n\n")
            .expect_err("malformed SSE JSON should fail the stream attempt");

        assert!(error.contains("SSE"));
    }

    #[test]
    fn station_key_connectivity_sse_rejects_missing_terminal_signal() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);

        let deltas = decoder
            .push(b"data: {\"type\":\"response.output_text.delta\",\"delta\":\"partial\"}\n\n")
            .unwrap();
        assert_eq!(deltas, vec!["partial".to_string()]);

        let error = decoder
            .finish()
            .expect_err("closing without response.completed should fail");

        assert!(error.contains("terminal"));
    }

    #[test]
    fn station_key_connectivity_sse_rejects_oversized_pending_data() {
        let mut decoder =
            StationKeyConnectivitySseDecoder::new(StationKeyConnectivityProbeKind::Responses);
        let oversized = vec![b'a'; STATION_KEY_CONNECTIVITY_SSE_PENDING_LIMIT + 1];

        let error = decoder
            .push(&oversized)
            .expect_err("oversized pending data should fail");

        assert!(error.contains("too large"));
    }

    #[test]
    fn station_key_connectivity_event_envelope_is_versioned_and_terminal() {
        let value = serde_json::to_value(station_key_connectivity_event_envelope(
            "run-1".to_string(),
            7,
            true,
            StationKeyConnectivityTestEventPayload::Completed { ok: true },
        ))
        .expect("serialize streaming envelope");

        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["runId"], "run-1");
        assert_eq!(value["sequence"], 7);
        assert_eq!(value["terminal"], true);
        assert_eq!(value["cancelCapability"], "detach_only");
        assert_eq!(value["event"]["type"], "completed");
        assert_eq!(value["event"]["ok"], true);
    }

    #[test]
    fn station_key_connectivity_stream_success_does_not_retry_non_stream() {
        let mut attempted_modes = Vec::new();
        let mut events = Vec::new();

        let result = run_station_key_connectivity_stream_first_probe(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            |mode| {
                attempted_modes.push(mode);
                StationKeyConnectivityProbeResult::success(200, 15, "stream ok".to_string())
            },
            |event| events.push(event),
        );

        assert_eq!(
            attempted_modes,
            vec![StationKeyConnectivityRequestMode::Stream]
        );
        assert!(result.ok);
        assert_eq!(
            result.response_mode,
            StationKeyConnectivityResponseMode::Stream
        );
        assert_eq!(result.stream_fallback_reason, None);
        assert!(matches!(
            events.first(),
            Some(StationKeyConnectivityTestEventPayload::AttemptStarted { model, .. }) if model == "gpt-test"
        ));
    }

    #[test]
    fn station_key_connectivity_stream_failure_retries_once_non_stream() {
        let mut attempted_modes = Vec::new();
        let mut events = Vec::new();

        let result = run_station_key_connectivity_stream_first_probe(
            "gpt-test",
            StationKeyConnectivityProbeKind::Responses,
            |mode| {
                attempted_modes.push(mode);
                match mode {
                    StationKeyConnectivityRequestMode::Stream => {
                        StationKeyConnectivityProbeResult::failure(
                            200,
                            9,
                            "missing terminal signal".to_string(),
                        )
                    }
                    StationKeyConnectivityRequestMode::NonStream => {
                        StationKeyConnectivityProbeResult::success(
                            200,
                            14,
                            "fallback ok".to_string(),
                        )
                    }
                }
            },
            |event| events.push(event),
        );

        assert_eq!(
            attempted_modes,
            vec![
                StationKeyConnectivityRequestMode::Stream,
                StationKeyConnectivityRequestMode::NonStream,
            ]
        );
        assert!(result.ok);
        assert_eq!(
            result.response_mode,
            StationKeyConnectivityResponseMode::NonStreamFallback
        );
        assert_eq!(
            result.stream_fallback_reason,
            Some("missing terminal signal".to_string())
        );
        assert!(events.iter().any(|event| matches!(
            event,
            StationKeyConnectivityTestEventPayload::Fallback { reason } if reason == "missing terminal signal"
        )));
    }

    #[test]
    fn station_key_connectivity_extracts_responses_reply_text() {
        let value = json!({
            "output": [{
                "type": "message",
                "content": [{
                    "type": "output_text",
                    "text": "Hi! What can I help you with?"
                }]
            }]
        });

        assert_eq!(
            extract_station_key_connectivity_reply(
                &value.to_string(),
                StationKeyConnectivityProbeKind::Responses
            ),
            Some("Hi! What can I help you with?".to_string())
        );
    }

    #[test]
    fn station_key_connectivity_extracts_chat_reply_text() {
        let value = json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": "Hi there"
                }
            }]
        });

        assert_eq!(
            extract_station_key_connectivity_reply(
                &value.to_string(),
                StationKeyConnectivityProbeKind::ChatCompletions
            ),
            Some("Hi there".to_string())
        );
    }

    #[test]
    fn station_key_connectivity_candidates_choose_lowest_allowed_model() {
        let capabilities = StationKeyCapabilities {
            station_key_id: "key-lowest".to_string(),
            supports_chat_completions: true,
            supports_responses: true,
            supports_embeddings: false,
            supports_stream: true,
            supports_tools: false,
            supports_vision: false,
            supports_reasoning: false,
            model_allowlist: vec![
                "gpt-4.1".to_string(),
                "gpt-4.1-mini".to_string(),
                "claude-sonnet-4".to_string(),
            ],
            model_blocklist: Vec::new(),
            preferred_models: vec!["gpt-4.1".to_string()],
            only_use_as_backup: false,
            routing_tags: Vec::new(),
            updated_at: "0".to_string(),
        };

        let candidates = station_key_connectivity_model_candidates(Some(&capabilities), None, &[]);

        assert_eq!(candidates[0], "gpt-4.1-mini");
        assert!(!candidates.contains(&"gpt-4o-mini".to_string()));
    }

    #[test]
    fn station_key_connectivity_probe_posts_to_responses_endpoint() {
        let url = build_station_key_connectivity_probe_url(
            "https://relay.example/v1",
            StationKeyConnectivityProbeKind::Responses,
        )
        .expect("build responses probe URL");

        assert_eq!(url, "https://relay.example/v1/responses");
    }

    #[test]
    fn station_key_connectivity_probe_uses_complete_api_namespace() {
        let url = build_station_key_connectivity_probe_url(
            "https://relay.example/api/v3",
            StationKeyConnectivityProbeKind::Responses,
        )
        .expect("build API namespace responses probe URL");

        assert_eq!(url, "https://relay.example/api/v3/responses");
    }

    #[test]
    fn station_key_connectivity_candidates_use_discovered_model_when_not_configured() {
        let discovered = vec!["claude-test".to_string()];
        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["claude-test"]);
    }

    #[test]
    fn station_key_connectivity_candidates_do_not_default_to_retired_gpt_4o_mini() {
        let candidates = station_key_connectivity_model_candidates(None, None, &[]);

        assert_eq!(candidates, vec!["gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_candidates_keep_fastest_discovered_models() {
        let discovered = vec![
            "codex-auto-review".to_string(),
            "gpt-5.4".to_string(),
            "gpt-5.5".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["codex-auto-review", "gpt-5.4"]);
    }

    #[test]
    fn station_key_connectivity_candidates_are_capped_for_interactive_tests() {
        let discovered = vec![
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
            "gpt-5.4".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_candidates_sort_discovered_models_by_lowest_cost() {
        let discovered = vec![
            "gpt-4.1".to_string(),
            "gpt-4.1-mini".to_string(),
            "gpt-4.1-nano".to_string(),
        ];

        let candidates =
            station_key_connectivity_model_candidates(None, None, discovered.as_slice());

        assert_eq!(candidates, vec!["gpt-4.1-nano", "gpt-4.1-mini"]);
    }

    #[test]
    fn station_key_connectivity_attempts_next_model_after_503() {
        let candidates = vec!["codex-auto-review".to_string(), "gpt-5.4".to_string()];
        let mut attempted = Vec::new();

        let (model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                attempted.push(candidate.to_string());
                if candidate == "gpt-5.4" {
                    StationKeyConnectivityProbeResult::success(
                        200,
                        42,
                        "Chat Completions 连通正常".to_string(),
                    )
                } else {
                    StationKeyConnectivityProbeResult::failure(
                        503,
                        12,
                        "Service temporarily unavailable".to_string(),
                    )
                }
            });

        assert_eq!(attempted, vec!["codex-auto-review", "gpt-5.4"]);
        assert_eq!(model, "gpt-5.4");
        assert!(result.ok);
    }

    #[test]
    fn station_key_connectivity_attempts_next_model_after_responses_and_chat_fail() {
        let candidates = vec!["codex-auto-review".to_string(), "gpt-5.4".to_string()];
        let mut attempted = Vec::new();

        let (model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::Auto,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match (candidate, kind) {
                            ("gpt-5.4", StationKeyConnectivityProbeKind::ChatCompletions) => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    11,
                                    "Chat Completions 连通正常".to_string(),
                                )
                            }
                            _ => StationKeyConnectivityProbeResult::failure(
                                503,
                                7,
                                "Service temporarily unavailable".to_string(),
                            ),
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "codex-auto-review".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
                (
                    "gpt-5.4".to_string(),
                    StationKeyConnectivityProbeKind::Responses,
                ),
                (
                    "gpt-5.4".to_string(),
                    StationKeyConnectivityProbeKind::ChatCompletions,
                ),
            ]
        );
        assert_eq!(model, "gpt-5.4");
        assert!(result.ok);
        assert_eq!(result.status_code, 200);
        assert_eq!(result.duration_ms, 18);
        assert_eq!(result.message, "Chat Completions 连通正常");
    }

    #[test]
    fn station_key_connectivity_network_error_does_not_switch_protocol() {
        let candidates = vec!["gpt-4.1-mini".to_string()];
        let mut attempted = Vec::new();

        let (_model, result) =
            run_station_key_connectivity_model_attempts(&candidates, |candidate| {
                run_station_key_connectivity_single_model_probe(
                    &UpstreamApiFormat::Auto,
                    None,
                    |kind| {
                        attempted.push((candidate.to_string(), kind));
                        match kind {
                            StationKeyConnectivityProbeKind::Responses => {
                                StationKeyConnectivityProbeResult::failure(
                                    0,
                                    9,
                                    "Network Error".to_string(),
                                )
                            }
                            StationKeyConnectivityProbeKind::ChatCompletions => {
                                StationKeyConnectivityProbeResult::success(
                                    200,
                                    13,
                                    "Chat Completions 连通正常".to_string(),
                                )
                            }
                        }
                    },
                )
            });

        assert_eq!(
            attempted,
            vec![(
                "gpt-4.1-mini".to_string(),
                StationKeyConnectivityProbeKind::Responses,
            )]
        );
        assert!(!result.ok);
        assert_eq!(result.status_code, 0);
    }

    #[test]
    fn station_key_connectivity_chat_probe_uses_low_token_request() {
        let body = build_station_key_connectivity_probe_body(
            "claude-test",
            StationKeyConnectivityProbeKind::ChatCompletions,
            StationKeyConnectivityRequestMode::NonStream,
        );

        assert_eq!(body["model"], "claude-test");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["messages"][0]["content"], "hi");
        assert_eq!(body["stream"], false);
        assert_eq!(body["max_tokens"], 32);
    }

    #[test]
    fn station_key_connectivity_auto_format_can_fallback_to_chat_on_503() {
        assert!(should_try_station_key_connectivity_chat_fallback(
            &UpstreamApiFormat::Auto,
            None,
            503,
        ));
    }
}
