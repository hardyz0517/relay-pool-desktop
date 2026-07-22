mod persistence {
    pub(crate) mod upgrade_fault {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/upgrade_fault.rs"
        ));
    }

    pub(crate) mod upgrade_journal {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/upgrade_journal.rs"
        ));
    }

    pub(crate) mod upgrade_recovery_plan {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/upgrade_recovery_plan.rs"
        ));
    }

    pub(crate) mod upgrade_recovery_executor {
        include!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/persistence/upgrade_recovery_executor.rs"
        ));
    }
}

use persistence::{
    upgrade_fault::NoUpgradeFaults,
    upgrade_journal::{
        JournalArtifactPaths, ReleasedSchemaProfile, Sha256Digest, UpgradeAttemptId,
        UpgradeJournal, UpgradeJournalPayload, UpgradePhase, UtcTimestamp, JOURNAL_VERSION,
    },
    upgrade_recovery_executor::{
        journal_artifact_paths_are_safe, observe_backup, observe_journal, observe_legacy_source,
        publish_v2_candidate_with_faults, read_legacy_tombstone,
        remove_file_and_sync_parent_with_faults, replace_legacy_with_tombstone,
        resolve_allowlisted_artifact, write_journal_atomically,
        write_journal_atomically_with_faults, RecoveryExecution, RecoveryExecutor,
        UpgradeExecutionError, UpgradeRecoveryIo, UPGRADE_JOURNAL_FILE,
    },
    upgrade_recovery_plan::{
        BackupState, CompatibilityState, ConfigGeneration, JournalState, LegacySidecarState,
        LegacySourceState, ObservedUpgradeState, RecoveryHaltReason, RecoveryPlan, RecoveryPlanner,
        V2CandidateState,
    },
};
use std::{cell::Cell, fs};

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const HASH_C: &str = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc";

#[test]
fn every_observed_upgrade_state_has_one_deterministic_plan_or_halt() {
    let mut count = 0;
    for state in ObservedUpgradeState::finite_test_matrix() {
        let first = RecoveryPlanner::plan(state.clone());
        let second = RecoveryPlanner::plan(state.clone());
        assert_eq!(first, second, "planner must be deterministic for {state:?}");
        assert!(first.is_executable() || matches!(first, RecoveryPlan::Halt(_)));
        count += 1;
    }

    assert_eq!(count, 69_120, "matrix dimensions changed without review");
}

#[test]
fn planner_accepts_only_the_specified_recovery_observations() {
    let cases = [
        (
            observed(UpgradePhase::Prepared),
            RecoveryPlan::RestartFromSource,
        ),
        (
            ObservedUpgradeState {
                journal: JournalState::Valid(UpgradePhase::BackupVerified),
                backup: BackupState::Verified,
                ..observed(UpgradePhase::BackupVerified)
            },
            RecoveryPlan::RebuildV2FromVerifiedBackup,
        ),
        (
            ObservedUpgradeState {
                journal: JournalState::Valid(UpgradePhase::V2Validated),
                backup: BackupState::Verified,
                candidate: V2CandidateState::ValidatedTemporary,
                compatibility: CompatibilityState::Writable,
                ..observed(UpgradePhase::V2Validated)
            },
            RecoveryPlan::ActivateValidatedV2,
        ),
        (
            ObservedUpgradeState {
                journal: JournalState::Valid(UpgradePhase::V2Validated),
                source: LegacySourceState::ValidTombstone,
                backup: BackupState::Verified,
                candidate: V2CandidateState::ValidatedFinal,
                compatibility: CompatibilityState::Writable,
                sidecars: LegacySidecarState::Present,
                ..observed(UpgradePhase::V2Validated)
            },
            RecoveryPlan::RecordLegacyDeactivated,
        ),
        (
            ObservedUpgradeState {
                journal: JournalState::Valid(UpgradePhase::LegacyDeactivated),
                config_generation: ConfigGeneration::Generation2,
                source: LegacySourceState::ValidTombstone,
                backup: BackupState::Verified,
                candidate: V2CandidateState::ValidatedFinal,
                compatibility: CompatibilityState::Writable,
                ..observed(UpgradePhase::LegacyDeactivated)
            },
            RecoveryPlan::CommitGeneration2,
        ),
        (
            ObservedUpgradeState {
                journal: JournalState::Valid(UpgradePhase::GenerationCommitted),
                config_generation: ConfigGeneration::Generation2,
                source: LegacySourceState::ValidTombstone,
                backup: BackupState::Verified,
                candidate: V2CandidateState::ValidatedFinal,
                compatibility: CompatibilityState::Writable,
                ..observed(UpgradePhase::GenerationCommitted)
            },
            RecoveryPlan::ReopenGeneration2,
        ),
        (
            ObservedUpgradeState {
                journal: JournalState::Valid(UpgradePhase::V2Reopened),
                config_generation: ConfigGeneration::Generation2,
                source: LegacySourceState::ValidTombstone,
                backup: BackupState::Verified,
                candidate: V2CandidateState::ValidatedFinal,
                compatibility: CompatibilityState::Writable,
                ..observed(UpgradePhase::V2Reopened)
            },
            RecoveryPlan::CleanupCompletedJournal,
        ),
    ];

    for (state, expected) in cases {
        assert_eq!(RecoveryPlanner::plan(state), expected);
    }
}

