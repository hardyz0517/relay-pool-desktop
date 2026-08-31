use std::{fs, path::Path};

fn source(relative: &str) -> String {
    fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join(relative))
        .expect("source file must exist")
        .replace("\r\n", "\n")
}

#[test]
fn canonical_observation_writer_owns_taxonomy_and_ordering() {
    let ingestion = source("src/application/observation_ingestion.rs");
    assert!(ingestion.contains("pub(crate) async fn append"));
    assert!(ingestion.contains("RoutingObservationStore"));
    assert!(ingestion.contains("producer_sequence"));
    assert!(ingestion.contains("Sha256"));
    assert!(ingestion.contains("traffic_equivalence"));
    assert!(!ingestion.contains("api_key"));
}

#[test]
fn health_transition_has_one_durable_health_writer_and_no_status_writeback() {
    let transitions = source("src/application/health_transitions.rs");
    let store = source("src/persistence/stores/health_observation_store.rs");
    assert_eq!(transitions.matches("upsert_station_key_health").count(), 1);
    assert!(!transitions.contains("update_station_key_status"));
    assert!(!store.contains("update_station_key_status"));
    assert!(!store.contains("UPDATE station_keys\n            SET status"));
}

#[test]
fn observation_model_rejects_anonymous_probe_quality_success() {
    let model = source("src/models/routing_observation.rs");
    assert!(model.contains("anonymous probe cannot produce model quality success evidence"));
    assert!(model.contains("producer_sequence"));
    assert!(model.contains("evidence_mass_basis_points"));
}

#[test]
fn request_finalization_projects_routing_observation_through_durable_outbox() {
    let source = source("src/application/request_finalization/mod.rs");
    // v3 finalization first persists the canonical attempt and request
    // terminal, then projects the finalized cluster through the durable
    // outbox. The projection and outbox deletion must share one transaction,
    // so a crash can leave work to replay without duplicating observations.
    assert!(source.contains(".enqueue(session.connection(), &write, terminal_at_ms)"));
    assert!(source.contains(".delete_claimed(session.connection(), &record.request_id, &owner)"));
    assert!(source.contains("append_with_generation_eligibility"));
    assert!(source.contains("routing_observation_from_finalized(sample)?"));
    assert!(source.contains("producer_sequence"));
}
