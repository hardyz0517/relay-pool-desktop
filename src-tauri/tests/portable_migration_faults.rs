use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn portable_migration_fault_matrix_qualification_runs_internal_flow_evidence() {
    let manifest_dir = manifest_dir();
    assert_source_fault_invariants(&manifest_dir);

    run_lib_test_filter(
        &manifest_dir,
        "activation_prepare_creates_verified_backup_freezes_runtime_and_writes_prepared_journal",
    );
    run_lib_test_filter(
        &manifest_dir,
        "activation_prepare_backup_failure_happens_before_freeze_and_journal",
    );
    run_lib_test_filter(
        &manifest_dir,
        "activation_prepare_freeze_after_failure_keeps_process_rejecting_writes_until_restart",
    );
    run_lib_test_filter(
        &manifest_dir,
        "recovery_plan_is_closed_over_phase_and_file_state",
    );
    run_lib_test_filter(&manifest_dir, "malformed_journal_is_not_treated_as_absent");
}

fn assert_source_fault_invariants(manifest_dir: &Path) {
    let fault = read_source(manifest_dir, "src/services/portable_migration/fault.rs");
    for step in [
        "TargetValidated",
        "BackupVerified",
        "BeforeFreeze",
        "AfterFreeze",
        "BeforeJournalPublish",
        "AfterJournalPublish",
    ] {
        assert!(
            fault.contains(step),
            "portable activation fault step missing from injectable matrix: {step}"
        );
    }

    let import_service = read_source(
        manifest_dir,
        "src/application/data_migration/import_service.rs",
    );
    for step in [
        "PortableActivationStep::TargetValidated",
        "PortableActivationStep::BackupVerified",
        "PortableActivationStep::BeforeFreeze",
        "PortableActivationStep::AfterFreeze",
        "PortableActivationStep::BeforeJournalPublish",
        "PortableActivationStep::AfterJournalPublish",
    ] {
        assert!(
            import_service.contains(step),
            "activation prepare flow must check fault boundary {step}"
        );
    }
    assert!(
        import_service.contains("DataMaintenanceState::ActivationPending")
            && import_service.contains("RuntimeUnavailable")
            && import_service.contains("read_journal(directory.path())"),
        "fault tests must prove the post-freeze/journal boundary has no ambiguous writable state"
    );

    let recovery = read_source(manifest_dir, "src/services/portable_migration/recovery.rs");
    assert!(
        recovery.contains("RecoveryPlan::Manual")
            && recovery.contains("ManualRecoveryRequired")
            && recovery.contains("validate_journal_paths"),
        "startup recovery must fail closed for ambiguous journal or artifact state"
    );
}

fn run_lib_test_filter(manifest_dir: &Path, filter: &str) {
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .current_dir(manifest_dir)
        .arg("test")
        .arg("--manifest-path")
        .arg(manifest_dir.join("Cargo.toml"))
        .arg("--lib")
        .arg(filter)
        .arg("--")
        .arg("--nocapture")
        .output()
        .unwrap_or_else(|error| panic!("failed to run child cargo test {filter}: {error}"));
    let combined = combined_output(&output);
    assert!(
        output.status.success(),
        "child cargo test failed for filter {filter}\n{combined}"
    );
    assert!(
        combined.contains(filter) && !combined.contains("0 passed"),
        "child cargo test filter {filter} did not execute a matching test\n{combined}"
    );
}

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_source(manifest_dir: &Path, relative_path: &str) -> String {
    fs::read_to_string(manifest_dir.join(relative_path))
        .unwrap_or_else(|error| panic!("failed to read {relative_path}: {error}"))
}

fn combined_output(output: &std::process::Output) -> String {
    format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}
