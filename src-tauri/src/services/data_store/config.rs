use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::persistence::upgrade_fault::{AtomicStep, UpgradeFailpoint, UpgradeFaultInjector};

use crate::services::data_store::atomic_file::{
    create_new_file, replace_existing_file as atomic_replace_existing_file, sync_parent,
    unique_sibling, AtomicFileError,
};

const INSTALLATION_MARKER_FILE: &str = "installation.marker";

pub fn installation_marker_exists(default_data_dir: &Path) -> bool {
    default_data_dir.join(INSTALLATION_MARKER_FILE).is_file()
}

pub fn create_installation_marker(default_data_dir: &Path) -> Result<(), String> {
    fs::create_dir_all(default_data_dir).map_err(|error| {
        format!(
            "无法创建安装标记目录 {}: {error}",
            default_data_dir.display()
        )
    })?;
    let marker_path = default_data_dir.join(INSTALLATION_MARKER_FILE);
    let file = File::create(&marker_path)
        .map_err(|error| format!("无法创建安装标记 {}: {error}", marker_path.display()))?;
    file.sync_all()
        .map_err(|error| format!("无法同步安装标记 {}: {error}", marker_path.display()))?;
    sync_parent(default_data_dir).map_err(|error| {
        format!(
            "failed to sync installation marker directory {}: {error}",
            default_data_dir.display()
        )
    })
}

fn write_config_v3_inner(
    config_path: &Path,
    config: &DataDirConfigV3,
    mut check: impl FnMut(AtomicStep) -> Result<(), String>,
) -> Result<(), String> {
    let parent = config_path
        .parent()
        .ok_or_else(|| format!("config path has no parent: {}", config_path.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "failed to create config directory {}: {error}",
            parent.display()
        )
    })?;
    let temp_path = unique_sibling(config_path, "config");
    let raw = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("failed to serialize V3 config: {error}"))?;
    check(AtomicStep::BeforeWrite)?;
    {
        let mut file = create_new_file(&temp_path).map_err(|error| {
            format!(
                "failed to create temporary config {}: {error}",
                temp_path.display()
            )
        })?;
        use std::io::Write;
        file.write_all(&raw).map_err(|error| {
            format!(
                "failed to write temporary config {}: {error}",
                temp_path.display()
            )
        })?;
        check(AtomicStep::BeforeFileSync)?;
        file.sync_all().map_err(|error| {
            format!(
                "failed to sync temporary config {}: {error}",
                temp_path.display()
            )
        })?;
    }
    check(AtomicStep::BeforeReplace)?;
    replace_config_file(&temp_path, config_path)?;
    check(AtomicStep::AfterReplaceBeforeParentSync)?;
    sync_parent(parent).map_err(|error| {
        format!(
            "failed to sync config directory {}: {error}",
            parent.display()
        )
    })?;
    check(AtomicStep::AfterDurableSync)
}

fn replace_config_file(temp_path: &Path, config_path: &Path) -> Result<(), String> {
    if !config_path.exists() {
        return fs::rename(temp_path, config_path)
            .map_err(|error| format!("无法创建数据目录配置 {}: {error}", config_path.display()));
    }
    atomic_replace_existing_file(temp_path, config_path).map_err(|error| {
        format!(
            "无法替换数据目录配置 {}: {}",
            config_path.display(),
            atomic_error_label(&error)
        )
    })
}

fn atomic_error_label(error: &AtomicFileError) -> String {
    match error {
        AtomicFileError::Io(error) => error.to_string(),
        other => other.to_string(),
    }
}

/// The only database generation understood by this binary.
///
/// The legacy generation-1 store has been retired. Keeping a closed enum here
/// means future config values still fail closed instead of being interpreted as
/// a compatible database by accident.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DatabaseGeneration {
    Two,
}

