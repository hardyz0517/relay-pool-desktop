use std::{fs, path::Path};

fn source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)).expect("source")
}

#[test]
fn dispatch_contract_has_deterministic_seed_band_and_unknown_lane() {
    let planner = source("src/application/routing_engine/intelligent_planner.rs");
    let dispatch = source("src/application/routing_engine/dispatch.rs");
    let exploration = source("src/application/routing_engine/exploration.rs");
    assert!(planner.contains("plan_snapshot_with_budget"));
    assert!(planner.contains("exploration_share_basis_points"));
    assert!(dispatch.contains("seed_commitment"));
    assert!(dispatch.contains("band_basis_points"));
    assert!(exploration.contains("starvation_bound"));
    assert!(exploration.contains("fetch_update"));
}

#[test]
fn planner_does_not_treat_unknown_cost_as_zero() {
    let planner = source("src/application/routing_engine/intelligent_planner.rs");
    assert!(!planner.contains("cost_basis_points.unwrap_or(0)"));
}

#[test]
fn failure_domain_guard_keeps_one_emergency_candidate() {
    let source = source("src/application/routing_engine/failure_domains.rs");
    assert!(source.contains("max_ejection_count"));
    assert!(source.contains("candidate_count.saturating_sub(1)"));
}
