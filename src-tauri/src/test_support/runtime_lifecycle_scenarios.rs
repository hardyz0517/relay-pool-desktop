//! Executable lifecycle scenarios for the runtime diagnostics boundary.
//!
//! These deliberately avoid a Tauri runtime. They exercise the same
//! `RuntimeLogLifecycle` used by application setup and exit against a caller
//! supplied temporary runtime-log root.

use std::{collections::BTreeSet, io, path::Path};

use crate::{
    observability::runtime::{
        crash::CrashMarkerError,
        reader::{DEFAULT_PAGE_BYTES, DEFAULT_PAGE_LINES},
        RuntimeEvent, RuntimeLogReader,
    },
    runtime_composition::RuntimeLogLifecycle,
};

pub async fn shutdown_failure_still_flushes_events_and_cleans_marker(root: &Path) {
    let lifecycle = RuntimeLogLifecycle::open(root);
    lifecycle.record_startup();
    assert!(root.join("runtime-crash.marker").is_file());

    lifecycle.shutdown(|| async { Err(()) }).await;

    assert!(
        !root.join("runtime-crash.marker").exists(),
        "a drain failure must not skip marker cleanup"
    );
    let codes = read_codes(root);
    assert!(codes.contains("app.bootstrap.started"));
    assert!(codes.contains("app.shutdown.started"));
    assert!(codes.contains("app.shutdown.persistence_drain_failed"));
}

pub async fn unclean_restart_records_recovery_and_clean_shutdown_resets_marker(root: &Path) {
    {
        let lifecycle = RuntimeLogLifecycle::open(root);
        lifecycle.record_startup();
        lifecycle.service().flush();
        lifecycle.marker().expect("working marker").record_panic();
    }

    assert!(root.join("runtime-crash.marker").is_file());
    let restarted = RuntimeLogLifecycle::open(root);
    restarted.record_startup();
    restarted.shutdown(|| async { Ok(()) }).await;

    assert!(
        !root.join("runtime-crash.marker").exists(),
        "a clean restart must consume the previous crash marker"
    );
    assert!(read_codes(root).contains("app.previous_session_unclean_exit"));
}

pub async fn marker_open_fault_is_durable_and_does_not_prevent_shutdown(root: &Path) {
    let lifecycle = RuntimeLogLifecycle::open_with_marker_for_tests(root, |_| {
        Err(CrashMarkerError::Io(io::Error::other(
            "fixture marker failure",
        )))
    });
    lifecycle.record_startup();
    lifecycle.shutdown(|| async { Ok(()) }).await;

    assert!(!root.join("runtime-crash.marker").exists());
    let codes = read_codes(root);
    assert!(codes.contains("runtime.crash_marker.unavailable"));
    assert!(codes.contains("app.shutdown.started"));
}

fn read_codes(root: &Path) -> BTreeSet<String> {
    let page = RuntimeLogReader::new(root).read_page_with_cursor(
        0,
        0,
        DEFAULT_PAGE_LINES,
        DEFAULT_PAGE_BYTES,
    );
    assert!(page.issues.is_empty(), "reader issues: {:?}", page.issues);
    page.lines
        .iter()
        .map(|line| {
            serde_json::from_slice::<RuntimeEvent>(line.as_bytes())
                .expect("published runtime event")
                .event_code
                .as_str()
                .to_owned()
        })
        .collect()
}
