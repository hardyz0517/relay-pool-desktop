use std::{
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

use serde::Serialize;

use super::{
    config::{installation_marker_exists, read_config},
    types::{CandidateHealth, CandidateRole, DataStoreStartupState, StartupDecision},
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataStoreDiagnosticReport {
    pub generated_at: String,
    pub config_version: Option<u32>,
    pub config_read_error: Option<&'static str>,
    pub installation_marker: bool,
    pub decision: DiagnosticDecision,
    pub candidates: Vec<DiagnosticCandidate>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum DiagnosticDecision {
    Ready,
    FirstRun,
    NeedsRecovery {
        reason: super::types::RecoveryReason,
    },
    Conflict {
        candidate_count: usize,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DiagnosticCandidate {
    pub anonymous_id: String,
    pub role: CandidateRole,
    pub health: CandidateHealth,
    pub schema_compatible: bool,
    pub size_bytes: Option<u64>,
    pub modified_at: Option<String>,
    pub counts: std::collections::BTreeMap<String, i64>,
}

pub(crate) fn build_diagnostic_report(
    default_data_dir: &Path,
    state: &DataStoreStartupState,
) -> Result<DataStoreDiagnosticReport, String> {
    let (config, config_read_error) =
        match read_config(&default_data_dir.join("relay-pool-data-dir.json")) {
            Ok(config) => (config, None),
            Err(_) => (None, Some("unreadable")),
        };
    Ok(DataStoreDiagnosticReport {
        generated_at: updated_at(),
        config_version: config.map(|config| config.version),
        config_read_error,
        installation_marker: installation_marker_exists(default_data_dir),
        decision: diagnostic_decision(&state.decision),
        candidates: state
            .candidates
            .iter()
            .enumerate()
            .map(|(index, candidate)| DiagnosticCandidate {
                anonymous_id: format!("candidate-{}", index + 1),
                role: candidate.role.clone(),
                health: candidate.health.clone(),
                schema_compatible: candidate.schema_compatible,
                size_bytes: candidate.size_bytes,
                modified_at: candidate.modified_at.clone(),
                counts: candidate.counts.clone(),
            })
            .collect(),
    })
}

fn diagnostic_decision(decision: &StartupDecision) -> DiagnosticDecision {
    match decision {
        StartupDecision::Ready { .. } => DiagnosticDecision::Ready,
        StartupDecision::FirstRun { .. } => DiagnosticDecision::FirstRun,
        StartupDecision::NeedsRecovery { reason } => DiagnosticDecision::NeedsRecovery {
            reason: reason.clone(),
        },
        StartupDecision::Conflict { candidate_ids } => DiagnosticDecision::Conflict {
            candidate_count: candidate_ids.len(),
        },
    }
}

fn updated_at() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs().to_string())
        .unwrap_or_else(|_| "0".to_string())
}

#[cfg(test)]
mod tests {
    use super::{build_diagnostic_report, DiagnosticDecision};
    use crate::services::data_store::types::{
        CandidateHealth, CandidateRole, DataStoreCandidate, DataStoreStartupState, RecoveryReason,
        StartupDecision,
    };
    use std::{collections::BTreeMap, fs, time::SystemTime};

    #[test]
    fn diagnostic_report_redacts_paths_names_urls_keys_cookies_and_secret_material() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("relay-pool-data-dir.json"),
            r#"{"version":2,"activeDataDir":null,"pendingDataDir":null,"sourceDataDir":null,"updatedAt":"0"}"#,
        )
        .expect("config");
        fs::write(root.join("installation.marker"), b"").expect("marker");
        let mut counts = BTreeMap::new();
        counts.insert("stations".to_string(), 1);
        let state = DataStoreStartupState::new(
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::Missing,
            },
            vec![DataStoreCandidate {
                id: "active:D:/Users/Alice/RelayPool/relay-pool-desktop.sqlite3".to_string(),
                role: CandidateRole::Active,
                path: "D:/Users/Alice/RelayPool/relay-pool-desktop.sqlite3".to_string(),
                health: CandidateHealth::Healthy,
                schema_compatible: true,
                size_bytes: Some(4096),
                modified_at: Some("2026-07-17T00:00:00Z".to_string()),
                counts,
            }],
            root.clone(),
            None,
        );

        let report = build_diagnostic_report(&root, &state).expect("report");
        let serialized = serde_json::to_string(&report).expect("serialize");

        assert!(serialized.contains("candidate-1"));
        assert!(serialized.contains("stations"));
        for secret in [
            "Alice",
            "RelayPool/relay-pool-desktop.sqlite3",
            "Sensitive Station",
            "https://secret.example/v1",
            "sk-sensitive",
            "session-cookie",
            "ciphertext",
            "aad",
        ] {
            assert!(!serialized.contains(secret), "leaked {secret}");
        }
    }

    #[test]
    fn diagnostic_decision_redacts_first_run_path_and_conflict_candidate_ids() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("relay-pool-data-dir.json"),
            r#"{"version":2,"activeDataDir":null,"pendingDataDir":null,"sourceDataDir":null,"updatedAt":"0"}"#,
        )
        .expect("config");

        let first_run = DataStoreStartupState::new(
            StartupDecision::FirstRun {
                default_data_dir: "D:/Users/Alice/AppData/Roaming/Relay Pool".into(),
            },
            Vec::new(),
            root.clone(),
            None,
        );
        let first_run_report = build_diagnostic_report(&root, &first_run).expect("first run");
        assert_eq!(first_run_report.decision, DiagnosticDecision::FirstRun);
        let first_run_json = serde_json::to_string(&first_run_report).expect("first run json");
        assert!(!first_run_json.contains("Alice"));
        assert!(!first_run_json.contains("AppData"));
        assert!(!first_run_json.contains("defaultDataDir"));

        let conflict = DataStoreStartupState::new(
            StartupDecision::Conflict {
                candidate_ids: vec![
                    "active:D:/Users/Alice/RelayPool/relay-pool-desktop.sqlite3".to_string(),
                    "source:E:/Sensitive/relay-pool-desktop.sqlite3".to_string(),
                ],
            },
            Vec::new(),
            root.clone(),
            None,
        );
        let conflict_report = build_diagnostic_report(&root, &conflict).expect("conflict");
        assert_eq!(
            conflict_report.decision,
            DiagnosticDecision::Conflict { candidate_count: 2 }
        );
        let conflict_json = serde_json::to_string(&conflict_report).expect("conflict json");
        assert!(conflict_json.contains("candidateCount"));
        assert!(!conflict_json.contains("candidateIds"));
        assert!(!conflict_json.contains("Alice"));
        assert!(!conflict_json.contains("Sensitive"));
        assert!(!conflict_json.contains("relay-pool-desktop.sqlite3"));
    }

    #[test]
    fn diagnostic_report_survives_truncated_config_without_leaking_parse_error_path() {
        let root = temp_root();
        fs::create_dir_all(&root).expect("root");
        fs::write(
            root.join("relay-pool-data-dir.json"),
            r#"{"version":2,"activeDataDir":"#,
        )
        .expect("config");
        let state = DataStoreStartupState::new(
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::Unreadable,
            },
            Vec::new(),
            root.clone(),
            None,
        );

        let report = build_diagnostic_report(&root, &state).expect("report");
        let serialized = serde_json::to_string(&report).expect("serialize");

        assert_eq!(report.config_version, None);
        assert_eq!(report.config_read_error, Some("unreadable"));
        assert!(!serialized.contains(&root.to_string_lossy().to_string()));
        assert!(!serialized.contains("relay-pool-data-dir.json"));
        assert!(!serialized.contains("expected"));
    }

    fn temp_root() -> std::path::PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("relay-pool-diagnostic-{unique}"))
    }
}
