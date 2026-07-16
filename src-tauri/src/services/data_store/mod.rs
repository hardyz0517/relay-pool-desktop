pub mod backup;
pub mod config;
pub mod decision;
pub mod inspect;
pub mod types;

use std::{
    fs,
    path::{Path, PathBuf},
};

use config::{installation_marker_exists, read_config, DataDirConfigV2};
use decision::{decide_startup, CandidateFacts, DecisionInput};
use inspect::{inspect_candidate, InspectedDataStoreCandidate};
use types::{CandidateHealth, CandidateRole, DataStoreRelocationIntent, DataStoreStartupState};

const DATA_DIR_CONFIG_FILE: &str = "relay-pool-data-dir.json";
const DATABASE_FILE: &str = "relay-pool-desktop.sqlite3";

pub fn inspect_startup(default_data_dir: &Path) -> Result<DataStoreStartupState, String> {
    let config = read_config(&default_data_dir.join(DATA_DIR_CONFIG_FILE))?;
    let initialized = config.is_some() || installation_marker_exists(default_data_dir);
    let relocation_intent = config.as_ref().and_then(trusted_relocation_intent);
    let pending_relocation = config
        .as_ref()
        .is_some_and(|c| c.version == 1 && c.pending_data_dir != c.source_data_dir);
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
    )?);
    if let Some(config) = &config {
        for (data_dir, role) in [
            (&config.source_data_dir, CandidateRole::Source),
            (&config.pending_data_dir, CandidateRole::Pending),
        ] {
            push_config_candidate(&mut inspected, data_dir, role)?;
        }
    }
    push_owned_backup_candidates(&mut inspected, default_data_dir)?;
    dedupe_candidates(&mut inspected);

    let facts: Vec<_> = inspected
        .iter()
        .filter(|c| include_candidate(c))
        .map(candidate_facts)
        .collect();
    let active = active_data_dir.as_ref().and_then(|dir| {
        let path = dir.join(DATABASE_FILE).display().to_string();
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
    Ok(DataStoreStartupState::new(
        decision,
        candidates,
        default_data_dir.to_path_buf(),
        relocation_intent,
    ))
}

fn trusted_relocation_intent(config: &DataDirConfigV2) -> Option<DataStoreRelocationIntent> {
    if config.version != 2 || config.active_data_dir != config.source_data_dir {
        return None;
    }
    let source_data_dir = config.source_data_dir.clone()?;
    let target_data_dir = config.pending_data_dir.clone()?;
    (!target_data_dir.join(DATABASE_FILE).exists()).then_some(DataStoreRelocationIntent {
        source_data_dir,
        target_data_dir,
    })
}

fn inspect_data_dir(
    data_dir: &Path,
    role: CandidateRole,
) -> Result<InspectedDataStoreCandidate, String> {
    inspect_candidate(&data_dir.join(DATABASE_FILE), role)
}

fn push_config_candidate(
    inspected: &mut Vec<InspectedDataStoreCandidate>,
    data_dir: &Option<PathBuf>,
    role: CandidateRole,
) -> Result<(), String> {
    if let Some(data_dir) = data_dir {
        inspected.push(inspect_data_dir(data_dir, role)?);
    }
    Ok(())
}

fn push_owned_backup_candidates(
    inspected: &mut Vec<InspectedDataStoreCandidate>,
    default_data_dir: &Path,
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
                &path.join(DATABASE_FILE),
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
        config::{create_installation_marker, write_config, DataDirConfigV2},
        types::{CandidateRole, DataStoreRelocationIntent, RecoveryReason, StartupDecision},
    };
    use rusqlite::Connection;
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

        let conflict = temp_root("conflict");
        let source = conflict.join("source");
        create_db(&conflict);
        create_db(&source);
        save_v2(&conflict, None, None, Some(source));
        assert!(matches!(inspect_startup(&conflict).expect("conflict").decision, StartupDecision::Conflict { .. }));

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

    fn create_db(data_dir: &Path) {
        fs::create_dir_all(data_dir).expect("data dir");
        let db = Connection::open(data_dir.join(DATABASE_FILE)).expect("db");
        db.execute_batch("CREATE TABLE stations(name TEXT); INSERT INTO stations VALUES ('station')").expect("fixture");
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
