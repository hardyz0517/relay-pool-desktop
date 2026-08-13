use relay_pool_desktop_lib::test_support::contract_scenarios;

#[test]
fn incomplete_stream_eof_is_attempt_failure_and_not_request_success() {
    contract_scenarios::incomplete_stream_is_not_request_success();
}

#[test]
fn downstream_drop_after_commit_records_failed_attempt_before_interrupted_request() {
    contract_scenarios::downstream_drop_is_interrupted_after_failed_attempt();
}
