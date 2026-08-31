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
        assert!(
            migration.contains(&format!("CREATE TABLE IF NOT EXISTS {table}")),
            "foundation migration missing {table}"
        );
    }
    assert!(!migration.contains("CAST(updated_at AS INTEGER)"));
    assert!(!migration.contains("unwrap_or(1)"));
    assert!(migration.contains("baseline_snapshot"));
    assert!(migration.contains("provenance"));
}

#[test]
fn routing_v3_circuit_and_retention_migrations_own_their_tables() {
    let circuit = read_source("src/persistence/migrations/0062_routing_key_circuit_v3.sql");
    assert!(
        circuit.contains("CREATE TABLE routing_circuit_clock_v3"),
        "routing v3 circuit migration missing routing_circuit_clock_v3"
    );

    let retention = read_source("src/persistence/migrations/0065_routing_raw_event_retention.sql");
    for table in [
        "routing_raw_event_retention_rollup",
        "routing_raw_event_retention_run",
    ] {
        assert!(
            retention.contains(&format!("CREATE TABLE {table}")),
            "routing v3 retention migration missing {table}"
        );
    }
}

#[test]
fn scoped_verdict_and_terminal_durability_migrations_are_additive() {
    let verdict = read_source("src/persistence/migrations/0035_scoped_routing_health_verdicts.sql");
    assert!(verdict.contains("CREATE TABLE routing_health_verdicts"));

    let outcome =
        read_source("src/persistence/migrations/0037_request_routing_outcome_summaries.sql");
    assert!(outcome.contains("CREATE TABLE IF NOT EXISTS request_routing_outcome_summaries"));

    let outbox = read_source("src/persistence/migrations/0039_request_terminal_outbox.sql");
    assert!(outbox.contains("CREATE TABLE request_terminal_outbox"));
}

#[test]
fn routing_outcome_migration_is_additive_and_redacts_unstable_values() {
    let migration =
        read_source("src/persistence/migrations/0037_request_routing_outcome_summaries.sql");
    assert!(migration.contains("CREATE TABLE IF NOT EXISTS request_routing_outcome_summaries"));
    for column in [
        "classification",
        "confidence",
        "evidence_source",
        "failure_domain_commitment_digest",
    ] {
        assert!(
            migration.contains(column),
            "routing outcome missing {column}"
        );
    }
    for forbidden in [
        "authorization TEXT",
        "upstream_url TEXT",
        "message TEXT",
        "request_body TEXT",
    ] {
        assert!(
            !migration.contains(forbidden),
            "routing outcome must not persist {forbidden}"
        );
    }
}

#[test]
fn cutover_migration_removes_legacy_routing_truths_and_writeback_fields() {
    let migration =
        read_source("src/persistence/migrations/0026_intelligent_routing_cutover_cleanup.sql");
    for statement in [
        "DELETE FROM settings",
        "DROP COLUMN health_writeback_mode",
        "DROP COLUMN health_writeback_decision",
        "DROP COLUMN health_writeback_reason",
        "schema_version = 26",
    ] {
        assert!(
            migration.contains(statement),
            "cutover migration missing {statement}"
        );
    }
    // Asset status and the two pre-v2 health tables remain as compatibility
    // storage for installed databases. They are explicitly excluded from the
    // portable catalog and are not read by the routing planner.
    assert!(migration.contains("Legacy health tables remain as non-routing compatibility storage"));
}

#[test]
fn projection_ingestion_migration_resets_only_derived_quality_state() {
    let migration =
        read_source("src/persistence/migrations/0027_routing_projection_monotonic_ingestion.sql");
    for statement in [
        "DELETE FROM routing_projector_checkpoints",
        "DELETE FROM routing_quality_summaries",
        "DELETE FROM routing_health_axes",
        "schema_version = 27",
    ] {
        assert!(
            migration.contains(statement),
            "projection ingestion migration missing {statement}"
        );
    }
    assert!(!migration.contains("DELETE FROM routing_observations"));
}

#[test]
fn portable_reader_accepts_pre_cutover_health_tables_as_ignored_history() {
    let reader = read_source("src/services/portable_migration/schema_reader.rs");
    for table in ["station_endpoint_health", "station_key_health"] {
        assert!(
            reader.contains(&format!("\"{table}\"")),
            "reader must accept legacy table {table}"
        );
    }
}

#[test]
fn legacy_routing_settings_are_reset_during_portable_import() {
    let catalog = read_source("src/services/portable_migration/catalog.rs");
    for key in [
        "default_routing_strategy",
        "default_routing_group_filter",
        "dispatch_algorithm_profile_json",
        "max_rate_multiplier",
        "allow_depleted_fallback",
    ] {
        assert!(
            catalog.contains(&format!("\"{key}\"")),
            "catalog must classify {key} as reset"
        );
    }
    assert!(catalog.contains("SettingPolicy::Reset"));
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
    assert!(catalog.contains("EXPECTED_USER_TABLE_COUNT_V1: usize = 110"));
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
    run_cargo_test("--test", "schema15_upgrade_fixture");
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
