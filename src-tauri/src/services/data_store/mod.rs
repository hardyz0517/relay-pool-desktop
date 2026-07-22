pub mod backup;
pub mod config;
pub(crate) mod data_directory_port;
pub mod decision;
pub mod diagnostic;
pub(crate) mod generation_upgrade;
pub mod inspect;
pub mod installation_lease;
pub mod relocation;
#[cfg(test)]
pub(crate) mod test_support;
pub mod types;

use std::{
    fs,
    path::{Path, PathBuf},
};

use config::{installation_marker_exists, read_config_v3, DataDirConfigV3, DatabaseGeneration};
use decision::{decide_startup, CandidateFacts, DecisionInput};
use inspect::{inspect_candidate, InspectedDataStoreCandidate};
use types::{
    CandidateHealth, CandidateRole, DataStoreRelocationIntent, DataStoreStartupState,
    RecoveryReason, StartupDecision,
};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";
#[cfg(test)]
const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";
const DATABASE_FILE_V2: &str = "relay-pool-desktop-v2.sqlite3";

pub fn inspect_startup(default_data_dir: &Path) -> Result<DataStoreStartupState, String> {
    let config_path = default_data_dir.join(DATA_DIR_CONFIG_FILE);
    let raw_config_version = fs::read_to_string(&config_path)
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|value| value.get("version").and_then(serde_json::Value::as_u64));
    let config = match read_config_v3(&config_path) {
        Ok(config) => config,
        Err(_) => return inspect_with_unreadable_config(default_data_dir),
    };
    let generation = config
        .as_ref()
        .map(|config| config.database_generation)
        .unwrap_or(DatabaseGeneration::One);
    let database_file = generation.database_file();
    let initialized = config.is_some() || installation_marker_exists(default_data_dir);
    let relocation_intent = config
        .as_ref()
        .and_then(|config| trusted_relocation_intent(config, database_file));
    let pending_relocation = config.as_ref().is_some_and(|c| {
        raw_config_version.unwrap_or(1) < 2 && c.pending_data_dir != c.source_data_dir
    });
    let active_data_dir = config
        .as_ref()
        .and_then(|c| c.active_data_dir.clone())
        .or_else(|| initialized.then(|| default_data_dir.to_path_buf()));
    let mut inspected = Vec::new();
    inspected.push(inspect_data_dir(
        active_data_dir.as_deref().unwrap_or(default_data_dir),
        if active_data_dir.is_some() {
            CandidateRole::Active
        } else {
            CandidateRole::Default
        },
        database_file,
    )?);
    let orphan_v2_candidate_id = if config.is_none() {
        let orphan = inspect_candidate(
            &default_data_dir.join(DATABASE_FILE_V2),
            CandidateRole::Located,
        )?;
        let candidate_id = include_candidate(&orphan).then(|| orphan.candidate.id.clone());
        inspected.push(orphan);
        candidate_id
    } else {
        None
    };
    if let Some(config) = &config {
        for (data_dir, role) in [
            (&config.source_data_dir, CandidateRole::Source),
            (&config.pending_data_dir, CandidateRole::Pending),
        ] {
            push_config_candidate(&mut inspected, data_dir, role, database_file)?;
        }
    }
    push_owned_backup_candidates(&mut inspected, default_data_dir, database_file)?;
    dedupe_candidates(&mut inspected);

    let facts: Vec<_> = inspected
        .iter()
        .filter(|c| include_candidate(c))
        .map(candidate_facts)
        .collect();
    let active = active_data_dir.as_ref().and_then(|dir| {
        let path = dir.join(database_file).display().to_string();
        inspected
            .iter()
            .find(|c| c.candidate.role == CandidateRole::Active && c.candidate.path == path)
            .map(candidate_facts)
    });
    let candidates = inspected
        .into_iter()
        .filter(include_candidate)
        .map(|c| c.candidate)
        .collect();
    let decision = decide_startup(&DecisionInput {
        initialized,
        active,
        candidates: facts,
        pending_relocation,
        default_data_dir: default_data_dir.to_path_buf(),
    });
    let decision = match decision {
        StartupDecision::Ready { candidate_id }
            if orphan_v2_candidate_id.as_ref() == Some(&candidate_id) =>
        {
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::UpgradeRecoveryRequired,
            }
        }
        other => other,
    };
    Ok(DataStoreStartupState::new(
        decision,
        candidates,
        default_data_dir.to_path_buf(),
        relocation_intent,
    )
    .with_database_generation(generation))
}

