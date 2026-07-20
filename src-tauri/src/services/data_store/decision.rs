use std::path::PathBuf;

use super::types::{CandidateHealth, CandidateRole, RecoveryReason, StartupDecision};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CandidateFacts {
    pub id: String,
    pub role: CandidateRole,
    pub health: CandidateHealth,
    pub contains_relay_pool_schema: bool,
    pub schema_compatible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionInput {
    pub initialized: bool,
    pub active: Option<CandidateFacts>,
    pub candidates: Vec<CandidateFacts>,
    pub pending_relocation: bool,
    pub default_data_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupStep {
    Lease,
    ConfigInspection,
    RecoveryPlanning,
    LegacyDetection,
    BackupImportValidation,
    Tombstone,
    GenerationCommit,
    V2Reopen,
    HealthCheck,
    AppServices,
    Proxy,
    Collectors,
    Monitors,
}

pub(crate) const fn startup_order() -> &'static [StartupStep] {
    &[
        StartupStep::Lease,
        StartupStep::ConfigInspection,
        StartupStep::RecoveryPlanning,
        StartupStep::LegacyDetection,
        StartupStep::BackupImportValidation,
        StartupStep::Tombstone,
        StartupStep::GenerationCommit,
        StartupStep::V2Reopen,
        StartupStep::HealthCheck,
        StartupStep::AppServices,
        StartupStep::Proxy,
        StartupStep::Collectors,
        StartupStep::Monitors,
    ]
}

pub(crate) fn decide_startup(input: &DecisionInput) -> StartupDecision {
    if input.pending_relocation {
        return StartupDecision::NeedsRecovery {
            reason: RecoveryReason::PendingRelocation,
        };
    }

    let protected_healthy_candidates = protected_healthy_candidates(&input.candidates);
    if protected_healthy_candidates.len() > 1 {
        return StartupDecision::Conflict {
            candidate_ids: protected_healthy_candidates,
        };
    }

    if let Some(active) = &input.active {
        return if candidate_is_ready(active) {
            StartupDecision::Ready {
                candidate_id: active.id.clone(),
            }
        } else {
            StartupDecision::NeedsRecovery {
                reason: recovery_reason_for_health(&active.health),
            }
        };
    }

    if !input.initialized && input.candidates.is_empty() {
        return StartupDecision::FirstRun {
            default_data_dir: input.default_data_dir.clone(),
        };
    }

    if let [candidate_id] = protected_healthy_candidates.as_slice() {
        return StartupDecision::Ready {
            candidate_id: candidate_id.clone(),
        };
    }

    StartupDecision::NeedsRecovery {
        reason: input
            .candidates
            .iter()
            .find(|candidate| candidate.health != CandidateHealth::Healthy)
            .map(|candidate| recovery_reason_for_health(&candidate.health))
            .unwrap_or(RecoveryReason::Missing),
    }
}

fn protected_healthy_candidates(candidates: &[CandidateFacts]) -> Vec<String> {
    candidates
        .iter()
        .filter(|candidate| candidate_is_ready(candidate))
        .map(|candidate| candidate.id.clone())
        .collect()
}

fn candidate_is_ready(candidate: &CandidateFacts) -> bool {
    candidate.health == CandidateHealth::Healthy
        && candidate.contains_relay_pool_schema
        && candidate.schema_compatible
}

fn recovery_reason_for_health(health: &CandidateHealth) -> RecoveryReason {
    match health {
        CandidateHealth::Healthy => RecoveryReason::OpenOrMigrationFailed,
        CandidateHealth::Missing => RecoveryReason::Missing,
        CandidateHealth::Unreadable => RecoveryReason::Unreadable,
        CandidateHealth::InvalidSqlite => RecoveryReason::InvalidSqlite,
        CandidateHealth::IntegrityFailed => RecoveryReason::IntegrityFailed,
    }
}
#[cfg(test)]
mod tests {
    use super::{decide_startup, startup_order, CandidateFacts, DecisionInput, StartupStep};
    use crate::services::data_store::types::{
        CandidateHealth, CandidateRole, RecoveryReason, StartupDecision,
    };

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum StartupDecisionTag {
        Ready,
        FirstRun,
        NeedsRecovery,
        Conflict,
    }

    impl StartupDecision {
        fn tag(&self) -> StartupDecisionTag {
            match self {
                StartupDecision::Ready { .. } => StartupDecisionTag::Ready,
                StartupDecision::FirstRun { .. } => StartupDecisionTag::FirstRun,
                StartupDecision::NeedsRecovery { .. } => StartupDecisionTag::NeedsRecovery,
                StartupDecision::Conflict { .. } => StartupDecisionTag::Conflict,
            }
        }
    }
    #[test]
    fn startup_decision_table_is_closed_and_deterministic() {
        let cases = [
            (
                "first run only when nothing is initialized or discovered",
                input(false, None, Vec::new(), false),
                StartupDecisionTag::FirstRun,
            ),
            (
                "healthy configured active database is ready",
                input(
                    true,
                    Some(healthy_candidate("active")),
                    vec![healthy_candidate("active")],
                    false,
                ),
                StartupDecisionTag::Ready,
            ),
            (
                "initialized install with missing active database needs recovery",
                input(
                    true,
                    Some(unhealthy_active(CandidateHealth::Missing)),
                    Vec::new(),
                    false,
                ),
                StartupDecisionTag::NeedsRecovery,
            ),
            (
                "one healthy unmarked legacy database is ready after open succeeds",
                input(false, None, vec![healthy_candidate("default")], false),
                StartupDecisionTag::Ready,
            ),
            (
                "two healthy protected databases require conflict resolution",
                input(
                    true,
                    Some(healthy_candidate("active")),
                    vec![healthy_candidate("active"), healthy_candidate("source")],
                    false,
                ),
                StartupDecisionTag::Conflict,
            ),
            (
                "corrupt active database needs recovery",
                input(
                    true,
                    Some(unhealthy_active(CandidateHealth::InvalidSqlite)),
                    Vec::new(),
                    false,
                ),
                StartupDecisionTag::NeedsRecovery,
            ),
            (
                "legacy pending relocation needs recovery before any automatic move",
                input(
                    true,
                    Some(healthy_candidate("source")),
                    vec![healthy_candidate("source")],
                    true,
                ),
                StartupDecisionTag::NeedsRecovery,
            ),
        ];

        for (name, input, expected) in cases {
            assert_eq!(decide_startup(&input).tag(), expected, "{}", name);
        }
    }
    #[test]
    fn pending_relocation_reports_specific_recovery_reason() {
        let decision = decide_startup(&DecisionInput {
            initialized: true,
            active: Some(healthy_candidate("source")),
            candidates: vec![healthy_candidate("source")],
            pending_relocation: true,
            default_data_dir: "C:/RelayPool/AppData".into(),
        });

        assert_eq!(
            decision,
            StartupDecision::NeedsRecovery {
                reason: RecoveryReason::PendingRelocation
            }
        );
    }

    #[test]
    fn startup_order_places_runtime_registration_after_v2_health() {
        let order = startup_order();
        assert!(
            order.iter().position(|step| *step == StartupStep::V2Reopen)
                < order
                    .iter()
                    .position(|step| *step == StartupStep::AppServices)
        );
        assert!(
            order
                .iter()
                .position(|step| *step == StartupStep::HealthCheck)
                < order.iter().position(|step| *step == StartupStep::Proxy)
        );
        assert!(
            order.iter().position(|step| *step == StartupStep::Proxy)
                < order
                    .iter()
                    .position(|step| *step == StartupStep::Collectors)
        );
        assert!(
            order
                .iter()
                .position(|step| *step == StartupStep::Collectors)
                < order.iter().position(|step| *step == StartupStep::Monitors)
        );
    }
    fn input(
        initialized: bool,
        active: Option<CandidateFacts>,
        candidates: Vec<CandidateFacts>,
        pending_relocation: bool,
    ) -> DecisionInput {
        DecisionInput {
            initialized,
            active,
            candidates,
            pending_relocation,
            default_data_dir: "C:/RelayPool/AppData".into(),
        }
    }

    fn healthy_candidate(id: &str) -> CandidateFacts {
        CandidateFacts {
            id: id.into(),
            role: CandidateRole::Default,
            health: CandidateHealth::Healthy,
            contains_relay_pool_schema: true,
            schema_compatible: true,
        }
    }

    fn unhealthy_active(health: CandidateHealth) -> CandidateFacts {
        CandidateFacts {
            id: "active".into(),
            role: CandidateRole::Active,
            health,
            contains_relay_pool_schema: false,
            schema_compatible: false,
        }
    }
}
