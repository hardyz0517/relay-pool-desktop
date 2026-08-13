use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn portable_migration_malicious_input_qualification_runs_internal_flow_evidence() {
    let manifest_dir = manifest_dir();
    assert_source_malicious_invariants(&manifest_dir);

    run_lib_test_filter(
        &manifest_dir,
        "malicious_portable_package_wrong_password_truncation_and_toctou_fail_closed",
    );
    run_lib_test_filter(
        &manifest_dir,
        "malicious_portable_package_rejects_non_sqlite_and_schema_object_attacks",
    );
    run_lib_test_filter(
        &manifest_dir,
        "reader_rejects_unknown_schema_objects_columns_and_spoofed_versions",
    );
    run_lib_test_filter(
        &manifest_dir,
        "manifest_rejects_duplicate_unknown_invalid_and_inexact_fields",
    );
    run_lib_test_filter(
        &manifest_dir,
        "malformed_framing_fixtures_match_expected_failures",
    );
    run_lib_test_filter(
        &manifest_dir,
        "numeric_limits_accept_limit_minus_one_and_limit_but_reject_limit_plus_one",
    );
    run_lib_test_filter(
        &manifest_dir,
        "expired_preparation_lease_removes_only_owned_staging_file",
    );
    run_lib_test_filter(
        &manifest_dir,
        "token_ttl_capacity_type_and_process_nonce_are_fail_closed",
    );
}

fn assert_source_malicious_invariants(manifest_dir: &Path) {
    let fixture_manifest = read_source(
        manifest_dir,
        "tests/fixtures/portable-migration/manifest.json",
    );
    for required_case in [
        "wrong-password-envelope",
        "truncated-frame",
        "unknown-required-feature",
        "too-new-schema",
        "malformed-sqlite",
        "trigger-view-schema-object",
        "foreign-key-broken",
        "resource-overflow",
    ] {
        assert!(
            fixture_manifest.contains(required_case),
            "portable migration fixture manifest is missing malicious/fault case {required_case}"
        );
    }

    let schema_reader = read_source(
        manifest_dir,
        "src/services/portable_migration/schema_reader.rs",
    );
    assert!(
        schema_reader.contains("\"trigger\" =>")
            && schema_reader.contains("\"view\" =>")
            && schema_reader.contains("TRUSTED_TRIGGERS_V1")
            && schema_reader.contains("UnsupportedSchemaObject")
            && !schema_reader.contains("ATTACH"),
        "portable reader must reject untrusted schema objects and avoid attached databases"
    );

    let validate = read_source(manifest_dir, "src/services/portable_migration/validate.rs");
    assert!(
        validate.contains("PRAGMA query_only = ON")
            && validate.contains("PRAGMA trusted_schema = OFF")
            && validate.contains("PRAGMA foreign_key_check"),
        "portable SQLite validation must use a hardened read-only connection"
    );

    let inspection_registry = read_source(
        manifest_dir,
        "src/services/portable_migration/inspection_registry.rs",
    );
    assert!(
        inspection_registry.contains("is_import_staging_sqlite")
            && !inspection_registry.contains("remove_dir_all"),
        "inspection cleanup must only delete the verified owned staging SQLite file"
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
