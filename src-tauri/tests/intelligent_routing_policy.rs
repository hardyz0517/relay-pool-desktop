//! Task 10 boundary checks. Runtime policy compilation must be independent of
//! generic settings, and every legacy strategy must have an explicit mapping.

use std::{fs, path::Path};

#[test]
fn policy_compiler_has_no_legacy_settings_lookup() {
    let source = read_source("src/application/routing_policy.rs");
    assert!(source.contains("pub(crate) fn compile_config"));
    assert!(source.contains("pub(crate) fn compile_json"));
    assert!(source.contains("RoutingPolicyConfigV1"));
    assert!(!source.contains("routing_policy_name"));
    assert!(!source.contains("unwrap_or(1)"));
}

#[test]
fn all_six_legacy_values_are_classified_in_adr_and_code() {
    let source = read_source("src/application/routing_policy.rs");
    let adr = read_source(
        "../docs/superpowers/adrs/routing-operational/0005-routing-policy-legacy-mapping.md",
    );
    for value in [
        "AutomaticBalanced",
        "PriorityFallback",
        "StableFirst",
        "BackupOnly",
        "CheapFirst",
        "CostStableFirst",
    ] {
        assert!(source.contains(value), "code mapping missing {value}");
    }
    for value in [
        "automatic_balanced",
        "priority_fallback",
        "stable_first",
        "backup_only",
        "cheap_first",
        "cost_stable_first",
    ] {
        assert!(adr.contains(value), "ADR mapping missing {value}");
    }
    assert!(source.contains("routing_configuration_required"));
    assert!(adr.contains("Intentionally lost semantics"));
}

fn read_source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .unwrap_or_else(|error| panic!("failed to read {relative}: {error}"))
}
