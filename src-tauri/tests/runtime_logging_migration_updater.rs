//! Contract coverage for migration and updater command logging boundaries.
//!
//! These commands intentionally keep their public error DTOs separate from
//! runtime events. The source contract makes that separation reviewable: all
//! fallible migration/updater operations use the fixed failure adapter and no
//! dynamic error is printed or formatted at the command boundary.

use std::{fs, path::PathBuf};

fn command_source(name: &str) -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("commands")
        .join(name);
    fs::read_to_string(path).expect("command source")
}

fn service_source(path: &str) -> String {
    fs::read_to_string(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("services")
            .join(path),
    )
    .expect("service source")
}

#[test]
fn migration_commands_use_fixed_failure_events_without_dynamic_output() {
    let migration = command_source("data_migration.rs");
    let descriptors = service_source("portable_migration/runtime_events.rs");
    assert_eq!(
        migration.matches("bootstrap::record_failure(").count(),
        9,
        "every fallible migration operation must use the runtime failure adapter"
    );
    assert_eq!(
        migration.matches("runtime_events::export_failed()").count(),
        3
    );
    assert_eq!(
        migration
            .matches("runtime_events::inspect_failed()")
            .count(),
        3
    );
    assert_eq!(
        migration
            .matches("runtime_events::prepare_failed()")
            .count(),
        3
    );
    assert!(descriptors.contains("migration.portable.export_failed"));
    assert!(descriptors.contains("migration.portable.inspect_failed"));
    assert!(descriptors.contains("migration.portable.prepare_failed"));
    assert!(!migration.contains("println!("));
    assert!(!migration.contains("eprintln!("));
    assert!(!migration.contains("error = ?error"));
    assert!(!migration.contains("error = %error"));
}

#[test]
fn updater_manifest_command_uses_fixed_failure_event_and_shared_transport() {
    let updater = command_source("updater.rs");
    let descriptors = service_source("updater/runtime_events.rs");
    assert!(updater.contains("runtime_events::manifest_inspect_failed()"));
    assert!(descriptors.contains("updater.manifest.inspect_failed"));
    assert!(updater.contains("bootstrap::record_failure("));
    assert!(updater.contains("runtime.outbound"));
    assert!(!updater.contains("println!("));
    assert!(!updater.contains("eprintln!("));
    assert!(!updater.contains("error = ?error"));
    assert!(!updater.contains("error = %error"));
}
