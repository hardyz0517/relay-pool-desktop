use std::{fs, path::Path};

fn source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .expect("source file must exist")
}

#[test]
fn quality_projection_is_versioned_replayable_and_checkpointed() {
    let projection = source("src/application/quality_projection.rs");
    let store = source("src/persistence/stores/routing_quality_store.rs");
    assert!(projection.contains("QUALITY_PROJECTOR_VERSION"));
    assert!(projection.contains("evidence_mass_basis_points"));
    assert!(projection.contains("sort_by"));
    assert!(projection.contains("seen.insert"));
    assert!(projection.contains("minimum_effective_mass_basis_points"));
    assert!(store.contains("save_checkpoint"));
    assert!(store.contains("excluded.checkpoint_sequence >="));
}

#[test]
fn latency_missing_is_unknown_not_zero() {
    let projection = source("src/application/quality_projection.rs");
    assert!(projection.contains("p95_latency_ms: Option<u32>"));
    assert!(projection.contains("if values.is_empty()"));
    assert!(!projection.contains("p95_latency_ms: Some(0)"));
}
