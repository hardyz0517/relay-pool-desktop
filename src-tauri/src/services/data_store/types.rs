use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use serde::Serialize;

use super::config::DatabaseGeneration;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum RecoveryReason {
    Missing,
    Unreadable,
    InvalidSqlite,
    IntegrityFailed,
    OpenOrMigrationFailed,
    PendingRelocation,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CandidateHealth {
    Healthy,
    Missing,
    Unreadable,
    InvalidSqlite,
    IntegrityFailed,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum CandidateRole {
    Active,
    Default,
    Source,
    Pending,
    Backup,
    Located,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataStoreCandidate {
    pub id: String,
    pub role: CandidateRole,
    pub path: String,
    pub health: CandidateHealth,
    pub schema_compatible: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub counts: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StartupDecision {
    Ready { candidate_id: String },
    FirstRun { default_data_dir: PathBuf },
    NeedsRecovery { reason: RecoveryReason },
    Conflict { candidate_ids: Vec<String> },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DataStoreStartupView {
    pub decision: StartupDecision,
    pub candidates: Vec<DataStoreCandidate>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivationResult {
    pub restart_required: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DataStoreRelocationIntent {
    pub source_data_dir: PathBuf,
    pub target_data_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct DataStoreStartupState {
    pub decision: StartupDecision,
    pub candidates: Vec<DataStoreCandidate>,
    default_data_dir: PathBuf,
    pub(crate) relocation_intent: Option<DataStoreRelocationIntent>,
    database_generation: DatabaseGeneration,
}

impl DataStoreStartupState {
    pub(crate) fn new(
        decision: StartupDecision,
        candidates: Vec<DataStoreCandidate>,
        default_data_dir: PathBuf,
        relocation_intent: Option<DataStoreRelocationIntent>,
    ) -> Self {
        Self {
            decision,
            candidates,
            default_data_dir,
            relocation_intent,
            database_generation: DatabaseGeneration::One,
        }
    }

    pub(crate) fn with_database_generation(mut self, generation: DatabaseGeneration) -> Self {
        self.database_generation = generation;
        self
    }

    pub(crate) fn database_generation(&self) -> DatabaseGeneration {
        self.database_generation
    }

    pub fn view(&self) -> DataStoreStartupView {
        DataStoreStartupView {
            decision: self.decision.clone(),
            candidates: self.candidates.clone(),
        }
    }

    pub(crate) fn default_data_dir(&self) -> &Path {
        &self.default_data_dir
    }
}
