use serde_json::Value;
use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
    process::Command,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
use tauri::State;

use crate::{
    commands::{data_recovery, error},
    ipc::dto::{
        updater_data_recovery::{
            ActivateDataStoreCandidateInputDto, ActivationResultDto, CreateNewDataStoreInputDto,
            DataStoreCandidateViewDto, DataStoreStartupViewDto,
        },
        EmptyInputDto,
    },
    observability::correlation,
    services::{
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
        secrets::{validation::validate_database_secrets, SecretManager},
    },
};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";
const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
const DATABASE_FILE_V2: &str = "relay-pool-desktop-v2.sqlite3";

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

fn is_supported_database_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == DATABASE_FILE || name == DATABASE_FILE_V2)
}

fn open_path_with_system(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer.exe").arg(path).status();
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let result = Command::new("xdg-open").arg(path).status();

    result
        .and_then(|status| {
            if status.success() {
                Ok(())
            } else {
                Err(std::io::Error::other(format!(
                    "launcher exited with status {status}"
                )))
            }
        })
        .map_err(|error| format!("failed to open {}: {error}", path.display()))
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
