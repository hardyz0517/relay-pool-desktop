//! Task 2 qualification harness.
//!
//! The migration and portable readers have their own focused tests in the
//! library targets. This target keeps the upgrade contract visible at the
//! integration boundary and executes those real fixtures as child Cargo
//! targets, so a skipped or renamed test cannot silently satisfy Task 2.

use std::{env, fs, path::Path, process::Command};

#[test]
fn foundation_migration_is_additive_and_has_no_timestamp_revision_fallback() {
    let migration =
        read_source("src/persistence/migrations/0024_intelligent_routing_foundation.sql");
    for table in [
        "domain_revisions",
        "routing_policy",
        "routing_policy_history",
        "routing_observations",
        "routing_projector_checkpoints",
        "routing_quality_summaries",
        "routing_health_axes",
    ] {
        assert!(migration.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")));
    }
    assert!(!migration.contains("CAST(updated_at AS INTEGER)"));
    assert!(!migration.contains("unwrap_or(1)"));
    assert!(migration.contains("baseline_snapshot"));
    assert!(migration.contains("provenance"));
}

#[test]
fn portable_catalog_declares_foundation_tables_and_explicit_json_rules() {
    let catalog = read_source("src/services/portable_migration/catalog.rs");
    for table in [
        "domain_revisions",
        "routing_policy",
        "routing_policy_history",
        "routing_observations",
        "routing_projector_checkpoints",
        "routing_quality_summaries",
        "routing_health_axes",
    ] {
        assert!(
            catalog.contains(&format!("\"{table}\"")),
            "catalog missing {table}"
        );
    }
    assert!(catalog.contains("EXPECTED_USER_TABLE_COUNT_V1: usize = 50"));
    assert!(catalog.contains("ROUTING_POLICY_RULES"));
    assert!(catalog.contains("ROUTING_OBSERVATION_RULES"));
    assert!(catalog.contains("ROUTING_QUALITY_RULES"));
}

#[test]
fn routing_policy_write_binds_domain_revision_and_history_to_one_transaction() {
    let store = read_source("src/persistence/stores/routing_policy_store.rs");
    assert!(store.contains("let mut transaction = connection.begin().await?"));
    assert!(store.contains("DomainRevisionStore"));
    assert!(store.contains("routing_policy_history"));
    assert!(store.contains("transaction.commit().await?"));
}

#[test]
fn fresh_current_and_portable_upgrade_fixtures_are_executed() {
    run_cargo_test("--test", "persistence_upgrade");
    run_cargo_test("--test", "portable_migration_e2e");
}

fn run_cargo_test(kind: &str, target: &str) {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let cargo = env::var("CARGO").unwrap_or_else(|_| "cargo".to_string());
    let output = Command::new(cargo)
        .current_dir(manifest_dir)
        .args(["test", "--locked", "--manifest-path"])
        .arg(manifest_dir.join("Cargo.toml"))
        .args([kind, target, "--", "--nocapture"])
        .output()
        .unwrap_or_else(|error| panic!("failed to run {target}: {error}"));
    let combined = format!(
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output.status.success(),
        "focused child target {target} failed\n{combined}"
    );
    assert!(
        !combined.contains("0 passed") && combined.contains("test result: ok"),
        "focused child target {target} did not execute tests\n{combined}"
    );
}

fn read_source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}