fn inspect_with_unreadable_config(
    default_data_dir: &Path,
) -> Result<DataStoreStartupState, String> {
    let candidate = inspect_data_dir(
        default_data_dir,
        CandidateRole::Default,
        DatabaseGeneration::One.database_file(),
    )?;
    let candidates = include_candidate(&candidate)
        .then_some(candidate.candidate)
        .into_iter()
        .collect();
    Ok(DataStoreStartupState::new(
        StartupDecision::NeedsRecovery {
            reason: RecoveryReason::Unreadable,
        },
        candidates,
        default_data_dir.to_path_buf(),
        None,
    )
    .with_database_generation(DatabaseGeneration::One))
}

fn trusted_relocation_intent(
    config: &DataDirConfigV3,
    database_file: &str,
) -> Option<DataStoreRelocationIntent> {
    if config.version != 3 || config.active_data_dir != config.source_data_dir {
        return None;
    }
    let source_data_dir = config.source_data_dir.clone()?;
    let target_data_dir = config.pending_data_dir.clone()?;
    (!target_data_dir.join(database_file).exists()).then_some(DataStoreRelocationIntent {
        source_data_dir,
        target_data_dir,
    })
}

fn inspect_data_dir(
    data_dir: &Path,
    role: CandidateRole,
    database_file: &str,
) -> Result<InspectedDataStoreCandidate, String> {
    inspect_candidate(&data_dir.join(database_file), role)
}

fn push_config_candidate(
    inspected: &mut Vec<InspectedDataStoreCandidate>,
    data_dir: &Option<PathBuf>,
    role: CandidateRole,
    database_file: &str,
) -> Result<(), String> {
    if let Some(data_dir) = data_dir {
        inspected.push(inspect_data_dir(data_dir, role, database_file)?);
    }
    Ok(())
}

fn push_owned_backup_candidates(
    inspected: &mut Vec<InspectedDataStoreCandidate>,
    default_data_dir: &Path,
    database_file: &str,
) -> Result<(), String> {
    let backups_root = default_data_dir.join("backups");
    if !backups_root.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(&backups_root).map_err(|e| {
        format!(
            "failed to read backup directory {}: {e}",
            backups_root.display()
        )
    })? {
        let path = entry
            .map_err(|e| format!("failed to read backup entry: {e}"))?
            .path();
        if path.is_dir() {
            inspected.push(inspect_candidate(
                &path.join(database_file),
                CandidateRole::Backup,
            )?);
        }
    }
    Ok(())
}

fn include_candidate(candidate: &InspectedDataStoreCandidate) -> bool {
    candidate.candidate.health != CandidateHealth::Missing
        || candidate.candidate.role == CandidateRole::Active
}

fn candidate_facts(candidate: &InspectedDataStoreCandidate) -> CandidateFacts {
    CandidateFacts {
        id: candidate.candidate.id.clone(),
        role: candidate.candidate.role.clone(),
        health: candidate.candidate.health.clone(),
        contains_relay_pool_schema: candidate.contains_relay_pool_schema,
        schema_compatible: candidate.candidate.schema_compatible,
    }
}

fn dedupe_candidates(candidates: &mut Vec<InspectedDataStoreCandidate>) {
    let mut seen = std::collections::BTreeSet::new();
    candidates.retain(|candidate| seen.insert(candidate.candidate.path.clone()));
}

#[cfg(test)]
#[rustfmt::skip]
mod tests {
    use super::{inspect_startup, DATABASE_FILE};
    use crate::services::data_store::{
        config::{
            create_installation_marker, write_config, DataDirConfigV2, DatabaseGeneration,
        },
        types::{CandidateRole, DataStoreRelocationIntent, RecoveryReason, StartupDecision},
    };
    use std::{fs, path::{Path, PathBuf}, time::SystemTime};

