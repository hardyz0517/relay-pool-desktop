use relay_pool_desktop_lib::test_support::contract_scenarios;

#[test]
fn request_lifecycle_allows_one_committed_terminal_record() {
    contract_scenarios::request_lifecycle_exactly_once();
}

#[test]
fn request_lifecycle_rejects_commit_before_attempt_start() {
    contract_scenarios::request_lifecycle_rejects_early_commit();
}

#[test]
fn attempt_lifecycle_separates_retry_from_health_effect() {
    contract_scenarios::attempt_lifecycle_keeps_retry_and_health_separate();
}
