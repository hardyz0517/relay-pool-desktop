//! Runtime diagnostics lifecycle qualification against a real temporary root.
//!
//! Native window startup is intentionally outside this test. The scenarios
//! exercise the production bootstrap/shutdown composition, JSONL writer and
//! crash marker directly without a Tauri runtime or user interaction.

use relay_pool_desktop_lib::test_support::runtime_lifecycle_scenarios;

#[tokio::test]
async fn shutdown_failure_does_not_skip_marker_cleanup_or_final_jsonl_flush() {
    let root = tempfile::tempdir().expect("runtime root");
    runtime_lifecycle_scenarios::shutdown_failure_still_flushes_events_and_cleans_marker(
        root.path(),
    )
    .await;
}

#[tokio::test]
async fn unclean_restart_emits_recovery_evidence_and_clean_shutdown_resets_marker() {
    let root = tempfile::tempdir().expect("runtime root");
    runtime_lifecycle_scenarios::unclean_restart_records_recovery_and_clean_shutdown_resets_marker(
        root.path(),
    )
    .await;
}

#[tokio::test]
async fn crash_marker_open_fault_is_visible_without_blocking_runtime_log_shutdown() {
    let root = tempfile::tempdir().expect("runtime root");
    runtime_lifecycle_scenarios::marker_open_fault_is_durable_and_does_not_prevent_shutdown(
        root.path(),
    )
    .await;
}
