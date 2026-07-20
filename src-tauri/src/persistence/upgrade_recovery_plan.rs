use super::upgrade_journal::UpgradePhase;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JournalState {
    Missing,
    Invalid,
    Valid(UpgradePhase),
}

impl JournalState {
    const ALL: [Self; 8] = [
        Self::Missing,
        Self::Invalid,
        Self::Valid(UpgradePhase::Prepared),
        Self::Valid(UpgradePhase::BackupVerified),
        Self::Valid(UpgradePhase::V2Validated),
        Self::Valid(UpgradePhase::LegacyDeactivated),
        Self::Valid(UpgradePhase::GenerationCommitted),
        Self::Valid(UpgradePhase::V2Reopened),
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigGeneration {
    Generation1,
    Generation2,
    Unknown,
}

impl ConfigGeneration {
    const ALL: [Self; 3] = [Self::Generation1, Self::Generation2, Self::Unknown];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacySourceState {
    Generation1,
    ValidTombstone,
    Missing,
    Unknown,
}

impl LegacySourceState {
    const ALL: [Self; 4] = [
        Self::Generation1,
        Self::ValidTombstone,
        Self::Missing,
        Self::Unknown,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BackupState {
    Missing,
    Verified,
    Invalid,
}

impl BackupState {
    const ALL: [Self; 3] = [Self::Missing, Self::Verified, Self::Invalid];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum V2CandidateState {
    Missing,
    InactiveTemporary,
    ValidatedTemporary,
    ValidatedFinal,
    Invalid,
}

impl V2CandidateState {
    const ALL: [Self; 5] = [
        Self::Missing,
        Self::InactiveTemporary,
        Self::ValidatedTemporary,
        Self::ValidatedFinal,
        Self::Invalid,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompatibilityState {
    NotApplicable,
    Writable,
    Incompatible,
    Unknown,
}

impl CompatibilityState {
    const ALL: [Self; 4] = [
        Self::NotApplicable,
        Self::Writable,
        Self::Incompatible,
        Self::Unknown,
    ];
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LegacySidecarState {
    Absent,
    Present,
    Unknown,
}

impl LegacySidecarState {
    const ALL: [Self; 3] = [Self::Absent, Self::Present, Self::Unknown];
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ObservedUpgradeState {
    pub(crate) journal: JournalState,
    pub(crate) config_generation: ConfigGeneration,
    pub(crate) source: LegacySourceState,
    pub(crate) backup: BackupState,
    pub(crate) candidate: V2CandidateState,
    pub(crate) relocation_intent: bool,
    pub(crate) compatibility: CompatibilityState,
    pub(crate) orphan_artifacts: bool,
    pub(crate) sidecars: LegacySidecarState,
}

impl ObservedUpgradeState {
    #[cfg(test)]
    pub(crate) fn finite_test_matrix() -> Vec<Self> {
        let mut states = Vec::with_capacity(69_120);
        for journal in JournalState::ALL {
            for config_generation in ConfigGeneration::ALL {
                for source in LegacySourceState::ALL {
                    for backup in BackupState::ALL {
                        for candidate in V2CandidateState::ALL {
                            for relocation_intent in [false, true] {
                                for compatibility in CompatibilityState::ALL {
                                    for orphan_artifacts in [false, true] {
                                        for sidecars in LegacySidecarState::ALL {
                                            states.push(Self {
                                                journal,
                                                config_generation,
                                                source,
                                                backup,
                                                candidate,
                                                relocation_intent,
                                                compatibility,
                                                orphan_artifacts,
                                                sidecars,
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        states
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryPlan {
    RestartFromSource,
    RebuildV2FromVerifiedBackup,
    ActivateValidatedV2,
    RecordLegacyDeactivated,
    RestoreGeneration1,
    CommitGeneration2,
    ReopenGeneration2,
    CleanupCompletedJournal,
    Halt(RecoveryHaltReason),
}

impl RecoveryPlan {
    pub(crate) fn is_executable(self) -> bool {
        !matches!(self, Self::Halt(_))
    }

    pub(crate) fn kind_code(self) -> &'static str {
        match self {
            Self::RestartFromSource => "restart_from_source",
            Self::RebuildV2FromVerifiedBackup => "rebuild_v2_from_verified_backup",
            Self::ActivateValidatedV2 => "activate_validated_v2",
            Self::RecordLegacyDeactivated => "record_legacy_deactivated",
            Self::RestoreGeneration1 => "restore_generation_1",
            Self::CommitGeneration2 => "commit_generation_2",
            Self::ReopenGeneration2 => "reopen_generation_2",
            Self::CleanupCompletedJournal => "cleanup_completed_journal",
            Self::Halt(_) => "halt",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RecoveryHaltReason {
    NoUpgradeInProgress,
    InvalidJournal,
    RelocationConflict,
    OrphanArtifacts,
    ConfigGenerationNotRecognized,
    SourceNotRecognized,
    BackupNotVerified,
    CandidateNotValidated,
    SidecarStateNotRecognized,
    V2NotWritable,
    UserDecisionRequired,
    InvalidPhaseObservation,
}

impl RecoveryHaltReason {
    pub(crate) fn as_code(self) -> &'static str {
        match self {
            Self::NoUpgradeInProgress => "no_upgrade_in_progress",
            Self::InvalidJournal => "invalid_journal",
            Self::RelocationConflict => "relocation_conflict",
            Self::OrphanArtifacts => "orphan_artifacts",
            Self::ConfigGenerationNotRecognized => "config_generation_not_recognized",
            Self::SourceNotRecognized => "source_not_recognized",
            Self::BackupNotVerified => "backup_not_verified",
            Self::CandidateNotValidated => "candidate_not_validated",
            Self::SidecarStateNotRecognized => "sidecar_state_not_recognized",
            Self::V2NotWritable => "v2_not_writable",
            Self::UserDecisionRequired => "user_decision_required",
            Self::InvalidPhaseObservation => "invalid_phase_observation",
        }
    }
}

pub(crate) struct RecoveryPlanner;

impl RecoveryPlanner {
    pub(crate) fn plan(state: ObservedUpgradeState) -> RecoveryPlan {
        if state.relocation_intent {
            return RecoveryPlan::Halt(RecoveryHaltReason::RelocationConflict);
        }
        match state.journal {
            JournalState::Invalid => return RecoveryPlan::Halt(RecoveryHaltReason::InvalidJournal),
            JournalState::Missing if state.orphan_artifacts => {
                return RecoveryPlan::Halt(RecoveryHaltReason::OrphanArtifacts)
            }
            JournalState::Missing => {
                return RecoveryPlan::Halt(RecoveryHaltReason::NoUpgradeInProgress)
            }
            JournalState::Valid(_) if state.orphan_artifacts => {
                return RecoveryPlan::Halt(RecoveryHaltReason::OrphanArtifacts)
            }
            JournalState::Valid(_) => {}
        }
        if state.config_generation == ConfigGeneration::Unknown {
            return RecoveryPlan::Halt(RecoveryHaltReason::ConfigGenerationNotRecognized);
        }
        if matches!(
            state.source,
            LegacySourceState::Missing | LegacySourceState::Unknown
        ) {
            return RecoveryPlan::Halt(RecoveryHaltReason::SourceNotRecognized);
        }
        if state.sidecars == LegacySidecarState::Unknown {
            return RecoveryPlan::Halt(RecoveryHaltReason::SidecarStateNotRecognized);
        }
        if state.backup == BackupState::Invalid {
            return RecoveryPlan::Halt(RecoveryHaltReason::BackupNotVerified);
        }
        if state.candidate == V2CandidateState::Invalid {
            return RecoveryPlan::Halt(RecoveryHaltReason::CandidateNotValidated);
        }

        let JournalState::Valid(phase) = state.journal else {
            unreachable!("missing and invalid journal states returned above")
        };
        match phase {
            UpgradePhase::Prepared => plan_prepared(&state),
            UpgradePhase::BackupVerified => plan_backup_verified(&state),
            UpgradePhase::V2Validated => plan_v2_validated(&state),
            UpgradePhase::LegacyDeactivated => plan_legacy_deactivated(&state),
            UpgradePhase::GenerationCommitted => plan_generation_committed(&state),
            UpgradePhase::V2Reopened => plan_v2_reopened(&state),
        }
    }
}

fn plan_prepared(state: &ObservedUpgradeState) -> RecoveryPlan {
    if state.config_generation == ConfigGeneration::Generation1
        && state.source == LegacySourceState::Generation1
        && state.backup == BackupState::Missing
        && matches!(
            state.candidate,
            V2CandidateState::Missing | V2CandidateState::InactiveTemporary
        )
        && state.compatibility == CompatibilityState::NotApplicable
    {
        RecoveryPlan::RestartFromSource
    } else {
        RecoveryPlan::Halt(RecoveryHaltReason::InvalidPhaseObservation)
    }
}

fn plan_backup_verified(state: &ObservedUpgradeState) -> RecoveryPlan {
    if state.config_generation == ConfigGeneration::Generation1
        && state.source == LegacySourceState::Generation1
        && state.backup == BackupState::Verified
        && matches!(
            state.candidate,
            V2CandidateState::Missing | V2CandidateState::InactiveTemporary
        )
        && state.compatibility == CompatibilityState::NotApplicable
    {
        RecoveryPlan::RebuildV2FromVerifiedBackup
    } else {
        phase_failure(state)
    }
}

fn plan_v2_validated(state: &ObservedUpgradeState) -> RecoveryPlan {
    if state.config_generation != ConfigGeneration::Generation1 {
        return RecoveryPlan::Halt(RecoveryHaltReason::InvalidPhaseObservation);
    }
    if state.backup != BackupState::Verified {
        return RecoveryPlan::Halt(RecoveryHaltReason::BackupNotVerified);
    }
    if state.compatibility != CompatibilityState::Writable {
        return RecoveryPlan::Halt(RecoveryHaltReason::V2NotWritable);
    }
    match (state.source, state.candidate) {
        (
            LegacySourceState::Generation1,
            V2CandidateState::ValidatedTemporary | V2CandidateState::ValidatedFinal,
        ) => RecoveryPlan::ActivateValidatedV2,
        (LegacySourceState::ValidTombstone, V2CandidateState::ValidatedFinal) => {
            RecoveryPlan::RecordLegacyDeactivated
        }
        (_, V2CandidateState::Missing | V2CandidateState::InactiveTemporary) => {
            RecoveryPlan::Halt(RecoveryHaltReason::CandidateNotValidated)
        }
        _ => RecoveryPlan::Halt(RecoveryHaltReason::InvalidPhaseObservation),
    }
}

fn plan_legacy_deactivated(state: &ObservedUpgradeState) -> RecoveryPlan {
    if state.source != LegacySourceState::ValidTombstone {
        return RecoveryPlan::Halt(RecoveryHaltReason::InvalidPhaseObservation);
    }
    if state.backup != BackupState::Verified {
        return RecoveryPlan::Halt(RecoveryHaltReason::BackupNotVerified);
    }
    if state.candidate != V2CandidateState::ValidatedFinal {
        return RecoveryPlan::Halt(RecoveryHaltReason::CandidateNotValidated);
    }
    if state.compatibility != CompatibilityState::Writable {
        return RecoveryPlan::Halt(RecoveryHaltReason::V2NotWritable);
    }
    match state.config_generation {
        ConfigGeneration::Generation1 => {
            RecoveryPlan::Halt(RecoveryHaltReason::UserDecisionRequired)
        }
        ConfigGeneration::Generation2 => RecoveryPlan::CommitGeneration2,
        ConfigGeneration::Unknown => {
            RecoveryPlan::Halt(RecoveryHaltReason::ConfigGenerationNotRecognized)
        }
    }
}

fn plan_generation_committed(state: &ObservedUpgradeState) -> RecoveryPlan {
    if generation_2_is_authoritative(state) {
        RecoveryPlan::ReopenGeneration2
    } else {
        phase_failure(state)
    }
}

fn plan_v2_reopened(state: &ObservedUpgradeState) -> RecoveryPlan {
    if generation_2_is_authoritative(state) && state.sidecars == LegacySidecarState::Absent {
        RecoveryPlan::CleanupCompletedJournal
    } else {
        phase_failure(state)
    }
}

fn generation_2_is_authoritative(state: &ObservedUpgradeState) -> bool {
    state.config_generation == ConfigGeneration::Generation2
        && state.source == LegacySourceState::ValidTombstone
        && state.backup == BackupState::Verified
        && state.candidate == V2CandidateState::ValidatedFinal
        && state.compatibility == CompatibilityState::Writable
}

fn phase_failure(state: &ObservedUpgradeState) -> RecoveryPlan {
    if state.backup != BackupState::Verified
        && !matches!(state.journal, JournalState::Valid(UpgradePhase::Prepared))
    {
        RecoveryPlan::Halt(RecoveryHaltReason::BackupNotVerified)
    } else if matches!(
        state.compatibility,
        CompatibilityState::Incompatible | CompatibilityState::Unknown
    ) {
        RecoveryPlan::Halt(RecoveryHaltReason::V2NotWritable)
    } else if matches!(
        state.candidate,
        V2CandidateState::Missing | V2CandidateState::InactiveTemporary
    ) {
        RecoveryPlan::Halt(RecoveryHaltReason::CandidateNotValidated)
    } else {
        RecoveryPlan::Halt(RecoveryHaltReason::InvalidPhaseObservation)
    }
}