#[test]
fn planner_halts_on_ambiguous_or_unsafe_observations() {
    let mut cases = Vec::new();

    cases.push((
        ObservedUpgradeState {
            relocation_intent: true,
            ..observed(UpgradePhase::Prepared)
        },
        RecoveryHaltReason::RelocationConflict,
    ));
    cases.push((
        ObservedUpgradeState {
            journal: JournalState::Invalid,
            ..observed(UpgradePhase::Prepared)
        },
        RecoveryHaltReason::InvalidJournal,
    ));
    cases.push((
        ObservedUpgradeState {
            orphan_artifacts: true,
            ..observed(UpgradePhase::Prepared)
        },
        RecoveryHaltReason::OrphanArtifacts,
    ));
    cases.push((
        ObservedUpgradeState {
            journal: JournalState::Valid(UpgradePhase::V2Validated),
            source: LegacySourceState::Missing,
            backup: BackupState::Verified,
            candidate: V2CandidateState::ValidatedFinal,
            compatibility: CompatibilityState::Writable,
            ..observed(UpgradePhase::V2Validated)
        },
        RecoveryHaltReason::SourceNotRecognized,
    ));
    cases.push((
        ObservedUpgradeState {
            journal: JournalState::Valid(UpgradePhase::LegacyDeactivated),
            source: LegacySourceState::ValidTombstone,
            backup: BackupState::Verified,
            candidate: V2CandidateState::ValidatedFinal,
            compatibility: CompatibilityState::Writable,
            ..observed(UpgradePhase::LegacyDeactivated)
        },
        RecoveryHaltReason::UserDecisionRequired,
    ));

    for (state, reason) in cases {
        assert_eq!(RecoveryPlanner::plan(state), RecoveryPlan::Halt(reason));
    }
}

#[test]
fn journal_round_trip_verifies_version_shape_paths_and_checksum() {
    let journal = valid_journal(UpgradePhase::BackupVerified);
    let encoded = journal.to_canonical_json().expect("valid journal");
    let decoded = UpgradeJournal::from_json(&encoded).expect("verified journal");

    assert_eq!(decoded, journal);
    assert_eq!(decoded.payload().journal_version, JOURNAL_VERSION);
    assert_eq!(
        decoded.payload().attempt_id.as_str(),
        "019f7d50-9d44-7000-8000-000000000001"
    );
    assert_eq!(decoded.payload().source_candidate_identity.as_str(), HASH_A);
    assert!(!encoded.windows(b"D:\\".len()).any(|bytes| bytes == b"D:\\"));
}

