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

#[cfg(test)]
mod tests {
    use super::{
        create_installation_marker, installation_marker_exists, read_config, write_config,
        write_config_with_replace_failure_for_test, DataDirConfigV2,
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
