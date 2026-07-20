use std::{
    fs::{self, File},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use serde_json::Value;

const INSTALLATION_MARKER_FILE: &str = "installation.marker";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DataDirConfigV2 {
    pub version: u32,
    pub active_data_dir: Option<PathBuf>,
    pub pending_data_dir: Option<PathBuf>,
    pub source_data_dir: Option<PathBuf>,
    pub updated_at: String,
}

pub fn read_config(config_path: &Path) -> Result<Option<DataDirConfigV2>, String> {
    read_config_strict(config_path)
}

#[allow(dead_code)]
fn read_config_legacy_unstrict(config_path: &Path) -> Result<Option<DataDirConfigV2>, String> {
    if !config_path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(config_path)
        .map_err(|error| format!("读取数据目录配置 {} 失败: {error}", config_path.display()))?;
    let value: Value = serde_json::from_str(&raw)
        .map_err(|error| format!("解析数据目录配置 {} 失败: {error}", config_path.display()))?;
    if value.get("version").and_then(Value::as_u64) == Some(2) {
        return serde_json::from_value(value)
            .map(Some)
            .map_err(|error| format!("解析数据目录配置 {} 失败: {error}", config_path.display()));
    }
    Ok(Some(DataDirConfigV2 {
        version: 1,
        active_data_dir: None,
        pending_data_dir: read_optional_path(&value, "pendingDataDir"),
        source_data_dir: read_optional_path(&value, "sourceDataDir"),
        updated_at: String::new(),
    }))
}

pub fn write_config(config_path: &Path, config: &DataDirConfigV2) -> Result<(), String> {
    write_config_inner(config_path, config, false)
}

#[cfg(test)]
fn write_config_with_replace_failure_for_test(
    config_path: &Path,
    config: &DataDirConfigV2,
) -> Result<(), String> {
    write_config_inner(config_path, config, true)
}

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
    sync_parent_dir(default_data_dir);
    Ok(())
}

fn read_optional_path(value: &Value, field: &str) -> Option<PathBuf> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
}

fn write_config_inner(
    config_path: &Path,
    config: &DataDirConfigV2,
    fail_before_replace: bool,
) -> Result<(), String> {
    let parent = config_path
        .parent()
        .ok_or_else(|| format!("数据目录配置路径 {} 没有父目录", config_path.display()))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("无法创建数据目录配置目录 {}: {error}", parent.display()))?;
    let temp_path = temp_config_path(config_path);
    let raw = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("序列化数据目录配置失败: {error}"))?;
    {
        let mut file = File::create(&temp_path)
            .map_err(|error| format!("无法创建临时配置 {}: {error}", temp_path.display()))?;
        use std::io::Write;
        file.write_all(&raw)
            .map_err(|error| format!("无法写入临时配置 {}: {error}", temp_path.display()))?;
        file.sync_all()
            .map_err(|error| format!("无法同步临时配置 {}: {error}", temp_path.display()))?;
    }
    if fail_before_replace {
        return Err("injected replace failure".to_string());
    }
    replace_config_file(&temp_path, config_path)?;
    sync_parent_dir(parent);
    Ok(())
}