#[test]
fn invalid_journal_is_rejected_fail_closed() {
    let journal = valid_journal(UpgradePhase::BackupVerified);
    let encoded = journal.to_canonical_json().expect("valid journal");
    let mut value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");

    value["payload"]["journalVersion"] = serde_json::json!(99);
    assert!(UpgradeJournal::from_json(&serde_json::to_vec(&value).unwrap()).is_err());

    let mut value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");
    value["payload"]["paths"]["v2Temporary"] = serde_json::json!("../escape.sqlite3");
    assert!(UpgradeJournal::from_json(&serde_json::to_vec(&value).unwrap()).is_err());

    let mut value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");
    value["canonicalPayloadChecksum"] = serde_json::json!(HASH_B);
    assert!(UpgradeJournal::from_json(&serde_json::to_vec(&value).unwrap()).is_err());

    let mut value: serde_json::Value = serde_json::from_slice(&encoded).expect("json");
    value["payload"]["sourceCandidateIdentity"] = serde_json::json!("not-a-sha256");
    assert!(UpgradeJournal::from_json(&serde_json::to_vec(&value).unwrap()).is_err());
}

#[test]
fn changed_precondition_stops_destructive_execution_without_replanning() {
    let observed = ObservedUpgradeState {
        journal: JournalState::Valid(UpgradePhase::V2Validated),
        backup: BackupState::Verified,
        candidate: V2CandidateState::ValidatedTemporary,
        compatibility: CompatibilityState::Writable,
        ..observed(UpgradePhase::V2Validated)
    };
    let journal = valid_journal(UpgradePhase::V2Validated);
    let execution = RecoveryExecution::prepare(observed.clone(), Some(&journal)).expect("prepare");
    assert_eq!(execution.plan(), RecoveryPlan::ActivateValidatedV2);
    let io = FakeUpgradeIo::new(observed, journal).with_backup_hash(HASH_C);

    assert_eq!(
        RecoveryExecutor::execute(&io, &execution),
        Err(UpgradeExecutionError::RecoveryPreconditionChanged)
    );
    assert_eq!(io.destructive_calls(), 0);
}

#[test]
fn executor_dispatches_exactly_the_frozen_plan_after_full_revalidation() {
    let observed = ObservedUpgradeState {
        journal: JournalState::Valid(UpgradePhase::V2Validated),
        backup: BackupState::Verified,
        candidate: V2CandidateState::ValidatedTemporary,
        compatibility: CompatibilityState::Writable,
        ..observed(UpgradePhase::V2Validated)
    };
    let journal = valid_journal(UpgradePhase::V2Validated);
    let execution = RecoveryExecution::prepare(observed.clone(), Some(&journal)).expect("prepare");
    let io = FakeUpgradeIo::new(observed, journal);

    assert_eq!(
        RecoveryExecutor::execute(&io, &execution),
        Ok(RecoveryPlan::ActivateValidatedV2)
    );
    assert_eq!(io.destructive_calls(), 1);
    assert_eq!(io.last_plan.get(), Some(RecoveryPlan::ActivateValidatedV2));
}

#[test]
fn tombstone_replacement_is_self_verifying_and_removes_legacy_sidecars() {
    let root = tempfile::tempdir().expect("tempdir");
    let legacy = root.path().join("relay-pool-desktop.sqlite3");
    fs::write(&legacy, b"SQLite format 3\0legacy data").expect("legacy");
    fs::write(root.path().join("relay-pool-desktop.sqlite3-wal"), b"wal").expect("wal");
    fs::write(root.path().join("relay-pool-desktop.sqlite3-shm"), b"shm").expect("shm");
    let attempt = UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000001").unwrap();

    replace_legacy_with_tombstone(&legacy, &attempt).expect("replace with tombstone");

    assert_eq!(read_legacy_tombstone(&legacy).expect("read"), Some(attempt));
    assert!(!root.path().join("relay-pool-desktop.sqlite3-wal").exists());
    assert!(!root.path().join("relay-pool-desktop.sqlite3-shm").exists());
    let bytes = fs::read(&legacy).expect("tombstone bytes");
    assert!(!bytes.starts_with(b"SQLite format 3\0"));
}