impl DatabaseGeneration {
    pub(crate) const fn database_file(self) -> &'static str {
        "relay-pool-desktop-v2.sqlite3"
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DataDirConfigV3 {
    pub version: u32,
    pub active_data_dir: Option<PathBuf>,
    pub pending_data_dir: Option<PathBuf>,
    pub source_data_dir: Option<PathBuf>,
    pub database_generation: DatabaseGeneration,
    pub updated_at: String,
}

/// Read and normalize every supported on-disk config shape.
///
/// Only the current V3 config is writable. Historical config shapes are
/// rejected so a retired generation-1 database can never be opened silently.
pub(crate) fn read_config_v3(config_path: &Path) -> Result<Option<DataDirConfigV3>, String> {
    if !config_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(config_path).map_err(|error| {
        format!(
            "failed to read data directory config {}: {error}",
            config_path.display()
        )
    })?;
    let value: Value = serde_json::from_str(&raw).map_err(|error| {
        format!(
            "failed to parse data directory config {}: {error}",
            config_path.display()
        )
    })?;
    let version = value.get("version").and_then(Value::as_u64);
    let config = match version {
        Some(3) => {
            let config = serde_json::from_value::<DataDirConfigV3>(value)
                .map_err(|error| format!("failed to decode V3 data directory config: {error}"))?;
            validate_paths(&config)?;
            config
        }
        Some(2) | Some(1) | None => {
            return Err(
                "legacy data directory config is no longer supported; use a generation 2 database"
                    .to_string(),
            )
        }
        Some(other) => return Err(format!("unsupported data directory config version {other}")),
    };
    Ok(Some(config))
}

pub(crate) fn write_config_v3(config_path: &Path, config: &DataDirConfigV3) -> Result<(), String> {
    if config.version != 3 {
        return Err("V3 data directory config must have version 3".to_string());
    }
    validate_paths(config)?;
    write_config_v3_inner(config_path, config, |_| Ok(()))
}

pub(crate) fn write_config_v3_with_faults(
    config_path: &Path,
    config: &DataDirConfigV3,
    faults: &dyn UpgradeFaultInjector,
) -> Result<(), String> {
    if config.version != 3 {
        return Err("V3 data directory config must have version 3".to_string());
    }
    validate_paths(config)?;
    write_config_v3_inner(config_path, config, |edge| {
        faults
            .check(UpgradeFailpoint::ConfigCommit(edge))
            .map_err(|error| error.to_string())
    })
}

fn validate_paths(config: &DataDirConfigV3) -> Result<(), String> {
    validate_path_locations(config)?;
    if config.pending_data_dir.is_some() != config.source_data_dir.is_some() {
        return Err("pendingDataDir and sourceDataDir must be provided together".to_string());
    }
    Ok(())
}

fn validate_path_locations(config: &DataDirConfigV3) -> Result<(), String> {
    for (name, path) in [
        ("activeDataDir", config.active_data_dir.as_ref()),
        ("pendingDataDir", config.pending_data_dir.as_ref()),
        ("sourceDataDir", config.source_data_dir.as_ref()),
    ] {
        if let Some(path) = path {
            if !path.is_absolute() {
                return Err(format!("{name} must be an absolute path"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        create_installation_marker, installation_marker_exists, read_config_v3, write_config_v3,
        write_config_v3_with_faults, DataDirConfigV3, DatabaseGeneration,
    };
    use crate::persistence::upgrade_fault::{
        AtomicStep, UpgradeFailpoint, UpgradeFaultInjector, UpgradeInjectedFailure,
        UPGRADE_INJECTED_FAILURE_CODE,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn unknown_config_version_fails_closed() {
        let (_root, config_path) = config_path("unknown-version");
        fs::write(
            &config_path,
            r#"{"version":99,"activeDataDir":"C:/future"}"#,
        )
        .expect("config");
        assert!(read_config_v3(&config_path).is_err());
    }

    #[test]
    fn v3_generation_and_relocation_fields_round_trip_atomically() {
        let (root, config_path) = config_path("v3-round-trip");
        let config = DataDirConfigV3 {
            version: 3,
            active_data_dir: Some(root.join("active")),
            pending_data_dir: None,
            source_data_dir: None,
            database_generation: DatabaseGeneration::Two,
            updated_at: "2026-07-20T00:00:00Z".to_string(),
        };
        write_config_v3(&config_path, &config).expect("write V3 config");
        assert_eq!(
            read_config_v3(&config_path)
                .expect("read")
                .expect("present"),
            config
        );
    }

    #[test]
    fn v3_fault_aware_writer_exposes_every_atomic_edge() {
        for (index, edge) in AtomicStep::ALL.into_iter().enumerate() {
            let (root, config_path) = config_path(&format!("v3-fault-edge-{index}"));
            let old_config = DataDirConfigV3 {
                version: 3,
                active_data_dir: Some(root.join("old-active")),
                pending_data_dir: None,
                source_data_dir: None,
                database_generation: DatabaseGeneration::Two,
                updated_at: "2026-07-20T00:00:00Z".to_string(),
            };
            let new_config = DataDirConfigV3 {
                active_data_dir: Some(root.join("new-active")),
                database_generation: DatabaseGeneration::Two,
                updated_at: "2026-07-21T00:00:00Z".to_string(),
                ..old_config.clone()
            };
            write_config_v3(&config_path, &old_config).expect("write old config");

            let error =
                write_config_v3_with_faults(&config_path, &new_config, &FaultAtAtomicEdge(edge))
                    .expect_err("inject atomic config failure");

            assert!(error.contains(UPGRADE_INJECTED_FAILURE_CODE));
            let observed = read_config_v3(&config_path)
                .expect("read config after injected failure")
                .expect("config remains present");
            let expected = match edge {
                AtomicStep::BeforeWrite
                | AtomicStep::BeforeFileSync
                | AtomicStep::BeforeReplace => &old_config,
                AtomicStep::AfterReplaceBeforeParentSync | AtomicStep::AfterDurableSync => {
                    &new_config
                }
            };
            assert_eq!(&observed, expected);
        }
    }

    #[test]
    fn v3_writer_rejects_unpaired_relocation_endpoints() {
        let (root, config_path) = config_path("v3-unpaired-relocation");
        let config = DataDirConfigV3 {
            version: 3,
            active_data_dir: None,
            pending_data_dir: None,
            source_data_dir: Some(root.join("source")),
            database_generation: DatabaseGeneration::Two,
            updated_at: "2026-07-20T00:00:00Z".to_string(),
        };

        let error = write_config_v3(&config_path, &config).expect_err("reject unpaired config");

        assert!(error.contains("must be provided together"));
        assert!(!config_path.exists());
    }

    #[test]
    fn installation_marker_is_created_only_after_success() {
        let root = temp_root("marker");
        fs::create_dir_all(&root).expect("root");

        assert!(!installation_marker_exists(&root));
        create_installation_marker(&root).expect("create marker");
        assert!(installation_marker_exists(&root));
        assert_eq!(
            fs::read(root.join("installation.marker")).expect("marker"),
            b""
        );
    }

    fn config_path(name: &str) -> (PathBuf, PathBuf) {
        let root = temp_root(name);
        fs::create_dir_all(&root).expect("root");
        let config_path = root.join("relay-pool-data-dir.json");
        (root, config_path)
    }

    fn temp_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("relay-pool-data-store-{name}-{unique}"))
    }

    struct FaultAtAtomicEdge(AtomicStep);

    impl UpgradeFaultInjector for FaultAtAtomicEdge {
        fn check(&self, failpoint: UpgradeFailpoint) -> Result<(), UpgradeInjectedFailure> {
            if failpoint == UpgradeFailpoint::ConfigCommit(self.0) {
                return Err(UpgradeInjectedFailure::new(failpoint));
            }
            Ok(())
        }
    }
}
