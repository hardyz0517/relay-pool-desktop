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
fn request_finalization_uses_the_same_transaction_for_routing_observation() {
    let source = source("src/application/request_finalization/mod.rs");
    assert!(source.contains(".append(&mut session, observation)"));
    assert!(source.contains("routing_observation(\n                    &write"));
    assert!(source.contains("producer_sequence"));
}