#[test]
fn journal_writes_are_replaceable_and_artifact_paths_cannot_escape_root() {
    let root = tempfile::tempdir().expect("tempdir");
    let journal_path = root.path().join("persistence-upgrade-journal.json");
    let prepared = valid_journal(UpgradePhase::Prepared);
    let validated = valid_journal(UpgradePhase::V2Validated);

    write_journal_atomically(&journal_path, &prepared).expect("first write");
    write_journal_atomically(&journal_path, &validated).expect("replace");
    let persisted = UpgradeJournal::from_json(&fs::read(&journal_path).expect("journal bytes"))
        .expect("valid persisted journal");
    assert_eq!(persisted, validated);
    let observed_journal = observe_journal(&journal_path);
    assert_eq!(
        observed_journal.state,
        JournalState::Valid(UpgradePhase::V2Validated)
    );
    assert_eq!(observed_journal.journal, Some(validated.clone()));
    assert!(journal_artifact_paths_are_safe(root.path(), &validated));

    assert!(resolve_allowlisted_artifact(root.path(), "backups/attempt/db.sqlite3").is_ok());
    assert_eq!(
        resolve_allowlisted_artifact(root.path(), "../escape.sqlite3"),
        Err(UpgradeExecutionError::RecoveryPreconditionChanged)
    );
    assert_eq!(
        resolve_allowlisted_artifact(root.path(), "C:/escape.sqlite3"),
        Err(UpgradeExecutionError::RecoveryPreconditionChanged)
    );
}

#[test]
fn filesystem_observation_fails_closed_for_invalid_source_backup_and_journal() {
    let root = tempfile::tempdir().expect("tempdir");
    let source = root.path().join("relay-pool-desktop.sqlite3");
    let backup = root.path().join("backup.sqlite3");
    let journal = root.path().join("persistence-upgrade-journal.json");

    assert_eq!(observe_legacy_source(&source), LegacySourceState::Missing);
    fs::write(&source, b"not sqlite").expect("invalid source");
    assert_eq!(observe_legacy_source(&source), LegacySourceState::Unknown);
    fs::write(&source, b"SQLite format 3\0payload").expect("sqlite header");
    assert_eq!(
        observe_legacy_source(&source),
        LegacySourceState::Generation1
    );

    fs::write(&backup, b"backup").expect("backup");
    assert_eq!(
        observe_backup(&backup, &Sha256Digest::parse(HASH_B).unwrap()),
        BackupState::Invalid
    );
    fs::write(&journal, b"{truncated").expect("journal");
    assert_eq!(observe_journal(&journal).state, JournalState::Invalid);
}

#[test]
fn no_fault_file_wrappers_preserve_atomic_artifact_boundaries() {
    let root = tempfile::tempdir().expect("tempdir");
    let journal_path = root.path().join(UPGRADE_JOURNAL_FILE);
    let journal = valid_journal(UpgradePhase::Prepared);

    write_journal_atomically_with_faults(&journal_path, &journal, &NoUpgradeFaults)
        .expect("journal write");
    assert_eq!(
        observe_journal(&journal_path).state,
        JournalState::Valid(UpgradePhase::Prepared)
    );

    let temporary = root.path().join("candidate.sqlite3.tmp");
    let final_path = root.path().join("candidate.sqlite3");
    fs::write(&temporary, b"SQLite format 3\0candidate").expect("candidate");
    publish_v2_candidate_with_faults(&temporary, &final_path, &NoUpgradeFaults)
        .expect("candidate publish");
    assert!(!temporary.exists());
    assert!(final_path.is_file());

    remove_file_and_sync_parent_with_faults(&final_path, &NoUpgradeFaults)
        .expect("candidate cleanup");
    assert!(!final_path.exists());
}

struct FakeUpgradeIo {
    observed: ObservedUpgradeState,
    journal: UpgradeJournal,
    source_hash: Sha256Digest,
    backup_hash: Sha256Digest,
    destructive_calls: Cell<usize>,
    last_plan: Cell<Option<RecoveryPlan>>,
}

