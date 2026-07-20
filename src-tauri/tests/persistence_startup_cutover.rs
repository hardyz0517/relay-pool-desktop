//! Focused, dependency-light proof for the generation/config cutover.
//!
//! Full Tauri startup composition is intentionally tested at the application
//! boundary once Task 14 registers the V2 services. These tests keep the
//! durable config invariants executable without starting a GUI or SQLite
//! runtime.

mod config {
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/src/services/data_store/config.rs"
    ));
}

use config::{
    read_config_v3, write_config_v3, DataDirConfigV2, DataDirConfigV3, DatabaseGeneration,
};
use std::{fs, path::PathBuf};

#[test]
fn v2_to_v3_preserves_relocation_and_selects_generation_one() {
    let root = temp_root("v2-to-v3");
    let config_path = root.join("relay-pool-data-dir.json");
    let source = root.join("source");
    let pending = root.join("pending");
    let v2 = DataDirConfigV2 {
        version: 2,
        active_data_dir: Some(source.clone()),
        pending_data_dir: Some(pending.clone()),
        source_data_dir: Some(source.clone()),
        updated_at: "2026-07-20T00:00:00Z".to_string(),
    };
    fs::create_dir_all(&root).expect("root");
    fs::write(&config_path, serde_json::to_vec(&v2).expect("encode")).expect("config");

    let v3 = read_config_v3(&config_path)
        .expect("read config")
        .expect("config present");
    assert_eq!(v3.database_generation, DatabaseGeneration::One);
    assert_eq!(v3.active_data_dir, v2.active_data_dir);
    assert_eq!(v3.pending_data_dir, v2.pending_data_dir);
    assert_eq!(v3.source_data_dir, v2.source_data_dir);
}

#[test]
fn unknown_version_never_defaults_to_first_run() {
    let root = temp_root("unknown-version");
    let config_path = root.join("relay-pool-data-dir.json");
    fs::create_dir_all(&root).expect("root");
    fs::write(&config_path, br#"{"version":99}"#).expect("config");

    assert!(read_config_v3(&config_path).is_err());
}

#[test]
fn generation_two_uses_a_dedicated_database_filename_and_round_trips() {
    let root = temp_root("generation-two");
    let config_path = root.join("relay-pool-data-dir.json");
    let config = DataDirConfigV3 {
        version: 3,
        active_data_dir: Some(root.clone()),
        pending_data_dir: None,
        source_data_dir: None,
        database_generation: DatabaseGeneration::Two,
        updated_at: "2026-07-20T00:00:00Z".to_string(),
    };

    write_config_v3(&config_path, &config).expect("write V3");
    assert_eq!(
        DatabaseGeneration::Two.database_file(),
        "relay-pool-desktop-v2.sqlite3"
    );
    assert_eq!(
        read_config_v3(&config_path)
            .expect("read")
            .expect("present"),
        config
    );
}

fn temp_root(name: &str) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("time")
        .as_nanos();
    std::env::temp_dir().join(format!("relay-pool-startup-cutover-{name}-{unique}"))
}
