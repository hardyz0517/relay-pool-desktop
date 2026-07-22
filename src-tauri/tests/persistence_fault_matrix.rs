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

mod runtime_composition {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/runtime_composition.rs"
    ));
}

use persistence::{
    upgrade_fault::{
        AtomicStep, NoUpgradeFaults, TombstoneStep, UpgradeFailpoint, UpgradeFaultInjector,
        UpgradeInjectedFailure,
    },
    upgrade_journal::{
        JournalArtifactPaths, ReleasedSchemaProfile, Sha256Digest, UpgradeAttemptId,
        UpgradeJournal, UpgradeJournalPayload, UpgradePhase, UtcTimestamp, JOURNAL_VERSION,
    },
    upgrade_recovery_executor::{
        journal_artifact_paths_are_safe, observe_backup, observe_journal, observe_legacy_source,
        publish_v2_candidate_with_faults, read_legacy_tombstone,
        remove_file_and_sync_parent_with_faults, replace_legacy_with_tombstone_with_faults,
        resolve_allowlisted_artifact, sha256_file, write_journal_atomically,
        write_journal_atomically_with_faults, RecoveryExecution, RecoveryExecutor,
        UpgradeExecutionError, UpgradeIoOperation, UpgradeRecoveryIo, UPGRADE_JOURNAL_FILE,
    },
    upgrade_recovery_plan::{
        BackupState, CompatibilityState, ConfigGeneration, JournalState, LegacySidecarState,
        LegacySourceState, ObservedUpgradeState, RecoveryHaltReason, RecoveryPlan, RecoveryPlanner,
        V2CandidateState,
    },
};
use runtime_composition::{
    drain_finalization, register_ready_services_in, ReadyServiceBundle, ReadyServiceRegistry,
    RuntimeCompositionError,
};
use sha2::{Digest, Sha256};
use std::{
    any::{Any, TypeId},
    cell::Cell,
    collections::HashMap,
    fs,
    path::Path,
    sync::Mutex,
};

const HASH_A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const HASH_B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const SECRET_CANARY: &str = "sk-qualification-must-not-leak";

#[test]
fn every_journal_phase_atomic_edge_is_restartable() {
    for phase in UpgradePhase::ALL {
        for edge in AtomicStep::ALL {
            let root = tempfile::tempdir().expect("journal fault fixture");
            let path = root.path().join("persistence-upgrade-journal.json");
            let journal = valid_journal(phase);
            let fault = OneShotUpgradeFault::new(UpgradeFailpoint::Journal { phase, edge });

            let error = write_journal_atomically_with_faults(&path, &journal, &fault)
                .expect_err("inject journal atomic edge");
            assert_injected_at(error, UpgradeFailpoint::Journal { phase, edge });

            let expected = match edge {
                AtomicStep::BeforeWrite
                | AtomicStep::BeforeFileSync
                | AtomicStep::BeforeReplace => JournalState::Missing,
                AtomicStep::AfterReplaceBeforeParentSync | AtomicStep::AfterDurableSync => {
                    JournalState::Valid(phase)
                }
            };
            assert_eq!(observe_journal(&path).state, expected);

            write_journal_atomically(&path, &journal).expect("retry journal publication");
            assert_eq!(observe_journal(&path).state, JournalState::Valid(phase));
        }
    }
}

