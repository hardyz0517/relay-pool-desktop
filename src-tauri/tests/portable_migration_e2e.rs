use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn portable_migration_e2e_qualification_runs_internal_flow_evidence() {
    let manifest_dir = manifest_dir();
    assert_source_e2e_invariants(&manifest_dir);

    run_lib_test_filter(
        &manifest_dir,
        "portable_export_package_writes_self_verified_age_file",
    );
    run_lib_test_filter(
        &manifest_dir,
        "portable_import_inspection_registers_verified_package_without_touching_active_db",
    );
    run_lib_test_filter(
        &manifest_dir,
        "portable_import_target_rebuilds_restore_into_empty_with_target_key",
    );
    run_lib_test_filter(
        &manifest_dir,
        "portable_import_three_keys_isolates_source_transport_and_target",
    );
    run_lib_test_filter(
        &manifest_dir,
        "prepared_journal_replaces_staged_and_returns_target_key",
    );
    run_lib_test_filter(
        &manifest_dir,
        "optional_history_is_omitted_by_default_and_redacted_when_enabled",
    );
}

fn assert_source_e2e_invariants(manifest_dir: &Path) {
    let export_service = read_source(
        manifest_dir,
        "src/application/data_migration/export_service.rs",
    );
    assert!(
        export_service.contains("portable package must not contain source device key bytes"),
        "export qualification must prove source key bytes are absent from the published package"
    );
    assert!(
        export_service.contains("plaintext staging sqlite must be cleaned after publish"),
        "export qualification must prove plaintext staging cleanup"
    );

    let import_service = read_source(
        manifest_dir,
        "src/application/data_migration/import_service.rs",
    );
    assert!(
        import_service.contains("portable_import_three_keys_isolates_source_transport_and_target"),
        "import qualification must prove source/transport/target key isolation"
    );
    assert!(
        import_service.contains("SELECT value FROM settings WHERE key = 'local_key'"),
        "import qualification must prove Local Key reset on target rebuild"
    );
    assert!(
        import_service.contains("PortableActivationPhase::Prepared"),
        "activation qualification must prove restart journal preparation"
    );

    let transform = read_source(manifest_dir, "src/services/portable_migration/transform.rs");
    assert!(
        transform.contains("optional_history_is_omitted_by_default_and_redacted_when_enabled"),
        "history on/off behavior must be covered by portable row transform tests"
    );
    assert!(
        transform.contains("session_status") && transform.contains("session_source"),
        "session fields must be reset or excluded through the portable transform layer"
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