    #[test]
    fn startup_orchestration_discovers_without_mutation_or_trust_leaks() {
        let first_run = temp_root("first-run").join("missing");
        let state = inspect_startup(&first_run).expect("startup");
        assert!(matches!(state.decision, StartupDecision::FirstRun { .. }));
        assert!(state.candidates.is_empty() && !first_run.exists());

        let dir = temp_root("marker-missing");
        create_installation_marker(&dir).expect("marker");
        save_v2(&dir, Some(dir.join("active")), None, None);
        let state = inspect_startup(&dir).expect("startup");
        assert!(matches!(state.decision, StartupDecision::NeedsRecovery { reason: RecoveryReason::Missing }));
        assert!(state.candidates.iter().any(|c| c.role == CandidateRole::Active));

        let legacy = temp_root("legacy-default");
        create_db(&legacy);
        let state = inspect_startup(&legacy).expect("legacy");
        assert!(matches!(state.decision, StartupDecision::Ready { .. }) && !config_path(&legacy).exists());

        let dir = temp_root("healthy-custom-active");
        let active = dir.join("custom-active");
        create_db(&active);
        save_v2(&dir, Some(active.clone()), None, None);
        let state = inspect_startup(&dir).expect("custom active");
        assert!(matches!(state.decision, StartupDecision::Ready { .. }));
        assert!(state.candidates.iter().any(|candidate| candidate.role == CandidateRole::Active && candidate.path == active.join(DATABASE_FILE).display().to_string()));

        let conflict = temp_root("conflict");
        let source = conflict.join("source");
        create_db(&conflict);
        create_db(&source);
        save_v2(&conflict, None, None, Some(source));
        assert!(matches!(inspect_startup(&conflict).expect("conflict").decision, StartupDecision::Conflict { .. }));

        let conflict = temp_root("default-source-pending-conflict");
        let source = conflict.join("source");
        let pending = conflict.join("pending");
        create_db(&conflict);
        create_db(&source);
        create_db(&pending);
        save_v2(&conflict, None, Some(pending), Some(source));
        assert!(matches!(inspect_startup(&conflict).expect("three-way conflict").decision, StartupDecision::Conflict { .. }));

        let dir = temp_root("empty-pending-populated-source");
        let source = dir.join("source");
        let pending = dir.join("pending");
        create_db(&source);
        fs::create_dir_all(&pending).expect("pending dir");
        save_v2(&dir, Some(source.clone()), Some(pending.clone()), Some(source.clone()));
        let state = inspect_startup(&dir).expect("empty pending");
        assert!(matches!(state.decision, StartupDecision::Ready { .. }));
        assert_eq!(state.relocation_intent, Some(DataStoreRelocationIntent { source_data_dir: source, target_data_dir: pending }));

        let dir = temp_root("legacy-pending");
        let source = dir.join("source");
        let pending = dir.join("pending");
        create_db(&source);
        fs::create_dir_all(&dir).expect("dir");
        fs::write(config_path(&dir), legacy_v1(&pending, &source)).expect("config");
        assert!(matches!(inspect_startup(&dir).expect("startup").decision, StartupDecision::NeedsRecovery { reason: RecoveryReason::PendingRelocation }));

        let dir = temp_root("trusted-v2");
        let source = dir.join("source");
        let pending = dir.join("pending");
        create_db(&source);
        save_v2(&dir, Some(source.clone()), Some(pending.clone()), Some(source.clone()));
        let state = inspect_startup(&dir).expect("startup");
        assert_eq!(state.relocation_intent, Some(DataStoreRelocationIntent { source_data_dir: source, target_data_dir: pending }));
        let view_json = serde_json::to_string(&state.view()).expect("view");
        assert!(matches!(state.decision, StartupDecision::Ready { .. }) && view_json.contains("ready"));
        assert!(!view_json.contains("relocationIntent") && !view_json.contains("targetDataDir"));

        let dir = temp_root("backups");
        create_db(&dir.join("backups").join("direct"));
        create_db(&dir.join("backups").join("nested").join("too-deep"));
        let state = inspect_startup(&dir).expect("startup");
        let count = state.candidates.iter().filter(|c| c.role == CandidateRole::Backup).count();
        assert_eq!(count, 1);
    }