#[test]
fn every_activation_atomic_edge_preserves_a_publishable_candidate() {
    for edge in AtomicStep::ALL {
        let root = tempfile::tempdir().expect("activation fault fixture");
        let temporary = root.path().join("candidate.tmp");
        let final_path = root.path().join("relay-pool-v2.sqlite3");
        let candidate = b"validated-v2-candidate";
        fs::write(&temporary, candidate).expect("write candidate");
        let fault = OneShotUpgradeFault::new(UpgradeFailpoint::Activation(edge));

        let error = publish_v2_candidate_with_faults(&temporary, &final_path, &fault)
            .expect_err("inject activation atomic edge");
        assert_injected_at(error, UpgradeFailpoint::Activation(edge));

        match edge {
            AtomicStep::BeforeWrite | AtomicStep::BeforeFileSync | AtomicStep::BeforeReplace => {
                assert_eq!(fs::read(&temporary).expect("candidate remains"), candidate);
                assert!(!final_path.exists());
                publish_v2_candidate_with_faults(&temporary, &final_path, &NoUpgradeFaults)
                    .expect("retry candidate publication");
            }
            AtomicStep::AfterReplaceBeforeParentSync | AtomicStep::AfterDurableSync => {
                assert!(!temporary.exists());
                assert_eq!(
                    fs::read(&final_path).expect("candidate published"),
                    candidate
                );
            }
        }
        assert_eq!(fs::read(&final_path).expect("durable candidate"), candidate);
    }
}

#[test]
fn every_tombstone_atomic_edge_keeps_source_or_verified_tombstone() {
    for edge in TombstoneStep::ALL {
        let root = tempfile::tempdir().expect("tombstone fault fixture");
        let source = root.path().join("relay-pool.sqlite3");
        let original = b"SQLite format 3\0protected-source";
        fs::write(&source, original).expect("write source");
        let attempt =
            UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000001").expect("attempt id");
        let fault = OneShotUpgradeFault::new(UpgradeFailpoint::Tombstone(edge));

        let error = replace_legacy_with_tombstone_with_faults(&source, &attempt, &fault)
            .expect_err("inject tombstone atomic edge");
        assert_injected_at(error, UpgradeFailpoint::Tombstone(edge));

        match edge {
            TombstoneStep::BeforeWrite
            | TombstoneStep::BeforeFileSync
            | TombstoneStep::BeforeReplace => {
                assert_eq!(fs::read(&source).expect("source remains"), original);
                replace_legacy_with_tombstone_with_faults(&source, &attempt, &NoUpgradeFaults)
                    .expect("retry tombstone replacement");
            }
            TombstoneStep::AfterReplaceBeforeParentSync | TombstoneStep::AfterDurableSync => {}
        }
        assert_eq!(
            read_legacy_tombstone(&source).expect("read tombstone"),
            Some(attempt)
        );
    }
}

#[test]
fn every_cleanup_atomic_edge_leaves_a_deterministic_terminal_state() {
    for edge in AtomicStep::ALL {
        let root = tempfile::tempdir().expect("cleanup fault fixture");
        let journal = root.path().join("persistence-upgrade-journal.json");
        fs::write(&journal, b"completed-journal").expect("write journal");
        let fault = OneShotUpgradeFault::new(UpgradeFailpoint::JournalCleanup(edge));

        let error = remove_file_and_sync_parent_with_faults(&journal, &fault)
            .expect_err("inject cleanup atomic edge");
        assert_injected_at(error, UpgradeFailpoint::JournalCleanup(edge));

        match edge {
            AtomicStep::BeforeWrite | AtomicStep::BeforeFileSync | AtomicStep::BeforeReplace => {
                assert!(journal.is_file());
                remove_file_and_sync_parent_with_faults(&journal, &NoUpgradeFaults)
                    .expect("retry journal cleanup");
            }
            AtomicStep::AfterReplaceBeforeParentSync | AtomicStep::AfterDurableSync => {
                assert!(!journal.exists());
            }
        }
        assert!(!journal.exists());
    }
}

