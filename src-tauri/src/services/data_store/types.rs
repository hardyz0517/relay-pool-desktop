use std::{
    collections::BTreeMap,
    fmt,
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
    MissingKey,
    KeyMismatch,
    CorruptedDatabase,
    InterruptedUpgrade,
    SchemaMigrationFailed,
    RoutingPolicyMigrationInvalid,
    AlertingUpgradeFailed,
    SecretBaselineFailed,
    InternalUpgradeError,
    UnsupportedSchemaVersion,
    InconsistentSchemaMetadata,
    PendingRelocation,
    SystemCredentialMissing,
    SystemCredentialUnavailable,
    SystemCredentialPermissionDenied,
    SystemCredentialCorrupt,
    SystemCredentialUnsupported,
    SystemCredentialInternal,
    PortableMigrationManualRecoveryRequired,
    PortableMigrationKeyUnavailable,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum StartupUpgradeStage {
    Probe,
    Migrate,
    Validate,
    Ready,
    Blocked,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct StartupUpgradeStatus {
    pub stage: StartupUpgradeStage,
    pub current_schema_version: Option<i64>,
    pub target_schema_version: i64,
    pub failure_reason: Option<RecoveryReason>,
    pub failure_stage: Option<StartupUpgradeStage>,
}

impl StartupUpgradeStatus {
    fn initial() -> Self {
        Self {
            stage: StartupUpgradeStage::Probe,
            current_schema_version: None,
            target_schema_version: crate::persistence::current_schema_version(),
            failure_reason: None,
            failure_stage: None,
        }
    }

    fn blocked(
        current_schema_version: Option<i64>,
        reason: RecoveryReason,
        failure_stage: StartupUpgradeStage,
    ) -> Self {
        Self {
            stage: StartupUpgradeStage::Blocked,
            current_schema_version,
            target_schema_version: crate::persistence::current_schema_version(),
            failure_reason: Some(reason),
            failure_stage: Some(failure_stage),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StartupUpgradeError {
    reason: RecoveryReason,
    message: String,
}

impl StartupUpgradeError {
    pub(crate) fn new(reason: RecoveryReason, message: impl Into<String>) -> Self {
        Self {
            reason,
            message: message.into(),
        }
    }

    pub(crate) fn recovery_reason(&self) -> RecoveryReason {
        self.reason.clone()
    }
}

impl fmt::Display for StartupUpgradeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl From<StartupUpgradeError> for String {
    fn from(error: StartupUpgradeError) -> Self {
        error.to_string()
    }
}

impl From<String> for StartupUpgradeError {
    fn from(message: String) -> Self {
        Self::new(RecoveryReason::InternalUpgradeError, message)
    }
}

impl From<&str> for StartupUpgradeError {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
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
    pub schema_version: Option<i64>,
    pub schema_metadata_consistent: bool,
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

#[cfg(test)]
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
    startup_upgrade: StartupUpgradeStatus,
}

impl DataStoreStartupState {
    pub(crate) fn new(
        decision: StartupDecision,
        candidates: Vec<DataStoreCandidate>,
        default_data_dir: PathBuf,
        relocation_intent: Option<DataStoreRelocationIntent>,
    ) -> Self {
        let startup_upgrade = match &decision {
            StartupDecision::NeedsRecovery { reason } => {
                StartupUpgradeStatus::blocked(None, reason.clone(), StartupUpgradeStage::Probe)
            }
            StartupDecision::Ready { .. }
            | StartupDecision::FirstRun { .. }
            | StartupDecision::Conflict { .. } => StartupUpgradeStatus::initial(),
        };
        Self {
            decision,
            candidates,
            default_data_dir,
            relocation_intent,
            database_generation: DatabaseGeneration::Two,
            startup_upgrade,
        }
    }

    pub(crate) fn with_database_generation(mut self, generation: DatabaseGeneration) -> Self {
        self.database_generation = generation;
        self
    }

    pub(crate) fn database_generation(&self) -> DatabaseGeneration {
        self.database_generation
    }

    pub(crate) fn startup_upgrade(&self) -> &StartupUpgradeStatus {
        &self.startup_upgrade
    }

    pub(crate) fn set_startup_upgrade_stage(
        &mut self,
        stage: StartupUpgradeStage,
        current_schema_version: Option<i64>,
    ) {
        debug_assert!(
            stage != StartupUpgradeStage::Blocked,
            "blocked startup upgrades must carry a typed recovery reason"
        );
        self.startup_upgrade = StartupUpgradeStatus {
            stage,
            current_schema_version,
            target_schema_version: crate::persistence::current_schema_version(),
            failure_reason: None,
            failure_stage: None,
        };
    }

    pub(crate) fn enter_recovery(
        &mut self,
        reason: RecoveryReason,
        current_schema_version: Option<i64>,
    ) {
        let failure_stage = match self.startup_upgrade.stage {
            StartupUpgradeStage::Probe
            | StartupUpgradeStage::Migrate
            | StartupUpgradeStage::Validate => self.startup_upgrade.stage.clone(),
            StartupUpgradeStage::Ready | StartupUpgradeStage::Blocked => StartupUpgradeStage::Probe,
        };
        self.decision = StartupDecision::NeedsRecovery {
            reason: reason.clone(),
        };
        self.startup_upgrade =
            StartupUpgradeStatus::blocked(current_schema_version, reason, failure_stage);
    }

    #[cfg(test)]
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

#[cfg(test)]
mod tests {
    use super::{DataStoreStartupState, RecoveryReason, StartupDecision, StartupUpgradeStage};
    use std::path::PathBuf;

    #[test]
    fn recovery_decision_always_exposes_a_blocked_typed_upgrade_status() {
        let mut state = DataStoreStartupState::new(
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::Missing,
            },
            Vec::new(),
            PathBuf::from("C:/RelayPool"),
            None,
        );

        assert_eq!(state.startup_upgrade().stage, StartupUpgradeStage::Blocked);
        assert_eq!(
            state.startup_upgrade().failure_reason,
            Some(RecoveryReason::Missing)
        );
        assert_eq!(
            state.startup_upgrade().failure_stage,
            Some(StartupUpgradeStage::Probe)
        );

        state.enter_recovery(RecoveryReason::KeyMismatch, Some(15));

        assert_eq!(state.startup_upgrade().stage, StartupUpgradeStage::Blocked);
        assert_eq!(state.startup_upgrade().current_schema_version, Some(15));
        assert_eq!(
            state.startup_upgrade().failure_reason,
            Some(RecoveryReason::KeyMismatch)
        );
        assert_eq!(
            state.startup_upgrade().failure_stage,
            Some(StartupUpgradeStage::Probe)
        );
    }

    #[test]
    fn recovery_retains_the_typed_stage_where_upgrade_failed() {
        let mut state = DataStoreStartupState::new(
            StartupDecision::Ready {
                candidate_id: "active".to_string(),
            },
            Vec::new(),
            PathBuf::from("C:/RelayPool"),
            None,
        );
        state.set_startup_upgrade_stage(StartupUpgradeStage::Migrate, Some(55));

        state.enter_recovery(RecoveryReason::SchemaMigrationFailed, Some(55));

        assert_eq!(state.startup_upgrade().stage, StartupUpgradeStage::Blocked);
        assert_eq!(
            state.startup_upgrade().failure_stage,
            Some(StartupUpgradeStage::Migrate)
        );
    }
}
