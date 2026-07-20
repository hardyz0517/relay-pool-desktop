//! Recovery-only command boundary.
//!
//! This module deliberately contains DTOs and authorization decisions only.
//! Existing commands remain the production adapter until the complete V2
//! composition swap; no command in this module is registered yet.

use serde::Serialize;

use crate::services::data_store::{
    config::DatabaseGeneration,
    types::{CandidateHealth, CandidateRole, DataStoreStartupState, StartupDecision},
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RecoveryRuntimeMode {
    Writable,
    InspectionOnly,
    Recovery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryAction {
    Backup,
    ExportDiagnostic,
    CheckForUpdates,
    LocateCandidate,
    ActivateCandidate,
    CreateDataStore,
    NormalApplication,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataRecoveryCapabilities {
    pub can_backup: bool,
    pub can_export_diagnostic: bool,
    pub can_check_for_updates: bool,
    pub can_locate_candidate: bool,
    pub can_activate_candidate: bool,
    pub can_create_data_store: bool,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct SchemaCompatibilityView {
    pub decision_code: &'static str,
    pub schema_version: Option<i64>,
    pub app_version: &'static str,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataStoreCandidateView {
    pub id: String,
    pub role: CandidateRole,
    pub path: String,
    pub health: CandidateHealth,
    pub database_generation: Option<DatabaseGeneration>,
    pub compatibility: Option<SchemaCompatibilityView>,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub counts: std::collections::BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum StartupDecisionView {
    Ready {
        candidate_id: String,
    },
    InspectionOnly {
        candidate_id: String,
        reason: &'static str,
    },
    FirstRun {
        default_data_dir: String,
    },
    NeedsRecovery {
        reason: crate::services::data_store::types::RecoveryReason,
    },
    Conflict {
        candidate_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataStoreStartupView {
    pub mode: RecoveryRuntimeMode,
    pub database_generation: DatabaseGeneration,
    pub compatibility: Option<SchemaCompatibilityView>,
    pub capabilities: DataRecoveryCapabilities,
    pub decision: StartupDecisionView,
    pub candidates: Vec<DataStoreCandidateView>,
}

pub(crate) fn startup_view(state: &DataStoreStartupState) -> DataStoreStartupView {
    let mode = match &state.decision {
        StartupDecision::Ready { .. } => RecoveryRuntimeMode::Writable,
        _ => RecoveryRuntimeMode::Recovery,
    };
    DataStoreStartupView {
        mode,
        database_generation: state.database_generation(),
        compatibility: None,
        capabilities: capabilities_for(mode),
        decision: decision_view(&state.decision),
        candidates: state
            .candidates
            .iter()
            .map(|candidate| DataStoreCandidateView {
                id: candidate.id.clone(),
                role: candidate.role.clone(),
                path: candidate.path.clone(),
                health: candidate.health.clone(),
                database_generation: Some(state.database_generation()),
                compatibility: None,
                size_bytes: candidate.size_bytes,
                modified_at: candidate.modified_at.clone(),
                counts: candidate.counts.clone(),
            })
            .collect(),
    }
}

pub(crate) fn is_action_allowed(mode: RecoveryRuntimeMode, action: RecoveryAction) -> bool {
    match mode {
        RecoveryRuntimeMode::Writable => matches!(action, RecoveryAction::NormalApplication),
        RecoveryRuntimeMode::InspectionOnly => matches!(
            action,
            RecoveryAction::Backup
                | RecoveryAction::ExportDiagnostic
                | RecoveryAction::CheckForUpdates
        ),
        RecoveryRuntimeMode::Recovery => matches!(
            action,
            RecoveryAction::Backup
                | RecoveryAction::ExportDiagnostic
                | RecoveryAction::CheckForUpdates
                | RecoveryAction::LocateCandidate
                | RecoveryAction::ActivateCandidate
                | RecoveryAction::CreateDataStore
        ),
    }
}

fn capabilities_for(mode: RecoveryRuntimeMode) -> DataRecoveryCapabilities {
    DataRecoveryCapabilities {
        can_backup: is_action_allowed(mode, RecoveryAction::Backup),
        can_export_diagnostic: is_action_allowed(mode, RecoveryAction::ExportDiagnostic),
        can_check_for_updates: is_action_allowed(mode, RecoveryAction::CheckForUpdates),
        can_locate_candidate: is_action_allowed(mode, RecoveryAction::LocateCandidate),
        can_activate_candidate: is_action_allowed(mode, RecoveryAction::ActivateCandidate),
        can_create_data_store: is_action_allowed(mode, RecoveryAction::CreateDataStore),
    }
}

fn decision_view(decision: &StartupDecision) -> StartupDecisionView {
    match decision {
        StartupDecision::Ready { candidate_id } => StartupDecisionView::Ready {
            candidate_id: candidate_id.clone(),
        },
        StartupDecision::FirstRun { default_data_dir } => StartupDecisionView::FirstRun {
            default_data_dir: default_data_dir.display().to_string(),
        },
        StartupDecision::NeedsRecovery { reason } => StartupDecisionView::NeedsRecovery {
            reason: reason.clone(),
        },
        StartupDecision::Conflict { candidate_ids } => StartupDecisionView::Conflict {
            candidate_ids: candidate_ids.clone(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{is_action_allowed, RecoveryAction, RecoveryRuntimeMode};

    #[test]
    fn inspection_only_is_read_only() {
        assert!(is_action_allowed(
            RecoveryRuntimeMode::InspectionOnly,
            RecoveryAction::Backup
        ));
        assert!(!is_action_allowed(
            RecoveryRuntimeMode::InspectionOnly,
            RecoveryAction::ActivateCandidate
        ));
        assert!(!is_action_allowed(
            RecoveryRuntimeMode::InspectionOnly,
            RecoveryAction::NormalApplication
        ));
    }

    #[test]
    fn recovery_allows_only_explicit_recovery_actions() {
        assert!(is_action_allowed(
            RecoveryRuntimeMode::Recovery,
            RecoveryAction::ActivateCandidate
        ));
        assert!(!is_action_allowed(
            RecoveryRuntimeMode::Recovery,
            RecoveryAction::NormalApplication
        ));
    }
}
