use std::{collections::BTreeMap, path::PathBuf};

use serde::Serialize;

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
