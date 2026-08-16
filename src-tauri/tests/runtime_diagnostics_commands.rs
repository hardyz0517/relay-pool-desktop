//! Contract tests for the developer-only diagnostics boundary.
//!
//! The command implementation depends on Tauri state and the application's
//! settings facade, so this test keeps the executable part small and checks
//! the production command source for the mandatory gate. DTO semantics live
//! beside the production DTO in its unit tests.

use std::{fs, path::PathBuf};

fn command_source() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("commands")
        .join("runtime_diagnostics.rs");
    fs::read_to_string(path).expect("runtime diagnostics command source")
}

#[test]
fn diagnostics_commands_have_explicit_developer_gate_before_each_operation() {
    let source = command_source();
    assert_eq!(
        source
            .matches("ensure_developer_mode(&settings).await?")
            .count(),
        2,
        "read and export must each enforce the gate before touching diagnostics"
    );
    assert!(source.contains("fn ensure_developer_mode"));
    assert!(source.contains("CommandErrorCode::PermissionDenied"));
}

#[test]
fn support_bundle_command_does_not_return_the_selected_destination() {
    let source = command_source();
    assert!(!source.contains("path: path.display().to_string()"));
    assert!(!source.contains("path.display()"));
}
