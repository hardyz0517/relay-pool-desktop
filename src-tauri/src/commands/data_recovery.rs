//! Recovery-only command boundary.
//!
//! This module deliberately contains DTOs and authorization decisions only.
//! Production commands map startup evidence through this module so the UI and
//! backend share one closed recovery contract.

use serde::Serialize;

use crate::services::data_store::{
    config::DatabaseGeneration,
    types::{
        CandidateHealth, CandidateRole, DataStoreCandidate, DataStoreStartupState, StartupDecision,
    },
};

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum RecoveryRuntimeMode {
    Writable,
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "inspection-only authorization is a tested release contract"
        )
    )]
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
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "normal-mode authorization is a tested release contract"
        )
    )]
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
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum StartupDecisionView {
    Ready {
        candidate_id: String,
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
    let mode = match (&state.decision, state.database_generation()) {
        (StartupDecision::Ready { .. }, DatabaseGeneration::Two) => RecoveryRuntimeMode::Writable,
        _ => RecoveryRuntimeMode::Recovery,
    };
    DataStoreStartupView {
        mode,
        database_generation: state.database_generation(),
        compatibility: (mode == RecoveryRuntimeMode::Writable).then_some(SchemaCompatibilityView {
            decision_code: "writable",
            schema_version: Some(
                crate::services::data_store::generation_upgrade::current_schema_version(),
            ),
            app_version: env!("CARGO_PKG_VERSION"),
        }),
        capabilities: capabilities_for(mode),
        decision: decision_view(&state.decision),
        candidates: state.candidates.iter().map(candidate_view).collect(),
    }
}

pub(crate) fn candidate_view(candidate: &DataStoreCandidate) -> DataStoreCandidateView {
    DataStoreCandidateView {
        id: candidate.id.clone(),
        role: candidate.role.clone(),
        path: candidate.path.clone(),
        health: candidate.health.clone(),
        database_generation: candidate_generation(candidate),
        compatibility: (candidate.health == CandidateHealth::Healthy
            && candidate.schema_compatible)
            .then_some(SchemaCompatibilityView {
                decision_code: "writable",
                schema_version: None,
                app_version: env!("CARGO_PKG_VERSION"),
            }),
        size_bytes: candidate.size_bytes,
        modified_at: candidate.modified_at.clone(),
        counts: candidate.counts.clone(),
    }
}

fn candidate_generation(candidate: &DataStoreCandidate) -> Option<DatabaseGeneration> {
    std::path::Path::new(&candidate.path)
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| match name {
            "relay-pool-desktop.sqlite3" => Some(DatabaseGeneration::One),
            "relay-pool-desktop-v2.sqlite3" => Some(DatabaseGeneration::Two),
            _ => None,
        })
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
    use super::{
        is_action_allowed, startup_view, RecoveryAction, RecoveryRuntimeMode, StartupDecisionView,
    };
    use crate::services::data_store::{
        config::DatabaseGeneration,
        types::{
            CandidateHealth, CandidateRole, DataStoreCandidate, DataStoreStartupState,
            StartupDecision,
        },
    };

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

    #[test]
    fn production_startup_view_matches_the_frontend_recovery_contract() {
        let candidate = DataStoreCandidate {
            id: "candidate-v2".to_string(),
            role: CandidateRole::Active,
            path: std::path::PathBuf::from("data")
                .join(DatabaseGeneration::Two.database_file())
                .display()
                .to_string(),
            health: CandidateHealth::Healthy,
            schema_compatible: true,
            size_bytes: Some(42),
            modified_at: None,
            counts: std::collections::BTreeMap::new(),
        };
        let state = DataStoreStartupState::new(
            StartupDecision::Ready {
                candidate_id: candidate.id.clone(),
            },
            vec![candidate],
            std::path::PathBuf::from("data"),
            None,
        )
        .with_database_generation(DatabaseGeneration::Two);

        let value = serde_json::to_value(startup_view(&state)).expect("serialize startup view");

        assert_eq!(value["mode"], "writable");
        assert_eq!(value["databaseGeneration"], "two");
        assert_eq!(value["compatibility"]["decisionCode"], "writable");
        assert_eq!(value["decision"]["kind"], "ready");
        assert_eq!(value["decision"]["candidateId"], "candidate-v2");
        assert!(value["decision"].get("candidate_id").is_none());
        assert_eq!(value["candidates"][0]["databaseGeneration"], "two");
        assert_eq!(
            value["candidates"][0]["compatibility"]["decisionCode"],
            "writable"
        );
        assert_eq!(value["capabilities"]["canActivateCandidate"], false);
    }

    #[test]
    fn startup_decision_fields_are_camel_case_for_every_frontend_variant() {
        let first_run = serde_json::to_value(StartupDecisionView::FirstRun {
            default_data_dir: "data".to_string(),
        })
        .expect("serialize first-run decision");
        let conflict = serde_json::to_value(StartupDecisionView::Conflict {
            candidate_ids: vec!["candidate-v1".to_string(), "candidate-v2".to_string()],
        })
        .expect("serialize conflict decision");

        assert_eq!(first_run["defaultDataDir"], "data");
        assert!(first_run.get("default_data_dir").is_none());
        assert_eq!(
            conflict["candidateIds"],
            serde_json::json!(["candidate-v1", "candidate-v2"])
        );
        assert!(conflict.get("candidate_ids").is_none());
    }
}