impl FakeUpgradeIo {
    fn new(observed: ObservedUpgradeState, journal: UpgradeJournal) -> Self {
        Self {
            observed,
            journal,
            source_hash: Sha256Digest::parse(HASH_A).unwrap(),
            backup_hash: Sha256Digest::parse(HASH_B).unwrap(),
            destructive_calls: Cell::new(0),
            last_plan: Cell::new(None),
        }
    }

    fn with_backup_hash(mut self, hash: &str) -> Self {
        self.backup_hash = Sha256Digest::parse(hash).unwrap();
        self
    }

    fn destructive_calls(&self) -> usize {
        self.destructive_calls.get()
    }

    fn record(&self, plan: RecoveryPlan) {
        self.destructive_calls.set(self.destructive_calls.get() + 1);
        self.last_plan.set(Some(plan));
    }
}

impl UpgradeRecoveryIo for FakeUpgradeIo {
    fn observe(&self) -> Result<ObservedUpgradeState, UpgradeExecutionError> {
        Ok(self.observed.clone())
    }

    fn load_journal(&self) -> Result<UpgradeJournal, UpgradeExecutionError> {
        Ok(self.journal.clone())
    }

    fn source_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError> {
        Ok(self.source_hash.clone())
    }

    fn backup_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError> {
        Ok(self.backup_hash.clone())
    }

    fn tombstone_attempt_id(&self) -> Result<UpgradeAttemptId, UpgradeExecutionError> {
        Ok(self.journal.payload().attempt_id.clone())
    }

    fn artifact_paths_are_safe(
        &self,
        _journal: &UpgradeJournal,
    ) -> Result<bool, UpgradeExecutionError> {
        Ok(true)
    }

    fn restart_from_source(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::RestartFromSource);
        Ok(())
    }

    fn rebuild_v2_from_verified_backup(
        &self,
        _: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::RebuildV2FromVerifiedBackup);
        Ok(())
    }

    fn activate_validated_v2(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::ActivateValidatedV2);
        Ok(())
    }

    fn record_legacy_deactivated(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::RecordLegacyDeactivated);
        Ok(())
    }

    fn restore_generation_1(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::RestoreGeneration1);
        Ok(())
    }

    fn commit_generation_2(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::CommitGeneration2);
        Ok(())
    }

    fn reopen_generation_2(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::ReopenGeneration2);
        Ok(())
    }

    fn cleanup_completed_journal(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.record(RecoveryPlan::CleanupCompletedJournal);
        Ok(())
    }
}

fn observed(phase: UpgradePhase) -> ObservedUpgradeState {
    ObservedUpgradeState {
        journal: JournalState::Valid(phase),
        config_generation: ConfigGeneration::Generation1,
        source: LegacySourceState::Generation1,
        backup: BackupState::Missing,
        candidate: V2CandidateState::Missing,
        relocation_intent: false,
        compatibility: CompatibilityState::NotApplicable,
        orphan_artifacts: false,
        sidecars: LegacySidecarState::Absent,
    }
}

fn valid_journal(phase: UpgradePhase) -> UpgradeJournal {
    let payload = UpgradeJournalPayload {
        journal_version: JOURNAL_VERSION,
        attempt_id: UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000001").unwrap(),
        phase,
        source_generation: 1,
        released_schema_profile: ReleasedSchemaProfile::parse("v0.3.1").unwrap(),
        source_candidate_identity: Sha256Digest::parse(HASH_A).unwrap(),
        verified_backup_sha256: (phase != UpgradePhase::Prepared)
            .then(|| Sha256Digest::parse(HASH_B).unwrap()),
        paths: JournalArtifactPaths::for_attempt(
            &UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000001").unwrap(),
        ),
        created_at: UtcTimestamp::parse("2026-07-20T10:00:00Z").unwrap(),
        updated_at: UtcTimestamp::parse("2026-07-20T10:01:00Z").unwrap(),
    };
    UpgradeJournal::seal(payload).expect("seal journal")
}
