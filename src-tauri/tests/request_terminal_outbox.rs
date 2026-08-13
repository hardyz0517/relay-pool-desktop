use relay_pool_desktop_lib::test_support::terminal_outbox_scenarios;

#[tokio::test]
async fn expired_lease_is_reclaimed_after_crash_without_changing_canonical_payload() {
    terminal_outbox_scenarios::expired_lease_replays_without_payload_changes().await;
}

#[tokio::test]
async fn conflicting_terminal_payload_and_tampered_digest_fail_closed() {
    terminal_outbox_scenarios::collision_and_digest_tamper_fail_closed().await;
}
