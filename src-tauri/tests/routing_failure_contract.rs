use relay_pool_desktop_lib::test_support::contract_scenarios;

#[test]
fn canonical_routing_failure_semantics_use_the_real_crate_graph() {
    contract_scenarios::routing_failure_semantics();
}

#[test]
fn route_planning_failures_keep_stable_public_mappings_in_the_real_crate_graph() {
    contract_scenarios::route_planning_failures_keep_stable_public_mappings();
}
