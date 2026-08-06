use std::{fs, path::Path};

fn source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).expect("source")
}

#[test]
fn coordinator_owns_replan_fallback_and_target_fence() {
    let coordinator = source("src/application/routing_engine/coordinator.rs");
    assert!(coordinator.contains("TargetFence"));
    assert!(coordinator.contains("AttemptExecutor"));
    assert!(coordinator.contains("ReplanLimit"));
    assert!(coordinator.contains("stale_target"));
    assert!(coordinator.contains("DecisionTraceRound"));
}

#[test]
fn execution_target_keeps_secret_resolution_after_revision_fence() {
    let resolver = source("src/application/operational_facts/target_resolver.rs");
    assert!(resolver.contains("StaleTarget"));
    assert!(resolver.contains("StaleCredentialRef"));
    assert!(resolver.contains("resolve_station_key_secret_ref"));
}