#[test]
fn filesystem_observation_contracts_fail_closed_and_keep_artifacts_allowlisted() {
    let root = tempfile::tempdir().expect("observation fixture");
    let source = root.path().join("source.sqlite3");
    let backup = root.path().join("backup.sqlite3");
    let journal = valid_journal(UpgradePhase::Prepared);

    assert_eq!(observe_legacy_source(&source), LegacySourceState::Missing);
    fs::write(&source, b"not sqlite").expect("invalid source");
    assert_eq!(observe_legacy_source(&source), LegacySourceState::Unknown);
    fs::write(&source, b"SQLite format 3\0legacy").expect("sqlite source");
    assert_eq!(
        observe_legacy_source(&source),
        LegacySourceState::Generation1
    );

    fs::write(&backup, b"backup bytes").expect("backup");
    let digest = sha256_file(&backup).expect("backup digest");
    assert_eq!(digest.as_str().len(), 64);
    assert_eq!(observe_backup(&backup, &digest), BackupState::Verified);
    assert_eq!(
        observe_backup(&backup, &Sha256Digest::parse(HASH_A).unwrap()),
        BackupState::Invalid
    );
    assert!(journal_artifact_paths_are_safe(root.path(), &journal));
    assert!(resolve_allowlisted_artifact(root.path(), "backups/attempt/db.sqlite3").is_ok());
    assert!(resolve_allowlisted_artifact(root.path(), "../escape.sqlite3").is_err());
    assert_eq!(UPGRADE_JOURNAL_FILE, "persistence-upgrade-journal.json");
}