    #[test]
    fn startup_orchestration_recovers_from_truncated_config_without_opening_database() {
        let dir = temp_root("truncated-config");
        create_db(&dir);
        fs::write(config_path(&dir), r#"{"version":2,"activeDataDir":"#).expect("config");

        let state = inspect_startup(&dir).expect("startup recovery state");

        assert!(matches!(state.decision, StartupDecision::NeedsRecovery { reason: RecoveryReason::Unreadable }));
        assert_eq!(state.candidates.len(), 1);
        assert_eq!(state.candidates[0].role, CandidateRole::Default);
    }

    #[test]
    fn generation_one_config_is_not_overridden_by_a_residual_v2_file() {
        let dir = temp_root("generation-authority");
        create_db(&dir);
        save_v2(&dir, Some(dir.clone()), None, None);
        fs::write(
            dir.join(DatabaseGeneration::Two.database_file()),
            b"untrusted residual v2 bytes",
        )
        .expect("residual v2 file");

        let state = inspect_startup(&dir).expect("generation-one startup");

        assert_eq!(state.database_generation(), DatabaseGeneration::One);
        assert!(matches!(state.decision, StartupDecision::Ready { .. }));
        assert!(state
            .candidates
            .iter()
            .all(|candidate| !candidate.path.ends_with(DatabaseGeneration::Two.database_file())));
    }

    #[test]
    fn orphan_v2_without_config_is_recovery_evidence_not_an_automatic_first_run() {
        let dir = temp_root("orphan-v2");
        create_db_file(&dir.join(DatabaseGeneration::Two.database_file()));

        let state = inspect_startup(&dir).expect("orphan V2 startup");

        assert_eq!(state.database_generation(), DatabaseGeneration::One);
        assert!(matches!(
            state.decision,
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::UpgradeRecoveryRequired
            }
        ));
        assert_eq!(state.candidates.len(), 1);
        assert_eq!(state.candidates[0].role, CandidateRole::Located);
        assert!(state.candidates[0]
            .path
            .ends_with(DatabaseGeneration::Two.database_file()));
    }

    fn create_db(data_dir: &Path) {
        fs::create_dir_all(data_dir).expect("data dir");
        create_db_file(&data_dir.join(DATABASE_FILE));
    }

    fn create_db_file(path: &Path) {
        fs::create_dir_all(path.parent().expect("database parent")).expect("data dir");
        let mut db = crate::services::data_store::test_support::open_database(path);
        crate::services::data_store::test_support::execute_batch(
            &mut db,
            "CREATE TABLE stations(name TEXT); INSERT INTO stations VALUES ('station')",
        );
        crate::services::data_store::test_support::close_database(db);
    }

    fn config_path(dir: &Path) -> PathBuf { dir.join("relay-pool-data-dir.json") }
    fn save_v2(dir: &Path, active_data_dir: Option<PathBuf>, pending_data_dir: Option<PathBuf>, source_data_dir: Option<PathBuf>) {
        write_config(&config_path(dir), &v2(active_data_dir, pending_data_dir, source_data_dir)).expect("config");
    }

    fn v2(active_data_dir: Option<PathBuf>, pending_data_dir: Option<PathBuf>, source_data_dir: Option<PathBuf>) -> DataDirConfigV2 {
        DataDirConfigV2 { version: 2, active_data_dir, pending_data_dir, source_data_dir, updated_at: "2026-07-17T00:00:00Z".to_string() }
    }
    fn temp_root(name: &str) -> PathBuf {
        let unique = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).expect("time").as_nanos();
        std::env::temp_dir().join(format!("relay-pool-startup-{name}-{unique}"))
    }
    fn slash(path: &Path) -> String { path.to_string_lossy().replace('\\', "/") }

    fn legacy_v1(pending: &Path, source: &Path) -> String {
        format!(r#"{{"pendingDataDir":"{}","sourceDataDir":"{}"}}"#, slash(pending), slash(source))
    }
}