fn write_config_v3_inner(
    config_path: &Path,
    config: &DataDirConfigV3,
    fail_before_replace: bool,
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
    let temp_path = temp_config_path(config_path);
    let raw = serde_json::to_vec_pretty(config)
        .map_err(|error| format!("failed to serialize V3 config: {error}"))?;
    {
        let mut file = File::create(&temp_path).map_err(|error| {
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
        file.sync_all().map_err(|error| {
            format!(
                "failed to sync temporary config {}: {error}",
                temp_path.display()
            )
        })?;
    }
    if fail_before_replace {
        return Err("injected replace failure".to_string());
    }
    replace_config_file(&temp_path, config_path)?;
    sync_parent_dir(parent);
    Ok(())
}

fn read_config_strict(config_path: &Path) -> Result<Option<DataDirConfigV2>, String> {
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
    match value.get("version").and_then(Value::as_u64) {
        Some(2) => serde_json::from_value(value)
            .map(Some)
            .map_err(|error| format!("failed to decode V2 config: {error}")),
        Some(3) => {
            let config = serde_json::from_value::<DataDirConfigV3>(value)
                .map_err(|error| format!("failed to decode V3 config: {error}"))?;
            DataDirConfigV2::try_from(config).map(Some)
        }
        Some(other) => Err(format!("unsupported data directory config version {other}")),
        None => Ok(Some(DataDirConfigV2 {
            version: 1,
            active_data_dir: None,
            pending_data_dir: read_optional_path(&value, "pendingDataDir"),
            source_data_dir: read_optional_path(&value, "sourceDataDir"),
            updated_at: String::new(),
        })),
    }
}

fn temp_config_path(config_path: &Path) -> PathBuf {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    config_path.with_extension(format!("tmp-{}-{unique}", std::process::id()))
}

fn replace_config_file(temp_path: &Path, config_path: &Path) -> Result<(), String> {
    if !config_path.exists() {
        return fs::rename(temp_path, config_path)
            .map_err(|error| format!("无法创建数据目录配置 {}: {error}", config_path.display()));
    }
    replace_existing_file(temp_path, config_path)
}

#[cfg(windows)]
fn replace_existing_file(temp_path: &Path, config_path: &Path) -> Result<(), String> {
    use std::ptr;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let replaced = wide_null(config_path.as_os_str());
    let replacement = wide_null(temp_path.as_os_str());
    let ok = unsafe {
        ReplaceFileW(
            replaced.as_ptr(),
            replacement.as_ptr(),
            ptr::null(),
            0,
            ptr::null_mut(),
            ptr::null_mut(),
        )
    };
    if ok == 0 {
        return Err(format!(
            "无法替换数据目录配置 {}: {}",
            config_path.display(),
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(windows)]
fn wide_null(value: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;

    value.encode_wide().chain(Some(0)).collect()
}

#[cfg(not(windows))]
fn replace_existing_file(temp_path: &Path, config_path: &Path) -> Result<(), String> {
    fs::rename(temp_path, config_path)
        .map_err(|error| format!("无法替换数据目录配置 {}: {error}", config_path.display()))
}

fn sync_parent_dir(parent: &Path) {
    let _ = File::open(parent).and_then(|file| file.sync_all());
}

/// The only database generations understood by this binary.
///
/// This is intentionally an enum instead of a free-form integer: a config
/// written by a future release must enter recovery rather than being opened
/// with an accidentally incompatible filename or schema.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum DatabaseGeneration {
    One,
    Two,
}

impl DatabaseGeneration {
    pub(crate) const fn database_file(self) -> &'static str {
        match self {
            Self::One => "relay-pool-desktop.sqlite3",
            Self::Two => "relay-pool-desktop-v2.sqlite3",
        }
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

impl TryFrom<DataDirConfigV2> for DataDirConfigV3 {
    type Error = String;

    fn try_from(value: DataDirConfigV2) -> Result<Self, Self::Error> {
        let config = Self {
            version: 3,
            active_data_dir: value.active_data_dir,
            pending_data_dir: value.pending_data_dir,
            source_data_dir: value.source_data_dir,
            database_generation: DatabaseGeneration::One,
            updated_at: value.updated_at,
        };
        validate_paths(&config)?;
        Ok(config)
    }
}

impl TryFrom<DataDirConfigV3> for DataDirConfigV2 {
    type Error = String;

    fn try_from(value: DataDirConfigV3) -> Result<Self, Self::Error> {
        if value.database_generation != DatabaseGeneration::One {
            return Err("generation 2 config cannot be represented as V2".to_string());
        }
        validate_paths(&value)?;
        Ok(Self {
            version: 2,
            active_data_dir: value.active_data_dir,
            pending_data_dir: value.pending_data_dir,
            source_data_dir: value.source_data_dir,
            updated_at: value.updated_at,
        })
    }
}

/// Read and normalize every supported on-disk config shape.
///
/// V1/legacy and V2 configs are treated as generation 1. Unknown versions,
/// malformed JSON, unknown generation values, and invalid relocation pairs are
/// rejected so startup can enter recovery without guessing a default store.
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
        Some(3) => serde_json::from_value::<DataDirConfigV3>(value)
            .map_err(|error| format!("failed to decode V3 data directory config: {error}"))?,
        Some(2) | Some(1) | None => {
            let legacy = decode_v2_value(&value)?;
            DataDirConfigV3::try_from(legacy)?
        }
        Some(other) => return Err(format!("unsupported data directory config version {other}")),
    };
    validate_paths(&config)?;
    Ok(Some(config))
}

pub(crate) fn write_config_v3(config_path: &Path, config: &DataDirConfigV3) -> Result<(), String> {
    if config.version != 3 {
        return Err("V3 data directory config must have version 3".to_string());
    }
    validate_paths(config)?;
    write_config_v3_inner(config_path, config, false)
}

fn decode_v2_value(value: &Value) -> Result<DataDirConfigV2, String> {
    if value.get("version").and_then(Value::as_u64) == Some(2) {
        return serde_json::from_value(value.clone())
            .map_err(|error| format!("failed to decode V2 data directory config: {error}"));
    }
    Ok(DataDirConfigV2 {
        version: 1,
        active_data_dir: None,
        pending_data_dir: read_optional_path(value, "pendingDataDir"),
        source_data_dir: read_optional_path(value, "sourceDataDir"),
        updated_at: String::new(),
    })
}

fn validate_paths(config: &DataDirConfigV3) -> Result<(), String> {
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
    if config.pending_data_dir.is_some() != config.source_data_dir.is_some() {
        return Err("pendingDataDir and sourceDataDir must be provided together".to_string());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        create_installation_marker, installation_marker_exists, read_config, read_config_v3,
        write_config, write_config_v3, write_config_with_replace_failure_for_test, DataDirConfigV2,
        DataDirConfigV3, DatabaseGeneration,
    };
    use std::{fs, path::PathBuf};

    #[test]
    fn reads_legacy_v1_pending_and_source_without_rewriting() {
        let (_root, config_path) = config_path("v1-config");
        fs::write(
            &config_path,
            r#"{"pendingDataDir":"C:/RelayPool/custom","sourceDataDir":"C:/RelayPool/default"}"#,
        )
        .expect("legacy config");

        let config = read_present(&config_path);

        assert_eq!(config.version, 1);
        assert_eq!(
            config.pending_data_dir,
            Some(PathBuf::from("C:/RelayPool/custom"))
        );
        assert_eq!(
            config.source_data_dir,
            Some(PathBuf::from("C:/RelayPool/default"))
        );
        assert!(fs::read_to_string(&config_path)
            .expect("config bytes")
            .contains("pendingDataDir"));
    }

    #[test]
    fn reads_v2_active_selection() {
        let (root, config_path) = config_path("v2-config");
        let active = root.join("active");
        write_config(&config_path, &v2_config(Some(active.clone()), None, None))
            .expect("write config");

        let config = read_present(&config_path);

        assert_eq!(config.version, 2);
        assert_eq!(config.active_data_dir, Some(active));
        assert_eq!(config.pending_data_dir, None);
        assert_eq!(config.source_data_dir, None);
    }

    #[test]
    fn truncated_json_is_rejected() {
        let (_root, config_path) = config_path("truncated-config");
        fs::write(&config_path, r#"{"version":2,"activeDataDir":"#).expect("truncated config");

        assert!(read_config(&config_path).is_err());
    }

    #[test]
    fn v2_config_upgrades_to_v3_without_losing_relocation_fields() {
        let (root, config_path) = config_path("v2-to-v3");
        let active = root.join("active");
        let pending = root.join("pending");
        let source = root.join("source");
        let v2 = v2_config(Some(active), Some(pending), Some(source));
        write_config(&config_path, &v2).expect("write config");

        let v3 = read_config_v3(&config_path)
            .expect("read config")
            .expect("config present");
        assert_eq!(v3.database_generation, DatabaseGeneration::One);
        assert_eq!(v3.active_data_dir, v2.active_data_dir);
        assert_eq!(v3.pending_data_dir, v2.pending_data_dir);
        assert_eq!(v3.source_data_dir, v2.source_data_dir);
    }

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
    fn failed_replace_preserves_previous_config_and_ignores_temp_file() {
        let (root, config_path) = config_path("failed-replace");
        let old_active = root.join("old-active");
        let new_active = root.join("new-active");
        write_config(
            &config_path,
            &v2_config(Some(old_active.clone()), None, None),
        )
        .expect("old config");

        let error = write_config_with_replace_failure_for_test(
            &config_path,
            &v2_config(Some(new_active), None, None),
        )
        .expect_err("injected failure");
        let config = read_present(&config_path);

        assert!(error.contains("injected replace failure"));
        assert_eq!(config.active_data_dir, Some(old_active));
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

    fn read_present(config_path: &PathBuf) -> DataDirConfigV2 {
        read_config(config_path)
            .expect("read config")
            .expect("config present")
    }

    fn v2_config(
        active_data_dir: Option<PathBuf>,
        pending_data_dir: Option<PathBuf>,
        source_data_dir: Option<PathBuf>,
    ) -> DataDirConfigV2 {
        DataDirConfigV2 {
            version: 2,
            active_data_dir,
            pending_data_dir,
            source_data_dir,
            updated_at: "2026-07-17T00:00:00Z".to_string(),
        }
    }

    fn temp_root(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!("relay-pool-data-store-{name}-{unique}"))
    }
}