#[test]
fn service_registration_fault_preserves_ready_generation_and_protected_artifacts() {
    let root = tempfile::tempdir().expect("service registration fault fixture");
    let source = root.path().join("source.sqlite3");
    let backup = root.path().join("backup.sqlite3");
    fs::write(&source, format!("protected source bytes {SECRET_CANARY}")).expect("source fixture");
    fs::write(&backup, b"protected backup bytes").expect("backup fixture");
    let before = protected_evidence(&source, &backup);
    let authoritative = ready_generation_two();
    let recovery = RecoveryPlanner::plan(authoritative.clone());
    let fault = OneShotUpgradeFault::new(UpgradeFailpoint::ServiceRegistration);
    let mut state = EquivalentTauriStateManager::default();

    let error = register_ready_services_in(
        &fault,
        &mut state,
        ReadyServiceBundle::new(
            ManagedSlotOne(1),
            ManagedSlotTwo(1),
            ManagedSlotThree(1),
            ManagedSlotFour(1),
            ManagedSlotFive(1),
        ),
    )
    .expect_err("service registration injection must fail closed");

    assert_eq!(
        error,
        RuntimeCompositionError::Injected(UpgradeInjectedFailure::new(
            UpgradeFailpoint::ServiceRegistration,
        ))
    );
    assert!(state.try_state::<ManagedSlotOne>().is_none());
    assert!(state.try_state::<ManagedSlotTwo>().is_none());
    assert!(state.try_state::<ManagedSlotThree>().is_none());
    assert!(state.try_state::<ManagedSlotFour>().is_none());
    assert!(state.try_state::<ManagedSlotFive>().is_none());
    assert_eq!(
        authoritative.config_generation,
        ConfigGeneration::Generation2
    );
    assert_eq!(
        recovery,
        RecoveryPlan::Halt(RecoveryHaltReason::NoUpgradeInProgress)
    );
    assert_eq!(protected_evidence(&source, &backup), before);
    assert_composition_redacted(&error, root.path());
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManagedSlotOne(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManagedSlotTwo(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManagedSlotThree(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManagedSlotFour(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ManagedSlotFive(u8);

#[derive(Default)]
struct EquivalentTauriStateManager {
    states: Mutex<HashMap<TypeId, Box<dyn Any + Send + Sync>>>,
}

impl EquivalentTauriStateManager {
    fn manage<T: Send + Sync + 'static>(&self, state: T) -> bool {
        let mut states = self.states.lock().expect("state manager poisoned");
        let type_id = TypeId::of::<T>();
        if states.contains_key(&type_id) {
            return false;
        }
        states.insert(type_id, Box::new(state));
        true
    }

    fn try_state<T: Copy + Send + Sync + 'static>(&self) -> Option<T> {
        self.states
            .lock()
            .expect("state manager poisoned")
            .get(&TypeId::of::<T>())
            .and_then(|state| state.downcast_ref::<T>())
            .copied()
    }
}

impl ReadyServiceRegistry for EquivalentTauriStateManager {
    fn contains<T: Send + Sync + 'static>(&self) -> bool {
        self.states
            .lock()
            .expect("state manager poisoned")
            .contains_key(&TypeId::of::<T>())
    }

    fn manage<T: Send + Sync + 'static>(&mut self, state: T) -> bool {
        EquivalentTauriStateManager::manage(self, state)
    }
}

#[test]
fn tauri_equivalent_state_manager_never_observes_partial_ready_services() {
    for occupied_slot in 0..5 {
        let mut state = EquivalentTauriStateManager::default();

        match occupied_slot {
            0 => assert!(state.manage(ManagedSlotOne(99))),
            1 => assert!(state.manage(ManagedSlotTwo(99))),
            2 => assert!(state.manage(ManagedSlotThree(99))),
            3 => assert!(state.manage(ManagedSlotFour(99))),
            4 => assert!(state.manage(ManagedSlotFive(99))),
            _ => unreachable!(),
        }

        let error = register_ready_services_in(
            &NoUpgradeFaults,
            &mut state,
            ReadyServiceBundle::new(
                ManagedSlotOne(1),
                ManagedSlotTwo(1),
                ManagedSlotThree(1),
                ManagedSlotFour(1),
                ManagedSlotFive(1),
            ),
        )
        .expect_err("an occupied state slot must fail closed");

        assert_eq!(error, RuntimeCompositionError::StateSlotOccupied);
        let observed = [
            state.try_state::<ManagedSlotOne>().map(|state| state.0),
            state.try_state::<ManagedSlotTwo>().map(|state| state.0),
            state.try_state::<ManagedSlotThree>().map(|state| state.0),
            state.try_state::<ManagedSlotFour>().map(|state| state.0),
            state.try_state::<ManagedSlotFive>().map(|state| state.0),
        ];
        let expected = std::array::from_fn(|index| (index == occupied_slot).then_some(99));
        assert_eq!(
            observed, expected,
            "slot {occupied_slot} collision must not publish any other ready service"
        );
    }
}

#[test]
fn tauri_equivalent_state_manager_publishes_the_complete_ready_bundle() {
    let mut state = EquivalentTauriStateManager::default();

    register_ready_services_in(
        &NoUpgradeFaults,
        &mut state,
        ReadyServiceBundle::new(
            ManagedSlotOne(1),
            ManagedSlotTwo(2),
            ManagedSlotThree(3),
            ManagedSlotFour(4),
            ManagedSlotFive(5),
        ),
    )
    .expect("a vacant registry must publish the complete ready bundle");

    assert_eq!(state.try_state::<ManagedSlotOne>(), Some(ManagedSlotOne(1)));
    assert_eq!(state.try_state::<ManagedSlotTwo>(), Some(ManagedSlotTwo(2)));
    assert_eq!(
        state.try_state::<ManagedSlotThree>(),
        Some(ManagedSlotThree(3))
    );
    assert_eq!(
        state.try_state::<ManagedSlotFour>(),
        Some(ManagedSlotFour(4))
    );
    assert_eq!(
        state.try_state::<ManagedSlotFive>(),
        Some(ManagedSlotFive(5))
    );
}

#[tokio::test]
async fn finalization_drain_fault_preserves_ready_generation_and_protected_artifacts() {
    let root = tempfile::tempdir().expect("finalization drain fault fixture");
    let source = root.path().join("source.sqlite3");
    let backup = root.path().join("backup.sqlite3");
    fs::write(&source, format!("protected source bytes {SECRET_CANARY}")).expect("source fixture");
    fs::write(&backup, b"protected backup bytes").expect("backup fixture");
    let before = protected_evidence(&source, &backup);
    let authoritative = ready_generation_two();
    let recovery = RecoveryPlanner::plan(authoritative.clone());
    let drains = Cell::new(0);
    let fault = OneShotUpgradeFault::new(UpgradeFailpoint::FinalizationDrain);

    let error = drain_finalization(&fault, async {
        drains.set(drains.get() + 1);
        Ok(())
    })
    .await
    .expect_err("finalization drain injection must fail closed");

    assert_eq!(
        error,
        RuntimeCompositionError::Injected(UpgradeInjectedFailure::new(
            UpgradeFailpoint::FinalizationDrain,
        ))
    );
    assert_eq!(drains.get(), 0, "injected drain must not be acknowledged");
    assert_eq!(
        authoritative.config_generation,
        ConfigGeneration::Generation2
    );
    assert_eq!(
        recovery,
        RecoveryPlan::Halt(RecoveryHaltReason::NoUpgradeInProgress)
    );
    assert_eq!(protected_evidence(&source, &backup), before);
    assert_composition_redacted(&error, root.path());
}

fn assert_composition_redacted(error: &RuntimeCompositionError, root: &Path) {
    let diagnostic = error.to_string();
    assert!(!diagnostic.contains(SECRET_CANARY), "secret leaked");
    assert!(
        !diagnostic.contains(&root.display().to_string()),
        "absolute path leaked"
    );
    assert!(
        diagnostic.starts_with("persistence_upgrade_fault_injected at runtime."),
        "diagnostic must use a bounded typed category: {diagnostic}"
    );
}

fn assert_injected_at(error: UpgradeExecutionError, expected: UpgradeFailpoint) {
    assert_eq!(
        error,
        UpgradeExecutionError::Injected(UpgradeInjectedFailure::new(expected))
    );
}

struct OneShotUpgradeFault {
    target: UpgradeFailpoint,
    fired: std::sync::Mutex<bool>,
}

impl OneShotUpgradeFault {
    fn new(target: UpgradeFailpoint) -> Self {
        Self {
            target,
            fired: std::sync::Mutex::new(false),
        }
    }
}

impl UpgradeFaultInjector for OneShotUpgradeFault {
    fn check(&self, failpoint: UpgradeFailpoint) -> Result<(), UpgradeInjectedFailure> {
        let mut fired = self.fired.lock().expect("fault mutex");
        if !*fired && failpoint == self.target {
            *fired = true;
            return Err(UpgradeInjectedFailure::new(failpoint));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FaultPoint {
    Observe,
    LoadJournal,
    ArtifactPathValidation,
    SourceDigest,
    BackupDigest,
    TombstoneRead,
    ExecuteAction,
}

#[derive(Clone, Copy, Debug)]
struct FaultCase {
    name: &'static str,
    state: fn() -> ObservedUpgradeState,
    expected_plan: RecoveryPlan,
    fault: FaultPoint,
}

#[test]
fn recovery_fault_matrix_preserves_authority_artifacts_and_redaction() {
    let cases = [
        FaultCase {
            name: "observer-read",
            state: prepared,
            expected_plan: RecoveryPlan::RestartFromSource,
            fault: FaultPoint::Observe,
        },
        FaultCase {
            name: "journal-read",
            state: prepared,
            expected_plan: RecoveryPlan::RestartFromSource,
            fault: FaultPoint::LoadJournal,
        },
        FaultCase {
            name: "artifact-path-validation",
            state: prepared,
            expected_plan: RecoveryPlan::RestartFromSource,
            fault: FaultPoint::ArtifactPathValidation,
        },
        FaultCase {
            name: "source-digest",
            state: prepared,
            expected_plan: RecoveryPlan::RestartFromSource,
            fault: FaultPoint::SourceDigest,
        },
        FaultCase {
            name: "backup-digest",
            state: backup_verified,
            expected_plan: RecoveryPlan::RebuildV2FromVerifiedBackup,
            fault: FaultPoint::BackupDigest,
        },
        FaultCase {
            name: "tombstone-read",
            state: legacy_deactivated,
            expected_plan: RecoveryPlan::CommitGeneration2,
            fault: FaultPoint::TombstoneRead,
        },
        FaultCase {
            name: "restart-from-source",
            state: prepared,
            expected_plan: RecoveryPlan::RestartFromSource,
            fault: FaultPoint::ExecuteAction,
        },
        FaultCase {
            name: "rebuild-from-backup",
            state: backup_verified,
            expected_plan: RecoveryPlan::RebuildV2FromVerifiedBackup,
            fault: FaultPoint::ExecuteAction,
        },
        FaultCase {
            name: "activate-v2",
            state: v2_validated,
            expected_plan: RecoveryPlan::ActivateValidatedV2,
            fault: FaultPoint::ExecuteAction,
        },
        FaultCase {
            name: "record-legacy-deactivated",
            state: v2_activated,
            expected_plan: RecoveryPlan::RecordLegacyDeactivated,
            fault: FaultPoint::ExecuteAction,
        },
        FaultCase {
            name: "commit-generation-2",
            state: legacy_deactivated,
            expected_plan: RecoveryPlan::CommitGeneration2,
            fault: FaultPoint::ExecuteAction,
        },
        FaultCase {
            name: "reopen-generation-2",
            state: generation_committed,
            expected_plan: RecoveryPlan::ReopenGeneration2,
            fault: FaultPoint::ExecuteAction,
        },
        FaultCase {
            name: "cleanup-completed-journal",
            state: v2_reopened,
            expected_plan: RecoveryPlan::CleanupCompletedJournal,
            fault: FaultPoint::ExecuteAction,
        },
    ];

    for case in cases {
        let root = tempfile::tempdir().expect("fault fixture root");
        let source = root.path().join("source.sqlite3");
        let backup = root.path().join("backup.sqlite3");
        fs::write(&source, format!("protected source bytes {SECRET_CANARY}"))
            .expect("source fixture");
        fs::write(&backup, b"protected backup bytes").expect("backup fixture");
        let before = protected_evidence(&source, &backup);

        let state = (case.state)();
        let journal = valid_journal(match state.journal {
            JournalState::Valid(phase) => phase,
            _ => panic!("fault cases require a valid journal"),
        });
        let execution = RecoveryExecution::prepare(state.clone(), Some(&journal))
            .expect("prepare deterministic recovery");
        assert_eq!(execution.plan(), case.expected_plan, "{}", case.name);
        let io = InjectedFailureIo::new(state.clone(), journal, case.fault);

        let error = RecoveryExecutor::execute(&io, &execution)
            .expect_err("injected boundary must fail closed");
        let expected_error = match case.fault {
            FaultPoint::LoadJournal => UpgradeExecutionError::JournalUnavailable,
            FaultPoint::ExecuteAction => UpgradeExecutionError::Io(UpgradeIoOperation::Write),
            _ => UpgradeExecutionError::Io(UpgradeIoOperation::Verify),
        };

        assert_eq!(error, expected_error, "{}", case.name);
        assert_eq!(
            io.observed_generation(),
            state.config_generation,
            "{}",
            case.name
        );
        assert_eq!(
            protected_evidence(&source, &backup),
            before,
            "{}",
            case.name
        );
        assert_eq!(io.successful_actions(), 0, "{}", case.name);
        assert_eq!(
            io.action_attempts(),
            usize::from(case.fault == FaultPoint::ExecuteAction),
            "{}",
            case.name
        );
        assert_redacted(case.name, &error.to_string(), root.path());
    }
}

#[test]
fn precondition_mismatch_matrix_never_reaches_a_destructive_action() {
    let cases = [
        ("source-changed", FaultPoint::SourceDigest, prepared()),
        (
            "backup-changed",
            FaultPoint::BackupDigest,
            backup_verified(),
        ),
        (
            "tombstone-attempt-changed",
            FaultPoint::TombstoneRead,
            legacy_deactivated(),
        ),
        (
            "unsafe-artifact-path",
            FaultPoint::ArtifactPathValidation,
            v2_validated(),
        ),
    ];

    for (name, mismatch, state) in cases {
        let phase = match state.journal {
            JournalState::Valid(phase) => phase,
            _ => unreachable!(),
        };
        let journal = valid_journal(phase);
        let execution = RecoveryExecution::prepare(state.clone(), Some(&journal)).unwrap();
        let io = InjectedFailureIo::new(state, journal, mismatch).with_mismatch();

        assert_eq!(
            RecoveryExecutor::execute(&io, &execution),
            Err(UpgradeExecutionError::RecoveryPreconditionChanged),
            "{name}"
        );
        assert_eq!(io.action_attempts(), 0, "{name}");
        assert_eq!(io.successful_actions(), 0, "{name}");
    }
}

fn assert_redacted(case: &str, diagnostic: &str, root: &Path) {
    assert!(!diagnostic.contains(SECRET_CANARY), "{case}: secret leaked");
    assert!(
        !diagnostic.contains(&root.display().to_string()),
        "{case}: absolute path leaked"
    );
    assert!(
        diagnostic.starts_with("upgrade ") || diagnostic == "upgrade journal is unavailable",
        "{case}: diagnostic must use a bounded typed category: {diagnostic}"
    );
}

struct InjectedFailureIo {
    observed: ObservedUpgradeState,
    journal: UpgradeJournal,
    fault: FaultPoint,
    mismatch: bool,
    action_attempts: Cell<usize>,
    successful_actions: Cell<usize>,
}

impl InjectedFailureIo {
    fn new(observed: ObservedUpgradeState, journal: UpgradeJournal, fault: FaultPoint) -> Self {
        Self {
            observed,
            journal,
            fault,
            mismatch: false,
            action_attempts: Cell::new(0),
            successful_actions: Cell::new(0),
        }
    }

    fn with_mismatch(mut self) -> Self {
        self.mismatch = true;
        self
    }

    fn injected<T>(&self, point: FaultPoint, value: T) -> Result<T, UpgradeExecutionError> {
        if self.fault == point && !self.mismatch {
            Err(UpgradeExecutionError::Io(UpgradeIoOperation::Verify))
        } else {
            Ok(value)
        }
    }

    fn action(&self) -> Result<(), UpgradeExecutionError> {
        self.action_attempts.set(self.action_attempts.get() + 1);
        if self.fault == FaultPoint::ExecuteAction {
            Err(UpgradeExecutionError::Io(UpgradeIoOperation::Write))
        } else {
            self.successful_actions
                .set(self.successful_actions.get() + 1);
            Ok(())
        }
    }

    fn successful_actions(&self) -> usize {
        self.successful_actions.get()
    }

    fn action_attempts(&self) -> usize {
        self.action_attempts.get()
    }

    fn observed_generation(&self) -> ConfigGeneration {
        self.observed.config_generation
    }
}

impl UpgradeRecoveryIo for InjectedFailureIo {
    fn observe(&self) -> Result<ObservedUpgradeState, UpgradeExecutionError> {
        self.injected(FaultPoint::Observe, self.observed.clone())
    }

    fn load_journal(&self) -> Result<UpgradeJournal, UpgradeExecutionError> {
        if self.fault == FaultPoint::LoadJournal && !self.mismatch {
            return Err(UpgradeExecutionError::JournalUnavailable);
        }
        Ok(self.journal.clone())
    }

    fn source_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError> {
        let hash = if self.fault == FaultPoint::SourceDigest && self.mismatch {
            HASH_B
        } else {
            HASH_A
        };
        self.injected(FaultPoint::SourceDigest, Sha256Digest::parse(hash).unwrap())
    }

    fn backup_sha256(&self) -> Result<Sha256Digest, UpgradeExecutionError> {
        let hash = if self.fault == FaultPoint::BackupDigest && self.mismatch {
            HASH_A
        } else {
            HASH_B
        };
        self.injected(FaultPoint::BackupDigest, Sha256Digest::parse(hash).unwrap())
    }

    fn tombstone_attempt_id(&self) -> Result<UpgradeAttemptId, UpgradeExecutionError> {
        let attempt = if self.fault == FaultPoint::TombstoneRead && self.mismatch {
            UpgradeAttemptId::parse("019f7d50-9d44-7000-8000-000000000002").unwrap()
        } else {
            self.journal.payload().attempt_id.clone()
        };
        self.injected(FaultPoint::TombstoneRead, attempt)
    }

    fn artifact_paths_are_safe(
        &self,
        _journal: &UpgradeJournal,
    ) -> Result<bool, UpgradeExecutionError> {
        if self.fault == FaultPoint::ArtifactPathValidation && self.mismatch {
            return Ok(false);
        }
        self.injected(FaultPoint::ArtifactPathValidation, true)
    }

    fn restart_from_source(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn rebuild_v2_from_verified_backup(
        &self,
        _: &UpgradeJournal,
    ) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn activate_validated_v2(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn record_legacy_deactivated(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn restore_generation_1(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn commit_generation_2(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn reopen_generation_2(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }

    fn cleanup_completed_journal(&self, _: &UpgradeJournal) -> Result<(), UpgradeExecutionError> {
        self.action()
    }
}

fn prepared() -> ObservedUpgradeState {
    state(
        UpgradePhase::Prepared,
        ConfigGeneration::Generation1,
        LegacySourceState::Generation1,
        BackupState::Missing,
        V2CandidateState::Missing,
        CompatibilityState::NotApplicable,
    )
}

fn backup_verified() -> ObservedUpgradeState {
    state(
        UpgradePhase::BackupVerified,
        ConfigGeneration::Generation1,
        LegacySourceState::Generation1,
        BackupState::Verified,
        V2CandidateState::Missing,
        CompatibilityState::NotApplicable,
    )
}

fn v2_validated() -> ObservedUpgradeState {
    state(
        UpgradePhase::V2Validated,
        ConfigGeneration::Generation1,
        LegacySourceState::Generation1,
        BackupState::Verified,
        V2CandidateState::ValidatedTemporary,
        CompatibilityState::Writable,
    )
}

fn v2_activated() -> ObservedUpgradeState {
    state(
        UpgradePhase::V2Validated,
        ConfigGeneration::Generation1,
        LegacySourceState::ValidTombstone,
        BackupState::Verified,
        V2CandidateState::ValidatedFinal,
        CompatibilityState::Writable,
    )
}

fn legacy_deactivated() -> ObservedUpgradeState {
    state(
        UpgradePhase::LegacyDeactivated,
        ConfigGeneration::Generation2,
        LegacySourceState::ValidTombstone,
        BackupState::Verified,
        V2CandidateState::ValidatedFinal,
        CompatibilityState::Writable,
    )
}

fn generation_committed() -> ObservedUpgradeState {
    state(
        UpgradePhase::GenerationCommitted,
        ConfigGeneration::Generation2,
        LegacySourceState::ValidTombstone,
        BackupState::Verified,
        V2CandidateState::ValidatedFinal,
        CompatibilityState::Writable,
    )
}

fn v2_reopened() -> ObservedUpgradeState {
    state(
        UpgradePhase::V2Reopened,
        ConfigGeneration::Generation2,
        LegacySourceState::ValidTombstone,
        BackupState::Verified,
        V2CandidateState::ValidatedFinal,
        CompatibilityState::Writable,
    )
}

fn ready_generation_two() -> ObservedUpgradeState {
    ObservedUpgradeState {
        journal: JournalState::Missing,
        config_generation: ConfigGeneration::Generation2,
        source: LegacySourceState::ValidTombstone,
        backup: BackupState::Verified,
        candidate: V2CandidateState::ValidatedFinal,
        relocation_intent: false,
        compatibility: CompatibilityState::Writable,
        orphan_artifacts: false,
        sidecars: LegacySidecarState::Absent,
    }
}

fn state(
    phase: UpgradePhase,
    config_generation: ConfigGeneration,
    source: LegacySourceState,
    backup: BackupState,
    candidate: V2CandidateState,
    compatibility: CompatibilityState,
) -> ObservedUpgradeState {
    ObservedUpgradeState {
        journal: JournalState::Valid(phase),
        config_generation,
        source,
        backup,
        candidate,
        relocation_intent: false,
        compatibility,
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

fn protected_evidence(source: &Path, backup: &Path) -> [(u64, String); 2] {
    [source, backup].map(|path| {
        let bytes = fs::read(path).expect("protected fixture bytes");
        (
            bytes.len() as u64,
            Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        )
    })
}
