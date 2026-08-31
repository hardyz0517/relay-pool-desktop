use std::{fs, path::Path};

fn source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).expect("source")
}

#[test]
fn v3_proxy_execution_owns_bounded_replanning_and_fallback() {
    let execution = source("src/services/proxy/execution.rs");
    assert!(execution.contains("MAX_EXECUTION_REPLANS"));
    assert!(execution.contains("allows_replan"));
    assert!(execution.contains("execution_replan_limit_exceeded"));
    assert!(execution.contains("replan_required"));
    assert!(!execution.contains("routing_engine::coordinator"));
}

#[test]
fn execution_target_keeps_secret_resolution_after_revision_fence() {
    let resolver = source("src/application/operational_facts/target_resolver.rs");
    assert!(resolver.contains("StaleTarget"));
    assert!(resolver.contains("StaleCredentialRef"));
    assert!(resolver.contains("resolve_station_key_secret_ref"));
}
